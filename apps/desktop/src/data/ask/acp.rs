//! Cursor's agent, run as `agent acp` and spoken to over the Agent Client Protocol: JSON-RPC, one
//! message per line on its stdin and stdout. One process serves every chat.
//!
//! Mrow offers it no file or terminal access of its own, and declines anything it asks
//! permission for, on top of the chats running in Cursor's read-only ask mode.
//!
//! Requests block their (worker) thread until the reply; session updates go to the UI thread
//! through the sink, with a wake.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, LazyLock, Mutex};

use rune::Waker;
use serde_json::{Value, json};

use crate::data::prefs;

/// A session update (streamed text, a tool call) for the UI thread.
pub struct Update {
    pub session: String,
    pub update: Value,
}

#[derive(Default)]
struct State {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    next_id: i64,
    waiting: HashMap<i64, Sender<Result<Value, String>>>,
    errors: String,
    declined: usize,
    /// Bumped per launch, so a stale reader's exit doesn't end a newer process's requests.
    generation: u64,
}

pub struct Client {
    state: Mutex<State>,
    /// Held while starting, so concurrent first requests wait for one launch.
    starting: Mutex<bool>,
    sink: Mutex<Option<(Sender<Update>, Option<Waker>)>>,
}

pub static CLIENT: LazyLock<Arc<Client>> =
    LazyLock::new(|| Arc::new(Client { state: Mutex::new(State::default()), starting: Mutex::new(false), sink: Mutex::new(None) }));

impl Client {
    pub fn set_sink(&self, tx: Sender<Update>, waker: Option<Waker>) {
        *self.sink.lock().unwrap() = Some((tx, waker));
    }

    /// How many permission requests have been declined since the agent started.
    pub fn declined(&self) -> usize {
        self.state.lock().unwrap().declined
    }

    /// Sends a request and waits for its reply. Starts the agent if it isn't running.
    pub fn request(self: &Arc<Self>, method: &str, params: Value) -> Result<Value, String> {
        self.start()?;
        self.send(method, params)
    }

    pub fn notify(&self, method: &str, params: Value) {
        self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn send(&self, method: &str, params: Value) -> Result<Value, String> {
        let (tx, rx) = channel();
        let id = {
            let mut s = self.state.lock().unwrap();
            if s.stdin.is_none() {
                return Err("Cursor's agent isn't running.".into());
            }
            s.next_id += 1;
            let id = s.next_id;
            s.waiting.insert(id, tx);
            id
        };
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        rx.recv().unwrap_or_else(|_| Err("Cursor's agent stopped.".into()))
    }

    fn write(&self, message: &Value) {
        let mut line = message.to_string();
        line.push('\n');
        log(">> ", &line);
        let mut s = self.state.lock().unwrap();
        if let Some(stdin) = s.stdin.as_mut() {
            if stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()).is_err() {
                s.stdin = None;
            }
        }
    }

    /// Stops the agent (when quitting); the next request starts a new one.
    pub fn stop(&self) {
        let mut s = self.state.lock().unwrap();
        s.stdin = None;
        if let Some(mut child) = s.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn start(self: &Arc<Self>) -> Result<(), String> {
        let mut started = self.starting.lock().unwrap();
        if *started && self.state.lock().unwrap().stdin.is_some() {
            return Ok(());
        }
        *started = false;
        self.launch()?;
        let reply = self.send(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": {"readTextFile": false, "writeTextFile": false},
                    "terminal": false,
                    // Plain model ids, with effort as a separate setting.
                    "_meta": {"parameterizedModelPicker": true},
                },
                "clientInfo": {"name": "Mrow", "version": "1.0"},
            }),
        );
        if let Err(e) = reply {
            self.stop();
            return Err(e);
        }
        *started = true;
        Ok(())
    }

    fn launch(self: &Arc<Self>) -> Result<(), String> {
        let path = executable()?;
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        let path_env = crate::data::paths::child_path();
        let mut child = Command::new(&path)
            .arg("acp")
            .current_dir(&home)
            .env("PATH", path_env)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't start Cursor's agent at {path}: {e}"))?;
        let stdout = child.stdout.take().ok_or("No output from Cursor's agent.")?;
        let mut stderr = child.stderr.take().ok_or("No error output from Cursor's agent.")?;
        let generation = {
            let mut s = self.state.lock().unwrap();
            s.generation += 1;
            s.stdin = child.stdin.take();
            s.child = Some(child);
            s.errors.clear();
            s.generation
        };
        let me = self.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                let mut s = me.state.lock().unwrap();
                s.errors.push_str(&String::from_utf8_lossy(&buffer[..n]));
                // Kept for error messages: the last couple of kilobytes is enough.
                if s.errors.len() > 2000 {
                    let mut cut = s.errors.len() - 2000;
                    while !s.errors.is_char_boundary(cut) {
                        cut += 1;
                    }
                    s.errors.drain(..cut);
                }
            }
        });
        let me = self.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.split(b'\n') {
                let Ok(line) = line else { break };
                if line.is_empty() {
                    continue;
                }
                let text = String::from_utf8_lossy(&line);
                log("<< ", &format!("{text}\n"));
                if let Ok(message) = serde_json::from_str::<Value>(&text) {
                    me.handle(message);
                }
            }
            me.exited(generation);
        });
        Ok(())
    }

    fn exited(&self, generation: u64) {
        let (waiting, errors) = {
            let mut s = self.state.lock().unwrap();
            if s.generation != generation {
                return;
            }
            s.stdin = None;
            if let Some(mut child) = s.child.take() {
                let _ = child.wait();
            }
            (std::mem::take(&mut s.waiting), std::mem::take(&mut s.errors))
        };
        let detail = errors.trim();
        let tail: String = detail.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
        let failure = if detail.is_empty() { "Cursor's agent stopped.".to_string() } else { format!("Cursor's agent stopped. {tail}") };
        for (_, tx) in waiting {
            let _ = tx.send(Err(failure.clone()));
        }
    }

    fn handle(&self, message: Value) {
        let method = message.get("method").and_then(Value::as_str).map(str::to_string);
        match (message.get("id"), method) {
            (Some(id), None) => {
                // A reply to one of ours.
                let Some(id) = id.as_i64() else { return };
                let tx = self.state.lock().unwrap().waiting.remove(&id);
                let result = match message.get("error") {
                    Some(error) => Err(explain(error.get("message").and_then(Value::as_str).unwrap_or("The agent returned an error."))),
                    None => Ok(message.get("result").cloned().unwrap_or(json!({}))),
                };
                if let Some(tx) = tx {
                    let _ = tx.send(result);
                }
            }
            (Some(id), Some(method)) => self.answer(id.clone(), &method, message.get("params")),
            (None, Some(method)) if method == "session/update" => {
                let params = message.get("params");
                let session = params.and_then(|p| p.get("sessionId")).and_then(Value::as_str);
                let update = params.and_then(|p| p.get("update"));
                if let (Some(session), Some(update)) = (session, update) {
                    self.deliver(Update { session: session.to_string(), update: update.clone() });
                }
            }
            _ => {}
        }
    }

    fn deliver(&self, update: Update) {
        if let Some((tx, waker)) = self.sink.lock().unwrap().as_ref() {
            let _ = tx.send(update);
            if let Some(w) = waker {
                w();
            }
        }
    }

    /// The agent's own requests. Permission is always declined, so nothing it asks to do (edit a
    /// file, run a command) happens; there's nothing else it may ask Mrow for.
    fn answer(&self, id: Value, method: &str, params: Option<&Value>) {
        if method != "session/request_permission" {
            self.write(&json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("Mrow doesn't offer {method}.")}}));
            return;
        }
        self.state.lock().unwrap().declined += 1;
        let reject = params
            .and_then(|p| p.get("options"))
            .and_then(Value::as_array)
            .and_then(|options| options.iter().find(|o| o.get("kind").and_then(Value::as_str).is_some_and(|k| k.starts_with("reject"))))
            .and_then(|o| o.get("optionId").cloned());
        let outcome = match reject {
            Some(option) => json!({"outcome": "selected", "optionId": option}),
            None => json!({"outcome": "cancelled"}),
        };
        self.write(&json!({"jsonrpc": "2.0", "id": id, "result": {"outcome": outcome}}));
        // Shown in the chat, so it's clear what the agent wanted and didn't get.
        if let Some(session) = params.and_then(|p| p.get("sessionId")).and_then(Value::as_str) {
            let call = params.and_then(|p| p.get("toolCall")).cloned().unwrap_or(json!({}));
            self.deliver(Update { session: session.to_string(), update: json!({"sessionUpdate": "mrow_declined", "toolCall": call}) });
        }
    }
}

fn explain(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("auth") || lower.contains("log in") || lower.contains("login") || lower.contains("unauthori") {
        return "Cursor isn't signed in. Run `agent login` in a terminal, then ask again.".into();
    }
    message.to_string()
}

/// `MROW_ACP_LOG=<file>` appends every message, both ways, for debugging.
fn log(direction: &str, line: &str) {
    static FILE: LazyLock<Option<Mutex<std::fs::File>>> =
        LazyLock::new(|| std::env::var_os("MROW_ACP_LOG").and_then(|p| std::fs::File::create(p).ok()).map(Mutex::new));
    if let Some(file) = FILE.as_ref() {
        let _ = file.lock().unwrap().write_all(format!("{direction}{line}").as_bytes());
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The agent's path: the one set in Settings, else the usual install places.
pub fn executable() -> Result<String, String> {
    let custom = prefs::read(|p| p.agent_path.trim().to_string());
    if !custom.is_empty() {
        if !is_executable(Path::new(&custom)) {
            return Err(format!("Cursor's agent isn't at {custom}. Fix the path in Settings."));
        }
        return Ok(custom);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let mut candidates = vec![format!("{home}/.local/bin/agent")];
    candidates.extend(crate::data::paths::tool_dirs().iter().map(|d| d.join("cursor-agent").to_string_lossy().into_owned()));
    candidates
        .into_iter()
        .find(|c| is_executable(Path::new(c)))
        .ok_or_else(|| "Couldn't find Cursor's agent. Install it (curl https://cursor.com/install -fsS | bash) or set its path in Settings.".to_string())
}
