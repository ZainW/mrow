//! A forge's command-line client (`glab` or `gh`), which holds the token in the keychain so this
//! app never sees it. Calls block; run them off the UI thread.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::prefs;

#[derive(Clone, Debug)]
pub struct Cli {
    /// "glab", "gh".
    pub name: &'static str,
    /// "GitLab", "GitHub", for messages.
    pub service: &'static str,
    pub env: &'static [(&'static str, &'static str)],
    pub login_command: &'static str,
    is_gitlab: bool,
}

/// Found once per CLI; the first fetches run at the same time, hence the lock.
static RESOLVED: LazyLock<Mutex<HashMap<&'static str, String>>> = LazyLock::new(Default::default);

/// Longer than any sane request; a stuck CLI (network, keychain prompt) is stopped after this.
const TIMEOUT: Duration = Duration::from_secs(60);

impl Cli {
    pub fn glab() -> Cli {
        Cli { name: "glab", service: "GitLab", env: &[("GLAB_NO_PROMPT", "1")], login_command: "glab auth login", is_gitlab: true }
    }

    pub fn gh() -> Cli {
        Cli {
            name: "gh",
            service: "GitHub",
            env: &[("GH_PROMPT_DISABLED", "1"), ("GH_NO_UPDATE_NOTIFIER", "1"), ("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1")],
            login_command: "gh auth login",
            is_gitlab: false,
        }
    }

    fn custom_path(&self) -> String {
        prefs::read(|p| if self.is_gitlab { p.glab_path.trim().to_string() } else { p.gh_path.trim().to_string() })
    }

    /// The host from Settings, blank for the CLI's default.
    pub fn host(&self) -> String {
        prefs::read(|p| if self.is_gitlab { p.hostname.trim().to_string() } else { p.github_host.trim().to_string() })
    }

    pub fn executable(&self) -> Result<String, String> {
        let custom = self.custom_path();
        if !custom.is_empty() {
            if !is_executable(&custom) {
                return Err(format!("{} isn't at {custom}. Fix the path in Settings.", self.name));
            }
            return Ok(custom);
        }
        if let Some(known) = RESOLVED.lock().unwrap().get(self.name) {
            return Ok(known.clone());
        }
        // An app opened from Finder gets a minimal PATH, so the usual places come first.
        let candidates = super::paths::tool_dirs();
        let found = candidates.iter().map(|d| d.join(self.name).to_string_lossy().into_owned()).find(|p| is_executable(p)).or_else(|| {
            let out = Command::new(super::paths::login_shell()).args(["-lc", &format!("command -v {}", self.name)]).stderr(Stdio::null()).output().ok()?;
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (!path.is_empty() && is_executable(&path)).then_some(path)
        });
        match found {
            Some(path) => {
                RESOLVED.lock().unwrap().insert(self.name, path.clone());
                Ok(path)
            }
            None => Err(format!("Couldn't find {0}. Install it ({1}) or set its path in Settings.", self.name, super::paths::install_hint(self.name))),
        }
    }

    /// `<cli> api …` against the host from Settings, returning the response body.
    pub fn api(&self, args: &[String], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let mut all = vec!["api".to_string()];
        let host = self.host();
        if !host.is_empty() {
            all.push("--hostname".into());
            all.push(host);
        }
        all.extend(args.iter().cloned());
        self.run(&all, input)
    }

    pub fn api_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let data = self.api(&[path.to_string()], None)?;
        serde_json::from_slice(&data).map_err(|e| format!("{} sent something unexpected: {e}", self.service))
    }

    /// A request with a JSON body, sent on stdin: no field parsing, no placeholders expanded
    /// inside it, and no limit on how long a comment can be.
    pub fn send(&self, path: &str, method: &str, json: &Value) -> Result<Vec<u8>, String> {
        let body = serde_json::to_vec(json).map_err(|e| e.to_string())?;
        let args: Vec<String> = ["-X", method, "-H", "Content-Type: application/json", "--input", "-", path].into_iter().map(String::from).collect();
        self.api(&args, Some(&body))
    }

    /// A GraphQL query, with string variables as `-f name=value` and numbers as `-F name=value`.
    pub fn graphql<T: DeserializeOwned>(&self, query: &str, vars: &[(&str, Option<String>)], numbers: &[(&str, i64)]) -> Result<T, String> {
        let mut args = vec!["graphql".to_string(), "-f".into(), format!("query={query}")];
        let mut vars: Vec<_> = vars.iter().collect();
        vars.sort_by_key(|(k, _)| *k);
        for (name, value) in vars {
            if let Some(value) = value {
                args.push("-f".into());
                args.push(format!("{name}={value}"));
            }
        }
        for (name, value) in numbers {
            args.push("-F".into());
            args.push(format!("{name}={value}"));
        }
        let data = self.api(&args, None)?;
        #[derive(serde::Deserialize)]
        struct Response<T> {
            data: Option<T>,
            errors: Option<Vec<GqlError>>,
        }
        #[derive(serde::Deserialize)]
        struct GqlError {
            message: String,
        }
        let response: Response<T> = serde_json::from_slice(&data).map_err(|e| format!("{} sent something unexpected: {e}", self.service))?;
        match (response.data, response.errors) {
            (Some(data), _) => Ok(data),
            (None, Some(errors)) if !errors.is_empty() => Err(errors.into_iter().map(|e| e.message).collect::<Vec<_>>().join("\n")),
            _ => Err(format!("{} returned no data.", self.service)),
        }
    }

    pub fn run(&self, args: &[String], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
        let path = self.executable()?;
        let mut cmd = Command::new(&path);
        cmd.args(args);
        cmd.env("PATH", super::paths::child_path()).env("NO_COLOR", "1");
        for (k, v) in self.env {
            cmd.env(k, v);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
        let mut child = cmd.spawn().map_err(|e| format!("Couldn't run {}: {e}", self.name))?;
        if let Some(input) = input {
            let mut stdin = child.stdin.take().unwrap();
            let input = input.to_vec();
            std::thread::spawn(move || {
                let _ = stdin.write_all(&input);
            });
        }
        // Read both pipes while it runs, so a large response can't fill a pipe and stall the CLI.
        let mut out = child.stdout.take().unwrap();
        let mut err = child.stderr.take().unwrap();
        let out_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            buf
        });
        let err_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf);
            buf
        });
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if start.elapsed() > TIMEOUT => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => return Err(format!("Couldn't run {}: {e}", self.name)),
            }
        };
        let stdout = out_thread.join().unwrap_or_default();
        let stderr = err_thread.join().unwrap_or_default();
        match status {
            None => Err(format!("{} didn't answer within a minute, so Mrow stopped it.", self.name)),
            Some(s) if s.success() => Ok(stdout),
            Some(s) => {
                let message = String::from_utf8_lossy(if stderr.is_empty() { &stdout } else { &stderr }).to_string();
                Err(self.tidy(&message, s.code().unwrap_or(-1)))
            }
        }
    }

    /// The CLIs' errors come boxed with ANSI art and blank lines; keep the words.
    pub fn tidy(&self, message: &str, status: i32) -> String {
        let lines: Vec<&str> = message.lines().map(str::trim).filter(|l| !l.is_empty() && *l != "ERROR").collect();
        if lines.is_empty() {
            return format!("{} exited with status {status}.", self.name);
        }
        let text = lines.join(" ");
        let lower = text.to_lowercase();
        // The HTTP status, not any "401" (merge request !401, line 4010…).
        let auth = lower.contains("unauthorized")
            || lower.contains("invalid_token")
            || lower.contains("bad credentials")
            || lower.contains("not logged in")
            || lower.contains("gh auth login");
        if auth {
            return format!("{} rejected {}'s token. Run `{}` in a terminal, then refresh.", self.service, self.name, self.login_command);
        }
        text
    }
}

fn is_executable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    Path::new(path).metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Pages through a REST list 100 at a time.
pub fn paged_rest<T: DeserializeOwned>(cli: &Cli, path: &str, max_pages: usize) -> Result<Vec<T>, String> {
    let mut all = Vec::new();
    let joiner = if path.contains('?') { '&' } else { '?' };
    for page in 1..=max_pages {
        let data = cli.api(&[format!("{path}{joiner}per_page=100&page={page}")], None)?;
        let items: Vec<T> = serde_json::from_slice(&data).map_err(|e| format!("{} sent something unexpected: {e}", cli.service))?;
        let n = items.len();
        all.extend(items);
        if n < 100 {
            break;
        }
    }
    Ok(all)
}

/// Runs jobs on their own threads and collects the results in order.
pub fn parallel<T: Send + 'static>(jobs: Vec<Box<dyn FnOnce() -> T + Send>>) -> Vec<T> {
    let handles: Vec<_> = jobs.into_iter().map(std::thread::spawn).collect();
    handles.into_iter().map(|h| h.join().expect("worker panicked")).collect()
}
