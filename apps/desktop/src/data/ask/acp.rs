//! An agent spoken to over the Agent Client Protocol (Cursor's `agent acp`, Claude's adapter,
//! `opencode acp`): JSON-RPC, one message per line on its stdin and stdout. One process per agent
//! serves every chat.
//!
//! Mrow offers it no file or terminal access of its own. It allows the agent's permission
//! requests for the tool kinds in `agents::TOOL_KINDS` and declines the rest.
//!
//! Requests block their (worker) thread until the reply; session updates go to the UI thread
//! through the sink, with a wake.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, LazyLock, Mutex};

use rune::Waker;
use serde_json::{Value, json};

use super::agents::AgentKind;

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

/// What a chat needs of an agent, whichever protocol it speaks underneath. Requests block their
/// (worker) thread until the reply.
pub trait Agent: Send + Sync {
    /// Sends a request and waits for its reply. Starts the agent if it isn't running.
    fn request(self: Arc<Self>, method: &str, params: Value) -> Result<Value, String>;
    fn notify(&self, method: &str, params: Value);
    /// Stops the agent (when quitting); the next request starts a new one.
    fn stop(&self);
    /// How many permission requests have been declined since the agent started.
    fn declined(&self) -> usize {
        0
    }
}

static SINK: Mutex<Option<(Sender<Update>, Option<Waker>)>> = Mutex::new(None);

/// Where every agent's session updates go.
pub fn set_sink(tx: Sender<Update>, waker: Option<Waker>) {
    *SINK.lock().unwrap() = Some((tx, waker));
}

pub fn deliver(update: Update) {
    if let Some((tx, waker)) = SINK.lock().unwrap().as_ref() {
        let _ = tx.send(update);
        if let Some(w) = waker {
            w();
        }
    }
}

static CLIENTS: LazyLock<Vec<(AgentKind, Arc<dyn Agent>)>> = LazyLock::new(|| {
    AgentKind::ALL
        .into_iter()
        .map(|kind| {
            let agent: Arc<dyn Agent> = match kind {
                AgentKind::Pi => Arc::new(super::pi::Pi::default()),
                _ => Arc::new(Client { kind, state: Mutex::new(State::default()), starting: Mutex::new(false) }),
            };
            (kind, agent)
        })
        .collect()
});

/// The one client for this agent.
pub fn agent(kind: AgentKind) -> Arc<dyn Agent> {
    CLIENTS.iter().find(|(k, _)| *k == kind).map(|(_, a)| a.clone()).expect("every agent has a client")
}

/// Stops every agent that's running.
pub fn stop_all() {
    for (_, agent) in CLIENTS.iter() {
        agent.stop();
    }
}

pub struct Client {
    kind: AgentKind,
    state: Mutex<State>,
    /// Held while starting, so concurrent first requests wait for one launch.
    starting: Mutex<bool>,
}

impl Agent for Client {
    fn request(self: Arc<Self>, method: &str, params: Value) -> Result<Value, String> {
        self.start()?;
        self.send(method, params)
    }

    fn notify(&self, method: &str, params: Value) {
        self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn stop(&self) {
        let mut s = self.state.lock().unwrap();
        s.stdin = None;
        if let Some(mut child) = s.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn declined(&self) -> usize {
        self.state.lock().unwrap().declined
    }
}

impl Client {
    fn send(&self, method: &str, params: Value) -> Result<Value, String> {
        let (tx, rx) = channel();
        let id = {
            let mut s = self.state.lock().unwrap();
            if s.stdin.is_none() {
                return Err(format!("{} isn't running.", self.kind.process()));
            }
            s.next_id += 1;
            let id = s.next_id;
            s.waiting.insert(id, tx);
            id
        };
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        rx.recv().unwrap_or_else(|_| Err(format!("{} stopped.", self.kind.process())))
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
                    // Cursor: plain model ids, with effort as a separate setting.
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
        let command = self.kind.command()?;
        let name = self.kind.process();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        let path_env = crate::data::paths::child_path();
        let mut child = Command::new(&command[0])
            .args(&command[1..])
            .current_dir(&home)
            .env("PATH", path_env)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't start {name} at {}: {e}", command[0]))?;
        let stdout = child.stdout.take().ok_or_else(|| format!("No output from {name}."))?;
        let mut stderr = child.stderr.take().ok_or_else(|| format!("No error output from {name}."))?;
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
        let name = self.kind.process();
        let failure = if detail.is_empty() { format!("{name} stopped.") } else { format!("{name} stopped. {tail}") };
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
                    Some(error) => Err(explain(self.kind, error.get("message").and_then(Value::as_str).unwrap_or("The agent returned an error."))),
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
                    deliver(Update { session: session.to_string(), update: update.clone() });
                }
            }
            _ => {}
        }
    }

    /// The agent's own requests. A permission request is allowed when its tool's kind is on the
    /// list (`agents::TOOL_KINDS`) and declined otherwise; there's nothing else it may ask Mrow for.
    fn answer(&self, id: Value, method: &str, params: Option<&Value>) {
        if method != "session/request_permission" {
            self.write(&json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("Mrow doesn't offer {method}.")}}));
            return;
        }
        let call = params.and_then(|p| p.get("toolCall")).cloned().unwrap_or(json!({}));
        let allowed = call.get("kind").and_then(Value::as_str).is_some_and(super::agents::allows);
        let options = params.and_then(|p| p.get("options")).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        let pick = |prefix: &str| {
            options.iter().find(|o| o.get("kind").and_then(Value::as_str).is_some_and(|k| k.starts_with(prefix))).and_then(|o| o.get("optionId").cloned())
        };
        let choice = if allowed { pick("allow_once").or_else(|| pick("allow")) } else { pick("reject") };
        let outcome = match choice {
            Some(option) => json!({"outcome": "selected", "optionId": option}),
            None => json!({"outcome": "cancelled"}),
        };
        self.write(&json!({"jsonrpc": "2.0", "id": id, "result": {"outcome": outcome}}));
        if allowed {
            return;
        }
        self.state.lock().unwrap().declined += 1;
        // Shown in the chat, so it's clear what the agent wanted and didn't get.
        if let Some(session) = params.and_then(|p| p.get("sessionId")).and_then(Value::as_str) {
            deliver(Update { session: session.to_string(), update: json!({"sessionUpdate": "mrow_declined", "toolCall": call}) });
        }
    }
}

pub fn explain(kind: AgentKind, message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("auth") || lower.contains("log in") || lower.contains("login") || lower.contains("unauthori") {
        return kind.sign_in().into();
    }
    message.to_string()
}

/// `MROW_ACP_LOG=<file>` appends every message, both ways, for debugging.
pub fn log(direction: &str, line: &str) {
    static FILE: LazyLock<Option<Mutex<std::fs::File>>> =
        LazyLock::new(|| std::env::var_os("MROW_ACP_LOG").and_then(|p| std::fs::File::create(p).ok()).map(Mutex::new));
    if let Some(file) = FILE.as_ref() {
        let _ = file.lock().unwrap().write_all(format!("{direction}{line}").as_bytes());
    }
}
