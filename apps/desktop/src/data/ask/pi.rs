//! pi, run as `pi --mode rpc` and spoken to in its own JSON lines. It has no ACP mode, so this
//! answers the few ACP calls a chat makes (`session/new`, `session/load`, `session/prompt`,
//! `session/set_config_option`, `session/cancel`) and turns pi's events into ACP session updates.
//!
//! A pi process holds one session, so each chat gets its own. Each runs with only the tools of
//! the allowed kinds (`agents::TOOL_KINDS`): `read`, `grep`, `find` and `ls`. A session's id is its file,
//! kept under Mrow's cache, so a chat reopens with `--session`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::acp::{Agent, Update, deliver, explain, log};
use super::agents::AgentKind;

#[derive(Default)]
pub struct Pi {
    /// Each chat's process, by session id.
    procs: Mutex<HashMap<String, Arc<Proc>>>,
}

enum RunEnd {
    Settled { aborted: bool },
    Exited(String),
}

#[derive(Default)]
struct Proc {
    session: Mutex<String>,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    next_id: AtomicU64,
    waiting: Mutex<HashMap<String, Sender<Result<Value, String>>>>,
    /// Set while a prompt runs; told when pi settles.
    run: Mutex<Option<Sender<RunEnd>>>,
    /// The last answer's stop reason and error, for the prompt's reply.
    stop: Mutex<Option<String>>,
    error: Mutex<Option<String>>,
    errors: Mutex<String>,
}

impl Agent for Pi {
    fn request(self: Arc<Self>, method: &str, params: Value) -> Result<Value, String> {
        let s = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        match method {
            "session/new" => {
                let proc = launch(Path::new(&s("cwd")), None)?;
                let state = proc.command(json!({"type": "get_state"}))?;
                let session =
                    state.get("sessionFile").or_else(|| state.get("sessionId")).and_then(Value::as_str).ok_or("pi didn't start a session.")?.to_string();
                *proc.session.lock().unwrap() = session.clone();
                let options = proc.options()?;
                self.procs.lock().unwrap().insert(session.clone(), proc);
                Ok(json!({"sessionId": session, "configOptions": options}))
            }
            "session/load" => {
                let session = s("sessionId");
                if !Path::new(&session).is_file() {
                    return Err("That pi session is gone.".into());
                }
                let proc = launch(Path::new(&s("cwd")), Some(&session))?;
                *proc.session.lock().unwrap() = session.clone();
                let messages = proc.command(json!({"type": "get_messages"}))?;
                replay(&session, messages.get("messages").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default());
                let options = proc.options()?;
                self.procs.lock().unwrap().insert(session, proc);
                Ok(json!({"configOptions": options}))
            }
            "session/prompt" => {
                let proc = self.proc(&s("sessionId"))?;
                let text: Vec<&str> = params
                    .get("prompt")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect())
                    .unwrap_or_default();
                proc.prompt(&text.join("\n\n"))
            }
            "session/set_config_option" | "session/set_model" => {
                let proc = self.proc(&s("sessionId"))?;
                let (id, value) = if method == "session/set_model" { ("model".to_string(), s("modelId")) } else { (s("configId"), s("value")) };
                match id.as_str() {
                    "model" => {
                        let (provider, model) = value.split_once('/').ok_or("pi names models as provider/model.")?;
                        proc.command(json!({"type": "set_model", "provider": provider, "modelId": model}))?;
                    }
                    "thinking" => {
                        proc.command(json!({"type": "set_thinking_level", "level": value}))?;
                    }
                    _ => return Err(format!("pi has no {id} setting.")),
                }
                Ok(json!({"configOptions": proc.options()?}))
            }
            // pi has no modes; its tools are limited when it starts.
            "session/set_mode" => Ok(json!({})),
            _ => Err(format!("pi doesn't offer {method}.")),
        }
    }

    fn notify(&self, method: &str, params: Value) {
        if method != "session/cancel" {
            return;
        }
        let session = params.get("sessionId").and_then(Value::as_str).unwrap_or("");
        if let Some(proc) = self.procs.lock().unwrap().get(session) {
            proc.write(&json!({"type": "abort"}));
        }
    }

    fn stop(&self) {
        for (_, proc) in self.procs.lock().unwrap().drain() {
            proc.kill();
        }
    }
}

impl Pi {
    fn proc(&self, session: &str) -> Result<Arc<Proc>, String> {
        let proc = self.procs.lock().unwrap().get(session).cloned();
        match proc {
            Some(p) if p.stdin.lock().unwrap().is_some() => Ok(p),
            _ => Err("pi stopped.".into()),
        }
    }
}

fn launch(cwd: &Path, session: Option<&str>) -> Result<Arc<Proc>, String> {
    let command = AgentKind::Pi.command()?;
    let sessions = crate::data::paths::cache_dir().join("pi-sessions");
    let _ = std::fs::create_dir_all(&sessions);
    let mut args: Vec<String> = command[1..].to_vec();
    args.extend(["--mode".to_string(), "rpc".to_string(), "--tools".to_string(), super::agents::pi_tools()]);
    args.extend(["--session-dir".to_string(), sessions.to_string_lossy().into_owned()]);
    if let Some(session) = session {
        args.extend(["--session".to_string(), session.to_string()]);
    }
    let mut child = Command::new(&command[0])
        .args(&args)
        .current_dir(cwd)
        .env("PATH", crate::data::paths::child_path())
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start pi at {}: {e}", command[0]))?;
    let stdout = child.stdout.take().ok_or("No output from pi.")?;
    let mut stderr = child.stderr.take().ok_or("No error output from pi.")?;
    let proc = Arc::new(Proc::default());
    *proc.stdin.lock().unwrap() = child.stdin.take();
    *proc.child.lock().unwrap() = Some(child);
    let me = proc.clone();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(n) = stderr.read(&mut buffer) {
            if n == 0 {
                break;
            }
            let mut errors = me.errors.lock().unwrap();
            errors.push_str(&String::from_utf8_lossy(&buffer[..n]));
            if errors.len() > 2000 {
                let mut cut = errors.len() - 2000;
                while !errors.is_char_boundary(cut) {
                    cut += 1;
                }
                errors.drain(..cut);
            }
        }
    });
    let me = proc.clone();
    std::thread::spawn(move || {
        // Split on LF only: JSON strings may hold U+2028, which other line readers split on.
        for line in BufReader::new(stdout).split(b'\n') {
            let Ok(line) = line else { break };
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end_matches('\r');
            if text.is_empty() {
                continue;
            }
            log("pi<< ", &format!("{text}\n"));
            if let Ok(record) = serde_json::from_str::<Value>(text) {
                me.handle(record);
            }
        }
        me.exited();
    });
    Ok(proc)
}

impl Proc {
    fn write(&self, record: &Value) {
        let mut line = record.to_string();
        line.push('\n');
        log("pi>> ", &line);
        let mut stdin = self.stdin.lock().unwrap();
        if let Some(pipe) = stdin.as_mut() {
            if pipe.write_all(line.as_bytes()).and_then(|_| pipe.flush()).is_err() {
                *stdin = None;
            }
        }
    }

    /// Sends a command and waits for its response's data.
    fn command(&self, mut command: Value) -> Result<Value, String> {
        let id = format!("m{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = channel();
        self.waiting.lock().unwrap().insert(id.clone(), tx);
        command["id"] = json!(id);
        self.write(&command);
        rx.recv().unwrap_or_else(|_| Err("pi stopped.".into()))
    }

    fn prompt(&self, message: &str) -> Result<Value, String> {
        let (tx, rx) = channel();
        *self.run.lock().unwrap() = Some(tx);
        *self.stop.lock().unwrap() = None;
        *self.error.lock().unwrap() = None;
        let accepted = self.command(json!({"type": "prompt", "message": message}));
        let disposition = match accepted {
            Ok(data) => data.get("disposition").and_then(Value::as_str).unwrap_or("started").to_string(),
            Err(e) => {
                self.run.lock().unwrap().take();
                return Err(explain(AgentKind::Pi, &e));
            }
        };
        if disposition == "handled" {
            self.run.lock().unwrap().take();
            return Ok(json!({"stopReason": "end_turn"}));
        }
        match rx.recv() {
            Ok(RunEnd::Settled { aborted: true }) => Ok(json!({"stopReason": "cancelled"})),
            Ok(RunEnd::Settled { aborted: false }) => {
                if let Some(error) = self.error.lock().unwrap().take() {
                    return Err(explain(AgentKind::Pi, &error));
                }
                let stop = match self.stop.lock().unwrap().as_deref() {
                    Some("length") => "max_tokens",
                    Some("aborted") => "cancelled",
                    _ => "end_turn",
                };
                Ok(json!({"stopReason": stop}))
            }
            Ok(RunEnd::Exited(e)) => Err(e),
            Err(_) => Err("pi stopped.".into()),
        }
    }

    /// The model and thinking level as ACP config options, so the picker works as for the others.
    fn options(&self) -> Result<Vec<Value>, String> {
        let state = self.command(json!({"type": "get_state"}))?;
        let models = self.command(json!({"type": "get_available_models"}))?;
        let levels = self.command(json!({"type": "get_available_thinking_levels"}))?;
        let id = |m: &Value| -> Option<String> { Some(format!("{}/{}", m.get("provider")?.as_str()?, m.get("id")?.as_str()?)) };
        let choices: Vec<Value> = models
            .get("models")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|m| {
                        let value = id(m)?;
                        let name = m.get("name").and_then(Value::as_str).unwrap_or(&value).to_string();
                        Some(json!({"value": value, "name": name}))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let current = state.get("model").and_then(id).unwrap_or_default();
        let mut options = vec![json!({"id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": current, "options": choices})];
        let levels: Vec<&str> = levels.get("levels").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        if levels.iter().any(|l| *l != "off") {
            let named: Vec<Value> = levels.iter().map(|l| json!({"value": l, "name": title(l)})).collect();
            let level = state.get("thinkingLevel").and_then(Value::as_str).unwrap_or("");
            options.push(json!({"id": "thinking", "name": "Thinking", "category": "thought_level", "type": "select", "currentValue": level, "options": named}));
        }
        Ok(options)
    }

    fn handle(&self, record: Value) {
        let kind = record.get("type").and_then(Value::as_str).unwrap_or("");
        let session = self.session.lock().unwrap().clone();
        let send = |update: Value| deliver(Update { session: session.clone(), update });
        match kind {
            "response" => {
                let Some(id) = record.get("id").and_then(Value::as_str) else { return };
                let Some(tx) = self.waiting.lock().unwrap().remove(id) else { return };
                let result = if record.get("success").and_then(Value::as_bool) == Some(true) {
                    Ok(record.get("data").cloned().unwrap_or(json!({})))
                } else {
                    Err(record.get("error").and_then(Value::as_str).unwrap_or("pi returned an error.").to_string())
                };
                let _ = tx.send(result);
            }
            "message_update" => {
                let event = &record["assistantMessageEvent"];
                let delta = event.get("delta").and_then(Value::as_str).unwrap_or("");
                match event.get("type").and_then(Value::as_str) {
                    Some("text_delta") => send(json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": delta}})),
                    Some("thinking_delta") => send(json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": delta}})),
                    _ => {}
                }
            }
            "tool_execution_start" => {
                let name = record.get("toolName").and_then(Value::as_str).unwrap_or("tool");
                let (title, kind) = tool_line(name, record.get("args").unwrap_or(&Value::Null));
                send(json!({"sessionUpdate": "tool_call", "toolCallId": record["toolCallId"], "title": title, "kind": kind, "status": "in_progress"}));
            }
            "tool_execution_end" => {
                let failed = record.get("isError").and_then(Value::as_bool) == Some(true);
                send(json!({"sessionUpdate": "tool_call_update", "toolCallId": record["toolCallId"], "status": if failed { "failed" } else { "completed" }}));
            }
            "message_end" if record["message"]["role"] == "assistant" => {
                let stop = record["message"].get("stopReason").and_then(Value::as_str).map(str::to_string);
                if stop.as_deref() == Some("error") {
                    let message = record["message"].get("errorMessage").and_then(Value::as_str).unwrap_or("The model returned an error.");
                    *self.error.lock().unwrap() = Some(message.to_string());
                }
                *self.stop.lock().unwrap() = stop;
            }
            "agent_settled" => {
                let aborted = record.get("aborted").and_then(Value::as_bool) == Some(true);
                if let Some(tx) = self.run.lock().unwrap().take() {
                    let _ = tx.send(RunEnd::Settled { aborted });
                }
            }
            // An extension asking for a choice would wait forever: decline it.
            "extension_ui_request" => {
                let dialog = matches!(record.get("method").and_then(Value::as_str), Some("select" | "confirm" | "input" | "editor"));
                if let (true, Some(id)) = (dialog, record.get("id")) {
                    self.write(&json!({"type": "extension_ui_response", "id": id, "cancelled": true}));
                }
            }
            _ => {}
        }
    }

    fn exited(&self) {
        *self.stdin.lock().unwrap() = None;
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.wait();
        }
        let errors = self.errors.lock().unwrap().trim().to_string();
        let tail: String = errors.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
        let failure = if tail.is_empty() { "pi stopped.".to_string() } else { format!("pi stopped. {tail}") };
        for (_, tx) in self.waiting.lock().unwrap().drain() {
            let _ = tx.send(Err(failure.clone()));
        }
        if let Some(tx) = self.run.lock().unwrap().take() {
            let _ = tx.send(RunEnd::Exited(failure));
        }
    }

    fn kill(&self) {
        *self.stdin.lock().unwrap() = None;
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A tool call's line in the chat, and its ACP kind: ("Read src/x.rs", "read").
pub fn tool_line(name: &str, args: &Value) -> (String, &'static str) {
    let a = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match name {
        "read" => (format!("Read {}", a("path")).trim().to_string(), "read"),
        "grep" => {
            let place = a("path");
            let place = if place.is_empty() { String::new() } else { format!(" in {place}") };
            (format!("Grep \"{}\"{place}", a("pattern")), "search")
        }
        "find" => (format!("Find {}", a("pattern")).trim().to_string(), "search"),
        "ls" => (format!("List {}", a("path")).trim().to_string(), "search"),
        "bash" => (format!("Run {}", a("command")), "execute"),
        "edit" | "write" => (format!("Edit {}", a("path")), "edit"),
        other => (other.to_string(), "other"),
    }
}

/// An earlier chat's messages as the updates that would have built it.
fn replay(session: &str, messages: &[Value]) {
    let send = |update: Value| deliver(Update { session: session.to_string(), update });
    let chunk = |kind: &str, text: &str| json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}});
    for message in messages {
        let blocks: Vec<Value> = match &message["content"] {
            Value::String(text) => vec![json!({"type": "text", "text": text})],
            Value::Array(blocks) => blocks.clone(),
            _ => continue,
        };
        match message["role"].as_str() {
            Some("user") => {
                for b in blocks.iter().filter(|b| b["type"] == "text") {
                    send(chunk("user_message_chunk", b["text"].as_str().unwrap_or("")));
                }
            }
            Some("assistant") => {
                for b in &blocks {
                    match b["type"].as_str() {
                        Some("text") => send(chunk("agent_message_chunk", b["text"].as_str().unwrap_or(""))),
                        Some("thinking") => send(chunk("agent_thought_chunk", b["thinking"].as_str().unwrap_or(""))),
                        Some("toolCall") => {
                            let (title, kind) = tool_line(b["name"].as_str().unwrap_or("tool"), &b["arguments"]);
                            send(json!({"sessionUpdate": "tool_call", "toolCallId": b["id"], "title": title, "kind": kind, "status": "completed"}));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn title(level: &str) -> String {
    match level {
        "xhigh" => "Extra High".into(),
        other => {
            let mut c = other.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}
