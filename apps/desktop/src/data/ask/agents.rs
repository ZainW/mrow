//! The agents a chat can run: Cursor, Claude and opencode over the Agent Client Protocol, and pi
//! over its own RPC mode. Each is found the same way: the command set in settings, else the
//! usual install places, else the login shell's `PATH`.
//!
//! What they may do is one list of tool kinds, `TOOL_KINDS`, applied two ways: each agent starts
//! with only its tools of those kinds where it has a setting for that, and Mrow allows the
//! permission requests of those kinds and declines the rest.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};

use serde_json::{Value, json};

use crate::data::prefs;

/// What a chat's agent may do, by ACP tool kind. Add "fetch" for web pages, "edit" to change the
/// clone, "execute" to run commands. Keep "execute" out while posting needs approval in the chat:
/// with it an agent could post through `gh` or `glab` itself.
pub const TOOL_KINDS: &[&str] = &["read", "search", "think"];

/// Each agent's own tools by kind: (kind, pi, opencode permission, Claude Code).
const NATIVE: [(&str, &[&str], &[&str], &[&str]); 5] = [
    ("read", &["read"], &["read"], &["Read", "NotebookRead"]),
    ("search", &["grep", "find", "ls"], &["grep", "glob", "list", "codesearch"], &["Grep", "Glob", "LS"]),
    ("fetch", &[], &["webfetch", "websearch"], &["WebFetch", "WebSearch"]),
    ("edit", &["edit", "write"], &["edit"], &["Edit", "Write", "MultiEdit", "NotebookEdit"]),
    ("execute", &["bash"], &["bash"], &["Bash", "BashOutput", "KillShell"]),
];

/// Whether a tool call of this ACP kind may run.
pub fn allows(kind: &str) -> bool {
    TOOL_KINDS.contains(&kind)
}

/// pi's `--tools`: "read,grep,find,ls".
pub fn pi_tools() -> String {
    NATIVE.iter().filter(|(k, ..)| allows(k)).flat_map(|(_, pi, ..)| pi.iter().copied()).collect::<Vec<_>>().join(",")
}

/// opencode's `permission` setting: everything denied but the allowed tools, and `.env` files
/// stay unread.
pub fn opencode_permissions() -> Value {
    let mut permission = serde_json::Map::new();
    permission.insert("*".into(), json!("deny"));
    for (_, _, tools, _) in NATIVE.iter().filter(|(k, ..)| allows(k)) {
        for tool in tools.iter() {
            let rule = if *tool == "read" { json!({"*": "allow", "*.env": "deny", "*.env.*": "deny", "*.env.example": "allow"}) } else { json!("allow") };
            permission.insert(tool.to_string(), rule);
        }
    }
    Value::Object(permission)
}

/// Claude Code's tools outside the allowed kinds, taken away when a session starts.
pub fn claude_disallowed() -> Vec<&'static str> {
    NATIVE.iter().filter(|(k, ..)| !allows(k)).flat_map(|(.., claude)| claude.iter().copied()).collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AgentKind {
    #[default]
    Cursor,
    Claude,
    Opencode,
    Pi,
}

impl AgentKind {
    pub const ALL: [AgentKind; 4] = [AgentKind::Cursor, AgentKind::Claude, AgentKind::Opencode, AgentKind::Pi];

    pub fn raw(self) -> &'static str {
        match self {
            AgentKind::Cursor => "cursor",
            AgentKind::Claude => "claude",
            AgentKind::Opencode => "opencode",
            AgentKind::Pi => "pi",
        }
    }

    pub fn from_raw(raw: &str) -> Option<AgentKind> {
        AgentKind::ALL.into_iter().find(|k| k.raw() == raw)
    }

    /// "Cursor", "Claude": for the header and the menus.
    pub fn name(self) -> &'static str {
        match self {
            AgentKind::Cursor => "Cursor",
            AgentKind::Claude => "Claude",
            AgentKind::Opencode => "opencode",
            AgentKind::Pi => "pi",
        }
    }

    /// The process, for errors: "Cursor's agent stopped."
    pub fn process(self) -> &'static str {
        match self {
            AgentKind::Cursor => "Cursor's agent",
            AgentKind::Claude => "Claude's agent",
            AgentKind::Opencode => "opencode",
            AgentKind::Pi => "pi",
        }
    }

    /// The model a new chat starts on before one is picked; blank for the agent's own default.
    pub fn default_model(self) -> &'static str {
        match self {
            AgentKind::Cursor => "claude-sonnet-5-5",
            _ => "",
        }
    }

    /// The line under its name in the agent menu.
    pub fn blurb(self) -> &'static str {
        match self {
            AgentKind::Cursor => "Cursor's agent",
            AgentKind::Claude => "Claude Code",
            AgentKind::Opencode => "opencode",
            AgentKind::Pi => "pi",
        }
    }

    /// What the agent can do in the clone, for the composer's tooltip.
    pub fn reach(self) -> &'static str {
        "It can read and search files. It can't edit them or run commands."
    }

    pub fn sign_in(self) -> &'static str {
        match self {
            AgentKind::Cursor => "Cursor isn't signed in. Run `agent login` in a terminal, then ask again.",
            AgentKind::Claude => "Claude isn't signed in. Run `claude` in a terminal and sign in with /login, then ask again.",
            AgentKind::Opencode => "opencode isn't signed in. Run `opencode auth login` in a terminal, then ask again.",
            AgentKind::Pi => "pi has no model it can use. Run `pi auth` in a terminal to sign in to a provider, then ask again.",
        }
    }

    /// The program and its arguments.
    pub fn command(self) -> Result<Vec<String>, String> {
        let custom = prefs::read(|p| match self {
            AgentKind::Cursor => p.agent_path.trim().to_string(),
            _ => p.agent_commands.get(self.raw()).map(|c| c.trim().to_string()).unwrap_or_default(),
        });
        if !custom.is_empty() {
            let mut words: Vec<String> = custom.split_whitespace().map(str::to_string).collect();
            if self == AgentKind::Cursor && words.len() == 1 {
                words.push("acp".into());
            }
            if !is_executable(Path::new(&words[0])) && find(&words[0]).is_none() {
                return Err(format!("{} isn't at {}. Fix its command in settings.json.", self.process(), words[0]));
            }
            return Ok(words);
        }
        let with = |program: String, args: &[&str]| std::iter::once(program).chain(args.iter().map(|a| a.to_string())).collect::<Vec<_>>();
        match self {
            AgentKind::Cursor => {
                let home = std::env::var("HOME").unwrap_or_default();
                let local = format!("{home}/.local/bin/agent");
                let found = if is_executable(Path::new(&local)) { Some(local) } else { find("cursor-agent") };
                found.map(|p| with(p, &["acp"])).ok_or_else(|| {
                    "Couldn't find Cursor's agent. Install it (curl https://cursor.com/install -fsS | bash) or set its path in Settings.".to_string()
                })
            }
            AgentKind::Claude => {
                if let Some(adapter) = find("claude-agent-acp").or_else(|| find("claude-code-acp")) {
                    return Ok(vec![adapter]);
                }
                // The adapter is a small npm package; npx fetches it the first time.
                find("npx").map(|npx| with(npx, &["-y", "@agentclientprotocol/claude-agent-acp"])).ok_or_else(|| {
                    "Couldn't find Claude's ACP adapter. Install it (npm i -g @agentclientprotocol/claude-agent-acp) or install Node.js so Mrow can run it with npx.".to_string()
                })
            }
            AgentKind::Opencode => find("opencode").map(|p| with(p, &["acp"])).ok_or_else(|| {
                "Couldn't find opencode. Install it (npm i -g opencode-ai), or set its command under agentCommands in settings.json.".to_string()
            }),
            AgentKind::Pi => {
                find("pi").map(|p| vec![p]).ok_or_else(|| "Couldn't find pi. Install it, or set its command under agentCommands in settings.json.".to_string())
            }
        }
    }
}

pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// A program by name: the tool folders first, then the login shell, which knows about version
/// managers (mise, asdf, nvm) that an app opened from the Dock or a launcher doesn't.
fn find(name: &str) -> Option<String> {
    static FOUND: LazyLock<Mutex<std::collections::HashMap<String, String>>> = LazyLock::new(Default::default);
    if name.contains('/') {
        return is_executable(Path::new(name)).then(|| name.to_string());
    }
    if let Some(known) = FOUND.lock().unwrap().get(name) {
        return Some(known.clone());
    }
    let found =
        crate::data::paths::tool_dirs().iter().map(|d| d.join(name)).find(|p| is_executable(p)).map(|p| p.to_string_lossy().into_owned()).or_else(|| {
            let out = Command::new(crate::data::paths::login_shell()).args(["-lc", &format!("command -v {name}")]).stderr(Stdio::null()).output().ok()?;
            let path = String::from_utf8_lossy(&out.stdout).trim().lines().last().unwrap_or("").to_string();
            (!path.is_empty() && is_executable(Path::new(&path))).then_some(path)
        })?;
    FOUND.lock().unwrap().insert(name.to_string(), found.clone());
    Some(found)
}
