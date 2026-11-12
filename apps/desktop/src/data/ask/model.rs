//! An agent's models and their effort settings. Mrow asks for Cursor's "parameterized" model
//! list, so its ids are plain ("claude-sonnet-5-5") and effort is a separate setting. opencode and
//! pi put the provider first ("anthropic/claude-sonnet-4-5").

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentModel {
    pub id: String,
    pub name: String,
}

pub const MAKERS: [&str; 6] = ["Cursor", "Anthropic", "OpenAI", "Google", "xAI", "Other"];

/// The short list the picker leads with: the newest of a few families, each with a line on when
/// it suits. Everything else is under "More Models".
pub const FEATURED: [(&str, &str); 5] = [
    ("auto", "Cursor picks a model for each question"),
    ("claude-opus", "Most capable, for the hardest questions"),
    ("claude-sonnet", "Smart and quick, for most reviews"),
    ("claude-haiku", "Fastest, for quick lookups"),
    ("composer", "Cursor's own model, built for code"),
];

fn is_version(part: &str) -> bool {
    !part.is_empty() && part.chars().all(|c| c.is_ascii_digit() || c == '.')
}

impl AgentModel {
    pub fn new(id: &str, name: &str) -> Self {
        AgentModel { id: id.into(), name: name.into() }
    }

    /// Ids from before the parameterized list carried the variant: "claude-sonnet-5-5[effort=high]".
    pub fn base(id: &str) -> &str {
        id.split('[').next().unwrap_or(id)
    }

    /// Cursor's own name ("Claude Sonnet 5.5"), or one made from the id when it sends none.
    pub fn title(&self) -> String {
        if self.name.is_empty() || self.name == self.id { pretty(&self.id) } else { self.name.clone() }
    }

    /// The title without the maker or provider, for the narrow composer button: "Sonnet 5.5".
    pub fn short_title(&self) -> String {
        let title = self.title();
        let title = title.rsplit_once('/').map(|(_, t)| t.to_string()).unwrap_or(title);
        title.strip_prefix("Claude ").map(str::to_string).unwrap_or(title)
    }

    /// The id without a provider: "anthropic/claude-opus-4-8" → "claude-opus-4-8".
    fn bare(&self) -> &str {
        self.id.rsplit_once('/').map_or(self.id.as_str(), |(_, id)| id)
    }

    pub fn maker(&self) -> &'static str {
        let id = self.id.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| id.contains(w));
        match self.bare().split('-').next().unwrap_or("") {
            "composer" | "auto" => "Cursor",
            _ if has(&["claude", "anthropic", "opus", "sonnet", "haiku", "fable"]) => "Anthropic",
            _ if has(&["gpt", "openai", "codex"]) => "OpenAI",
            _ if has(&["gemini", "google"]) => "Google",
            _ if has(&["grok", "xai"]) => "xAI",
            _ => "Other",
        }
    }

    /// The id without its version: "claude-opus-4-8" → "claude-opus", "gemini-3.1-pro" → "gemini-pro".
    pub fn family(&self) -> String {
        if self.id.starts_with("auto") {
            return "auto".into();
        }
        self.bare().split('-').filter(|p| !is_version(p)).collect::<Vec<_>>().join("-")
    }

    /// "claude-opus-4-8" → [4, 8], "gpt-5.4-mini" → [5, 4]: compared to find a family's newest.
    pub fn version(&self) -> Vec<i64> {
        self.bare().split('-').filter(|p| is_version(p)).flat_map(|p| p.split('.')).filter_map(|n| n.parse().ok()).collect()
    }

    pub fn newer(&self, other: &AgentModel) -> bool {
        let (x, y) = (self.version(), other.version());
        for i in 0..x.len().max(y.len()) {
            let (l, r) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
            if l != r {
                return l > r;
            }
        }
        false
    }

    /// This model's line in the short list, if its family has one.
    pub fn blurb(&self) -> Option<&'static str> {
        let family = self.family();
        FEATURED.iter().find(|(f, _)| *f == family).map(|(_, b)| *b)
    }
}

/// The newest model of each featured family, in the list's order.
pub fn featured(models: &[AgentModel]) -> Vec<AgentModel> {
    FEATURED
        .iter()
        .filter_map(|(family, _)| {
            models.iter().filter(|m| m.family() == *family).fold(None::<&AgentModel>, |best, m| match best {
                Some(b) if !m.newer(b) => Some(b),
                _ => Some(m),
            })
        })
        .cloned()
        .collect()
}

/// "claude-opus-4-8" → "Claude Opus 4.8", "gpt-5.3-codex" → "GPT-5.3 Codex".
pub fn pretty(id: &str) -> String {
    let parts: Vec<&str> = id.split('-').collect();
    let word = |p: &str| -> String {
        if p == "glm" {
            return "GLM".into();
        }
        let mut c = p.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    };
    if parts.first() == Some(&"gpt") && parts.len() > 1 {
        let mut words = vec![format!("GPT-{}", parts[1])];
        words.extend(parts[2..].iter().map(|p| word(p)));
        return words.join(" ");
    }
    let mut words: Vec<String> = Vec::new();
    for part in parts {
        match words.last_mut() {
            Some(last) if is_version(part) && is_version(last) => {
                last.push('.');
                last.push_str(part);
            }
            _ => words.push(if is_version(part) { part.to_string() } else { word(part) }),
        }
    }
    words.join(" ")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffortOption {
    pub value: String,
    pub name: String,
}

/// Whether a config option is the model picker: Cursor calls it "model", opencode gives it the
/// "model" category.
pub fn is_model_option(o: &Value) -> bool {
    o.get("id").and_then(Value::as_str) == Some("model") || o.get("category").and_then(Value::as_str) == Some("model")
}

/// A model's effort setting as the agent offers it. Its id differs by model ("reasoning_effort",
/// "effort", "reasoning"); `values` is empty for a model without one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffortOptions {
    pub config_id: String,
    pub values: Vec<EffortOption>,
    pub current: String,
}

impl EffortOptions {
    pub fn option(&self, value: Option<&str>) -> Option<&EffortOption> {
        let value = value?;
        self.values.iter().find(|o| o.value == value)
    }

    /// The model a session is on and its effort setting, from a `configOptions` snapshot.
    pub fn from(options: &[Value]) -> Option<(String, EffortOptions)> {
        let s = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
        let model = options.iter().find(|o| is_model_option(o)).and_then(|o| s(o, "currentValue"))?;
        let Some(thought) = options.iter().find(|o| s(o, "category").as_deref() == Some("thought_level")) else {
            return Some((model, EffortOptions { config_id: String::new(), values: vec![], current: String::new() }));
        };
        let Some(id) = s(thought, "id") else {
            return Some((model, EffortOptions { config_id: String::new(), values: vec![], current: String::new() }));
        };
        let values = thought
            .get("options")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|o| {
                        let value = s(o, "value")?;
                        Some(EffortOption { name: s(o, "name").unwrap_or_else(|| value.clone()), value })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some((model, EffortOptions { config_id: id, values, current: s(thought, "currentValue").unwrap_or_default() }))
    }
}
