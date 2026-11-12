//! The Swift app's Ask self-test checks, ported.

use super::citations::{dedent, expand};
use super::model::{AgentModel, EffortOptions, featured, pretty};

#[test]
fn citations_fill_from_the_clone() {
    let clone = std::env::temp_dir().join(format!("mrow-selftest-{}", std::process::id()));
    std::fs::create_dir_all(clone.join("src")).unwrap();
    std::fs::write(clone.join("src/a.ts"), "one\ntwo\nthree\nfour\n").unwrap();
    let c = Some(clone.as_path());
    assert_eq!(expand("See:\n```2:3:src/a.ts\n```\nso.", c), "See:\n```src/a.ts lines 2–3\ntwo\nthree\n```\nso.");
    assert_eq!(expand(&format!("```1:1:{}/src/a.ts\nmine\n```", clone.display()), c), "```src/a.ts line 1\nmine\n```");
    assert_eq!(expand("```4:9:src/gone.ts\n```", c), "`gone.ts` · lines 4–9\n");
    assert_eq!(expand("```ts\nlet a = 1\n```", c), "```ts\nlet a = 1\n```");
    assert_eq!(dedent("    a\n      b\n\n    c\n"), "a\n  b\n\nc\n");
    let _ = std::fs::remove_dir_all(&clone);
}

#[test]
fn agent_models_are_named_and_featured() {
    let catalog: Vec<AgentModel> = [
        "auto-smart",
        "claude-opus-4-8",
        "claude-opus-5-5",
        "claude-opus-5",
        "gpt-5.5",
        "claude-sonnet-5-5",
        "claude-haiku-4-5",
        "gemini-3.1-pro",
        "composer-2.5",
    ]
    .iter()
    .map(|id| AgentModel::new(id, ""))
    .collect();
    let ids: Vec<String> = featured(&catalog).into_iter().map(|m| m.id).collect();
    assert_eq!(ids, ["auto-smart", "claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-4-5", "composer-2.5"]);
    assert_eq!(AgentModel::new("gemini-3.1-pro", "").family(), "gemini-pro");
    let sonnet = AgentModel::new("claude-sonnet-5-5", "Claude Sonnet 5.5");
    assert_eq!((sonnet.title().as_str(), sonnet.short_title().as_str(), sonnet.maker()), ("Claude Sonnet 5.5", "Sonnet 5.5", "Anthropic"));
    assert_eq!(AgentModel::new("gpt-5.3-codex", "").title(), "GPT-5.3 Codex");
    assert_eq!(pretty("gemini-3.8-flash"), "Gemini 3.8 Flash");
    assert_eq!(pretty("claude-opus-4-8"), "Claude Opus 4.8");
    assert_eq!(pretty("glm-5p3"), "GLM 5p3");
    assert_eq!(AgentModel::base("claude-sonnet-5-5[context=300k,reasoning_effort=high]"), "claude-sonnet-5-5");
}

#[test]
fn effort_is_read_from_config_options() {
    let options = vec![
        serde_json::json!({"id": "model", "category": "model", "currentValue": "gpt-5.5"}),
        serde_json::json!({"id": "reasoning", "category": "thought_level", "currentValue": "medium",
            "options": [{"value": "none", "name": "None"}, {"value": "medium", "name": "Medium"}, {"value": "extra-high", "name": "Extra High"}]}),
        serde_json::json!({"id": "fast", "category": "model_config", "currentValue": "false"}),
    ];
    let (model, effort) = EffortOptions::from(&options).unwrap();
    assert_eq!((model.as_str(), effort.config_id.as_str(), effort.current.as_str()), ("gpt-5.5", "reasoning", "medium"));
    assert_eq!(effort.values.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["None", "Medium", "Extra High"]);
    assert!(EffortOptions::from(&options[..1]).unwrap().1.values.is_empty(), "model without an effort setting");
}

#[test]
fn tool_titles_drop_the_clone_path() {
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("code/app");
    let title = |t: String| super::tool_title(&serde_json::json!({"title": t}), Some(&home));
    assert_eq!(title("Read code/app/src/x.ts (1 - 9)".into()), "Read src/x.ts (1 - 9)");
    assert_eq!(title(format!("Read {}/src/x.ts", home.display())), "Read src/x.ts");
}

#[test]
fn checkouts_match_remotes() {
    let dir = std::env::temp_dir().join(format!("mrow-clone-{}", std::process::id()));
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join(".git/config"), "[remote \"origin\"]\n\turl = git@gitlab.com:Fieldnotes/Ledger.git\n").unwrap();
    assert!(super::checkouts::matches(&dir, "fieldnotes/ledger"));
    assert!(!super::checkouts::matches(&dir, "fieldnotes/hooks"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saved_chats_show_only_the_question() {
    let mut chat = super::Chat::new("x");
    chat.stream(super::EntryKind::Question, &format!("context…\n{}\n\nWhy?", super::context::END));
    chat.trim_context();
    assert_eq!(chat.entries.len(), 2);
    assert_eq!(chat.entries[0].kind, super::EntryKind::Context);
    assert_eq!(chat.entries[1].text, "Why?");
}

#[test]
fn models_come_from_settings_or_the_model_list() {
    // opencode: the models are a setting's choices, some in groups.
    let opencode = serde_json::json!({"configOptions": [
        {"id": "model", "category": "model", "currentValue": "openai/gpt-5.5", "options": [
            {"value": "claude-agent/sonnet", "name": "claude-agent/Sonnet 5.5"},
            {"group": "openai", "name": "OpenAI", "options": [{"value": "openai/gpt-5.5", "name": "openai/GPT-5.5"}]}
        ]},
        {"id": "mode", "category": "mode", "currentValue": "build", "options": [{"value": "build", "name": "Build"}, {"value": "plan", "name": "Plan"}]}
    ]});
    let ids: Vec<String> = super::models_in(&opencode).into_iter().map(|m| m.id).collect();
    assert_eq!(ids, ["claude-agent/sonnet", "openai/gpt-5.5"]);

    // Claude's adapter: a model list beside the settings, and session modes.
    let claude = serde_json::json!({
        "models": {"currentModelId": "default", "availableModels": [{"modelId": "default", "name": "Default"}, {"modelId": "opus", "name": "Opus"}]},
        "modes": {"currentModeId": "default", "availableModes": [{"id": "default"}, {"id": "acceptEdits"}, {"id": "plan"}]}
    });
    assert_eq!(super::models_in(&claude).len(), 2);
    let (model, effort) = EffortOptions::from(&super::options(&claude)).unwrap();
    assert_eq!(model, "default");
    assert!(effort.values.is_empty());
}

#[test]
fn tools_follow_one_list() {
    assert_eq!(super::agents::pi_tools(), "read,grep,find,ls");
    let claude = super::agents::claude_disallowed();
    assert!(claude.contains(&"Bash") && claude.contains(&"Edit") && !claude.contains(&"Read"));
    let oc = super::agents::opencode_permissions();
    assert_eq!((oc["*"].as_str(), oc["bash"].as_str(), oc["grep"].as_str()), (Some("deny"), None, Some("allow")));
    assert_eq!(oc["read"]["*.env"], "deny");
}

#[test]
fn provider_prefixed_models_group_by_maker() {
    let pi = AgentModel::new("anthropic/claude-opus-4-8", "Claude Opus 4.8");
    assert_eq!((pi.maker(), pi.family().as_str(), pi.version()), ("Anthropic", "claude-opus", vec![4, 8]));
    assert_eq!(AgentModel::new("claude-agent/sonnet", "claude-agent/Sonnet 5.5").short_title(), "Sonnet 5.5");
    assert_eq!(AgentModel::new("openai/gpt-6-sol", "").maker(), "OpenAI");
    assert_eq!(AgentModel::new("opencode/big-pickle", "").maker(), "Other");
}

#[test]
fn pi_tool_calls_read_like_the_others() {
    let line = |name: &str, args: serde_json::Value| super::pi::tool_line(name, &args);
    assert_eq!(line("read", serde_json::json!({"path": "src/x.rs"})), ("Read src/x.rs".to_string(), "read"));
    assert_eq!(line("grep", serde_json::json!({"pattern": "fn main", "path": "src"})), ("Grep \"fn main\" in src".to_string(), "search"));
    assert_eq!(line("ls", serde_json::json!({})), ("List".to_string(), "search"));
}
