//! Claude, through the user's own Claude Code, ported from Mdow: a Rust take on the part of
//! Anthropic's Agent SDK a chat needs (`claude-agent-sdk-python`, `subprocess_cli.py` and
//! `query.py`). No Node and no ACP adapter, and nothing touches the user's credentials: the CLI
//! signs in as it always does.
//!
//! Each chat is one `claude` process in stream-json mode: one JSON message per line on its stdin
//! and stdout. Mrow writes user messages and control requests (initialize, interrupt, set_model,
//! apply_flag_settings); Claude Code writes its messages, partial ones included, and asks
//! permission for any tool call outside its read-only defaults (`can_use_tool`, through
//! `--permission-prompt-tool stdio`).
//!
//! The session is small and hermetic, which also keeps it fast: only the tools of the allowed
//! kinds (`agents::TOOL_KINDS`), no MCP servers, and none of the user's settings files, whose
//! allow rules (or a tool that posts a comment) would get around the list.
//!
//! It answers the ACP calls a chat makes (`session/new`, `session/load`, `session/prompt`,
//! `session/set_config_option`, `session/cancel`, `session/close`) and reports in ACP's shapes,
//! so the rest of Ask treats Claude like the other agents.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::{Value, json};

use super::acp::{Agent, Update, deliver, explain, log};
use super::agents::{self, AgentKind};

/// What Mrow tells Claude when it declines a tool call.
const DECLINED: &str = "Mrow doesn't allow this here: only reading and searching this folder.";

/// How long a request other than a prompt may take, so a hung process (at a login prompt, say)
/// can't hold a worker forever.
const PATIENCE: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Claude {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    declined: AtomicUsize,
}

#[derive(Default)]
struct State {
    stdin: Option<ChildStdin>,
    child: Option<Child>,
    next: u64,
    /// Control requests waiting on their reply, by request id.
    waiting: HashMap<String, Sender<Result<Value, String>>>,
    /// The prompt waiting on its result.
    turn: Option<Sender<Result<Value, String>>>,
    cancelled: bool,
    /// Stopped turns whose result hasn't come yet. Claude Code answers messages in order, so the
    /// next results, and what streams before them, belong to those turns, not the new one.
    stale: u32,
    /// The models from `initialize`.
    models: Vec<Value>,
    model: String,
    effort: String,
    /// The message streaming now, and those whose text arrived in parts.
    message: String,
    streamed: HashSet<String>,
    /// The end of stderr, for errors.
    errors: String,
    /// Set once the process has exited, so a turn begun after can't wait forever.
    gone: bool,
}

struct Session {
    id: String,
    directory: String,
    state: Mutex<State>,
}

impl Agent for Claude {
    fn request(self: Arc<Self>, method: &str, params: Value) -> Result<Value, String> {
        let s = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        match method {
            "session/new" => self.start(&s("cwd"), None),
            "session/load" => {
                let id = s("sessionId");
                let transcript = transcript(&id).ok_or("That chat with Claude is gone.")?;
                let reply = self.start(&s("cwd"), Some(&id))?;
                replay(&id, &transcript);
                Ok(reply)
            }
            "session/prompt" => {
                let text: Vec<&str> = params
                    .get("prompt")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect())
                    .unwrap_or_default();
                self.session(&s("sessionId"))?.prompt(&text.join("\n\n"))
            }
            "session/set_config_option" => {
                let session = self.session(&s("sessionId"))?;
                match s("configId").as_str() {
                    "model" => session.set_model(&s("value")),
                    "effort" => session.set_effort(&s("value")),
                    other => Err(format!("Claude has no {other} setting.")),
                }
            }
            // Claude has no modes here; its tools are limited when it starts.
            "session/set_mode" => Ok(json!({})),
            _ => Err(format!("Claude doesn't offer {method}.")),
        }
    }

    /// `session/cancel` interrupts the turn; `session/close` ends the chat's process.
    fn notify(&self, method: &str, params: Value) {
        let id = params.get("sessionId").and_then(Value::as_str).unwrap_or("");
        match method {
            "session/cancel" => {
                let session = self.lock().get(id).cloned();
                if let Some(s) = session {
                    s.interrupt();
                }
            }
            "session/close" => {
                let removed = self.lock().remove(id);
                if let Some(s) = removed {
                    s.stop();
                }
            }
            _ => {}
        }
    }

    fn stop(&self) {
        let all: Vec<_> = self.lock().drain().map(|(_, s)| s).collect();
        for s in all {
            s.stop();
        }
    }

    fn declined(&self) -> usize {
        self.declined.load(Ordering::Relaxed)
    }
}

impl Claude {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<Session>>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn session(&self, id: &str) -> Result<Arc<Session>, String> {
        let s = self.lock().get(id).cloned().ok_or("This chat with Claude has ended.")?;
        if s.state().stdin.is_none() {
            return Err(format!("{} stopped.", AgentKind::Claude.process()));
        }
        Ok(s)
    }

    /// Starts a chat in `directory`, or picks up an earlier one: the process, then the SDK's
    /// `initialize`.
    fn start(self: &Arc<Self>, directory: &str, resume: Option<&str>) -> Result<Value, String> {
        let program = AgentKind::Claude.command()?;
        let id = resume.map(str::to_string).unwrap_or_else(uuid);
        let directory = if directory.is_empty() { "/" } else { directory };
        let tools = claude_tools();
        let mut command = Command::new(&program[0]);
        command
            .args(&program[1..])
            .args(["--output-format", "stream-json", "--input-format", "stream-json", "--verbose"])
            .args(["--permission-prompt-tool", "stdio", "--permission-mode", "default"])
            .args(["--tools", &tools, "--setting-sources=", "--strict-mcp-config"])
            .args(["--mcp-config", r#"{"mcpServers":{}}"#])
            .args(["--include-partial-messages", "--thinking", "adaptive", "--thinking-display", "summarized"]);
        match resume {
            Some(id) => command.args(["--resume", id]),
            None => command.arg(format!("--session-id={id}")),
        };
        let mut child = command
            .current_dir(directory)
            .env("PATH", agents::child_path(&program[0]))
            // Set when Mrow itself runs inside Claude Code; the child would think it's nested.
            .env_remove("CLAUDECODE")
            .env("CLAUDE_AGENT_SDK_CLIENT_APP", "Mrow")
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't start Claude Code at {}: {e}", program[0]))?;
        let stdout = child.stdout.take().ok_or("No output from Claude Code.")?;
        let mut stderr = child.stderr.take().ok_or("No output from Claude Code.")?;
        let s = Arc::new(Session {
            id: id.clone(),
            directory: directory.to_string(),
            state: Mutex::new(State { stdin: child.stdin.take(), child: Some(child), model: "default".into(), ..State::default() }),
        });
        let me = s.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                keep_errors(&mut me.state().errors, &String::from_utf8_lossy(&buffer[..n]));
            }
        });
        let (me, owner) = (s.clone(), self.clone());
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).split(b'\n') {
                let Ok(line) = line else { break };
                if line.is_empty() {
                    continue;
                }
                log("<< ", &format!("{}\n", String::from_utf8_lossy(&line)));
                if let Ok(message) = serde_json::from_slice::<Value>(&line) {
                    if let Some(declined) = me.handle(message) {
                        owner.declined.fetch_add(declined, Ordering::Relaxed);
                    }
                }
            }
            me.exited();
        });
        // A chat picked up again replaces its old process, if one is still there.
        let old = self.lock().insert(id.clone(), s.clone());
        if let Some(old) = old {
            old.stop();
        }
        let reply = match s.control(json!({"subtype": "initialize", "hooks": null})) {
            Ok(r) => r,
            Err(e) => {
                let removed = self.lock().remove(&id);
                if let Some(s) = removed {
                    s.stop();
                }
                return Err(e);
            }
        };
        let mut st = s.state();
        st.models = reply.get("models").and_then(Value::as_array).cloned().unwrap_or_default();
        Ok(json!({"sessionId": id, "configOptions": options(&st)}))
    }
}

/// Claude Code's tools of the allowed kinds, for `--tools`.
fn claude_tools() -> String {
    agents::claude_allowed().join(",")
}

/// A random version 4 UUID, for `--session-id`.
fn uuid() -> String {
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        b = (t ^ ((std::process::id() as u128) << 64)).to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// Adds to the end of stderr kept for error messages: the last couple of kilobytes is enough.
fn keep_errors(errors: &mut String, part: &str) {
    errors.push_str(part);
    if errors.len() > 2000 {
        let mut cut = errors.len() - 2000;
        while !errors.is_char_boundary(cut) {
            cut += 1;
        }
        errors.drain(..cut);
    }
}

/// The session's settings in ACP's shape: its model, and the model's effort where it has one.
fn options(st: &State) -> Vec<Value> {
    let models: Vec<Value> = st
        .models
        .iter()
        .filter_map(|m| {
            let value = m.get("value")?.as_str()?;
            let name = m.get("displayName").and_then(Value::as_str).unwrap_or(value);
            Some(json!({"value": value, "name": name, "description": m.get("description").cloned().unwrap_or(Value::Null)}))
        })
        .collect();
    let mut out = vec![json!({"id": "model", "category": "model", "currentValue": st.model, "options": models})];
    let levels = efforts(st);
    if !levels.is_empty() {
        let choices: Vec<Value> = levels.iter().map(|l| json!({"value": l, "name": effort_name(l)})).collect();
        out.push(json!({"id": "effort", "category": "thought_level", "currentValue": st.effort, "options": choices}));
    }
    out
}

/// The effort levels the current model takes.
fn efforts(st: &State) -> Vec<String> {
    st.models
        .iter()
        .find(|m| m.get("value").and_then(Value::as_str) == Some(st.model.as_str()))
        .and_then(|m| m.get("supportedEffortLevels"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

/// An effort level's name: "high" → "High", "xhigh" → "Extra High".
fn effort_name(level: &str) -> String {
    match level {
        "xhigh" => "Extra High".into(),
        other => {
            let mut c = other.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

fn wait(rx: &Receiver<Result<Value, String>>, prompt: bool) -> Result<Value, String> {
    let name = AgentKind::Claude.process();
    if prompt {
        return rx.recv().unwrap_or_else(|_| Err(format!("{name} stopped.")));
    }
    match rx.recv_timeout(PATIENCE) {
        Ok(reply) => reply,
        Err(RecvTimeoutError::Timeout) => Err(format!("{name} didn't answer.")),
        Err(RecvTimeoutError::Disconnected) => Err(format!("{name} stopped.")),
    }
}

impl Session {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self, message: &Value) {
        let mut line = message.to_string();
        line.push('\n');
        log(">> ", &line);
        let mut st = self.state();
        if let Some(stdin) = st.stdin.as_mut() {
            if stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()).is_err() {
                st.stdin = None;
            }
        }
    }

    fn next_request(&self) -> String {
        let mut st = self.state();
        st.next += 1;
        format!("mrow_{}", st.next)
    }

    /// Sends a control request and waits for its reply.
    fn control(&self, request: Value) -> Result<Value, String> {
        let (tx, rx) = channel();
        let id = self.next_request();
        {
            let mut st = self.state();
            if st.stdin.is_none() {
                return Err("Claude Code isn't running.".into());
            }
            st.waiting.insert(id.clone(), tx);
        }
        self.write(&json!({"type": "control_request", "request_id": id, "request": request}));
        let reply = wait(&rx, false);
        // A reply that never came: its waiter goes, so a late one isn't sent nowhere.
        self.state().waiting.remove(&id);
        reply
    }

    /// Sends the user's message and waits for the turn's result.
    fn prompt(&self, text: &str) -> Result<Value, String> {
        let (tx, rx) = channel();
        self.begin(tx)?;
        self.write(&json!({
            "type": "user",
            "message": {"role": "user", "content": text},
            "parent_tool_use_id": null,
            "session_id": self.id,
        }));
        wait(&rx, true)
    }

    /// Makes `tx` the turn's waiter. A stopped turn still running: its result isn't this one's.
    fn begin(&self, tx: Sender<Result<Value, String>>) -> Result<(), String> {
        let mut st = self.state();
        if st.gone {
            return Err(format!("{} stopped.", AgentKind::Claude.process()));
        }
        if st.turn.replace(tx).is_some() {
            st.stale += 1;
        }
        st.cancelled = false;
        Ok(())
    }

    fn interrupt(&self) {
        {
            let mut st = self.state();
            if st.turn.is_none() {
                return;
            }
            st.cancelled = true;
        }
        // Its reply isn't waited on: the turn's result ends the prompt.
        let id = self.next_request();
        self.write(&json!({"type": "control_request", "request_id": id, "request": {"subtype": "interrupt"}}));
    }

    fn set_model(&self, model: &str) -> Result<Value, String> {
        self.control(json!({"subtype": "set_model", "model": model}))?;
        let mut st = self.state();
        st.model = model.to_string();
        // The effort carries over where the new model takes it.
        if !efforts(&st).contains(&st.effort) {
            st.effort.clear();
        }
        Ok(json!({"configOptions": options(&st)}))
    }

    fn set_effort(&self, effort: &str) -> Result<Value, String> {
        self.control(json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": effort}}))?;
        let mut st = self.state();
        st.effort = effort.to_string();
        Ok(json!({"configOptions": options(&st)}))
    }

    fn stop(&self) {
        let mut st = self.state();
        st.stdin = None;
        if let Some(mut child) = st.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn exited(&self) {
        let (waiting, turn, errors) = {
            let mut st = self.state();
            st.stdin = None;
            st.gone = true;
            if let Some(mut child) = st.child.take() {
                let _ = child.wait();
            }
            st.stale = 0;
            (std::mem::take(&mut st.waiting), st.turn.take(), std::mem::take(&mut st.errors))
        };
        let name = AgentKind::Claude.process();
        let detail = errors.trim();
        let failure = if detail.is_empty() {
            format!("{name} stopped.")
        } else {
            let tail: String = detail.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
            explain(AgentKind::Claude, &format!("{name} stopped. {tail}"))
        };
        for (_, tx) in waiting {
            let _ = tx.send(Err(failure.clone()));
        }
        if let Some(tx) = turn {
            let _ = tx.send(Err(failure));
        }
    }

    fn update(&self, update: Value) {
        deliver(Update { session: self.id.clone(), update });
    }

    /// Handles one message from Claude Code; returns 1 when it declined a tool call.
    fn handle(&self, message: Value) -> Option<usize> {
        let kind = message.get("type").and_then(Value::as_str).unwrap_or("");
        // Subagents' messages carry their parent call; Mrow gives Claude no subagents. A stopped
        // turn's last words are dropped.
        let top = message.get("parent_tool_use_id").is_none_or(Value::is_null) && self.state().stale == 0;
        match kind {
            "control_response" => {
                let response = message.get("response").cloned().unwrap_or_default();
                let id = response.get("request_id").and_then(Value::as_str).unwrap_or("");
                let tx = self.state().waiting.remove(id)?;
                let result = match response.get("subtype").and_then(Value::as_str) {
                    Some("error") => Err(explain(AgentKind::Claude, response.get("error").and_then(Value::as_str).unwrap_or("Claude Code refused."))),
                    _ => Ok(response.get("response").cloned().unwrap_or(json!({}))),
                };
                let _ = tx.send(result);
            }
            "control_request" => return self.permission(&message),
            "stream_event" if top => self.stream(message.get("event").unwrap_or(&Value::Null)),
            "assistant" if top => self.assistant(message.get("message").unwrap_or(&Value::Null)),
            "user" if top => self.tool_results(message.get("message").unwrap_or(&Value::Null)),
            "result" => self.result(&message),
            _ => {}
        }
        None
    }

    /// Text and thinking as they arrive.
    fn stream(&self, event: &Value) {
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                let id = event.pointer("/message/id").and_then(Value::as_str).unwrap_or("");
                self.state().message = id.to_string();
            }
            Some("content_block_delta") => {
                let delta = event.get("delta").unwrap_or(&Value::Null);
                let (kind, text) = match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => ("agent_message_chunk", delta.get("text")),
                    Some("thinking_delta") => ("agent_thought_chunk", delta.get("thinking")),
                    _ => return,
                };
                let Some(text) = text.and_then(Value::as_str).filter(|t| !t.is_empty()) else { return };
                {
                    let mut st = self.state();
                    let id = st.message.clone();
                    st.streamed.insert(id);
                }
                self.update(json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}}));
            }
            _ => {}
        }
    }

    /// A finished message: its tool calls, and its text where none of it streamed.
    fn assistant(&self, message: &Value) {
        let id = message.get("id").and_then(Value::as_str).unwrap_or("");
        let streamed = self.state().streamed.contains(id);
        for update in assistant_updates(message, streamed, "pending") {
            self.update(update);
        }
    }

    /// Tool results come back as a user message.
    fn tool_results(&self, message: &Value) {
        for update in result_updates(message) {
            self.update(update);
        }
    }

    fn result(&self, message: &Value) {
        let (turn, cancelled) = {
            let mut st = self.state();
            st.streamed.clear();
            if st.stale > 0 {
                st.stale -= 1;
                return;
            }
            (st.turn.take(), std::mem::take(&mut st.cancelled))
        };
        let Some(tx) = turn else { return };
        let s = |k: &str| message.get(k).and_then(Value::as_str).unwrap_or("");
        let reply = if cancelled {
            Ok("cancelled")
        } else if s("subtype") == "error_max_turns" {
            Ok("max_turn_requests")
        } else if message.get("is_error").and_then(Value::as_bool).unwrap_or(false) || s("subtype").starts_with("error") {
            let errors: Vec<&str> = message.get("errors").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let text = if !s("result").is_empty() {
                s("result").to_string()
            } else if !errors.is_empty() {
                errors.join(" ")
            } else {
                "Claude Code stopped with an error.".to_string()
            };
            Err(explain(AgentKind::Claude, &text))
        } else {
            Ok(match s("stop_reason") {
                "max_tokens" => "max_tokens",
                "refusal" => "refusal",
                _ => "end_turn",
            })
        };
        let _ = tx.send(reply.map(|r| json!({"stopReason": r})));
    }

    /// `can_use_tool`: a tool of an allowed kind inside the folder runs; anything else is
    /// declined and shown in the chat. Claude Code asks about a read only when it's outside the
    /// folder, so in practice this declines those.
    fn permission(&self, message: &Value) -> Option<usize> {
        let request_id = message.get("request_id").and_then(Value::as_str).unwrap_or("").to_string();
        let request = message.get("request").unwrap_or(&Value::Null);
        let s = |k: &str| request.get(k).and_then(Value::as_str).unwrap_or("");
        if s("subtype") != "can_use_tool" {
            let error = format!("Mrow doesn't handle {}.", s("subtype"));
            self.write(&json!({"type": "control_response", "response": {"subtype": "error", "request_id": request_id, "error": error}}));
            return None;
        }
        let input = request.get("input").cloned().unwrap_or(json!({}));
        let (kind, title) = describe(s("tool_name"), &input);
        if agents::allows(kind) && self.inside(kind, &input) {
            let response = json!({"behavior": "allow", "updatedInput": input});
            self.write(&json!({"type": "control_response", "response": {"subtype": "success", "request_id": request_id, "response": response}}));
            return None;
        }
        let response = json!({"behavior": "deny", "message": DECLINED});
        self.write(&json!({"type": "control_response", "response": {"subtype": "success", "request_id": request_id, "response": response}}));
        let call = json!({"toolCallId": s("tool_use_id"), "title": title, "kind": kind, "rawInput": input});
        // Shown in the chat, so it's clear what the agent wanted and didn't get.
        self.update(json!({"sessionUpdate": "mrow_declined", "toolCall": call}));
        Some(1)
    }

    /// Whether a tool's file or folder is in the session's folder.
    /// Every path the tool names must be inside; a read that names none Mrow knows is not.
    fn inside(&self, kind: &str, input: &Value) -> bool {
        let mut paths: Vec<&str> = ["file_path", "path", "notebook_path"].iter().filter_map(|k| input.get(*k).and_then(Value::as_str)).collect();
        // Glob takes an absolute pattern too: "/etc/*".
        if let Some(pattern) = input.get("pattern").and_then(Value::as_str).filter(|p| p.starts_with('/') || p.starts_with('~')) {
            paths.push(pattern);
        }
        if paths.is_empty() {
            return kind == "search";
        }
        paths.iter().all(|p| within(p, &self.directory))
    }
}

/// An assistant message's content as session updates: its tool calls, and its text and thinking
/// unless they already streamed.
fn assistant_updates(message: &Value, streamed: bool, status: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let chunk = |kind: &str, text: &str| json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}});
    for block in message.get("content").and_then(Value::as_array).into_iter().flatten() {
        let s = |k: &str| block.get(k).and_then(Value::as_str).unwrap_or("");
        match s("type") {
            "tool_use" => {
                let input = block.get("input").cloned().unwrap_or(json!({}));
                let (kind, title) = describe(s("name"), &input);
                out.push(json!({"sessionUpdate": "tool_call", "toolCallId": s("id"), "title": title, "kind": kind, "status": status, "rawInput": input}));
            }
            "text" if !streamed && !s("text").is_empty() => out.push(chunk("agent_message_chunk", s("text"))),
            "thinking" if !streamed && !s("thinking").is_empty() => out.push(chunk("agent_thought_chunk", s("thinking"))),
            _ => {}
        }
    }
    out
}

/// A user message's tool results as updates to their calls.
fn result_updates(message: &Value) -> Vec<Value> {
    let Some(blocks) = message.get("content").and_then(Value::as_array) else { return Vec::new() };
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        .map(|block| {
            let failed = block.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": block.get("tool_use_id").and_then(Value::as_str).unwrap_or(""),
                "status": if failed { "failed" } else { "completed" },
            })
        })
        .collect()
}

/// A tool's ACP kind and title.
fn describe(tool: &str, input: &Value) -> (&'static str, String) {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
    match tool {
        "Read" | "NotebookRead" => ("read", format!("Read {}", s("file_path"))),
        "Glob" => ("search", format!("Find {}", s("pattern"))),
        "Grep" => ("search", format!("Search for {}", s("pattern"))),
        "LS" => ("search", format!("List {}", s("path"))),
        "Edit" | "MultiEdit" | "NotebookEdit" => ("edit", format!("Edit {}", s("file_path"))),
        "Write" => ("edit", format!("Write {}", s("file_path"))),
        "Bash" => ("execute", s("command").to_string()),
        "WebFetch" => ("fetch", format!("Fetch {}", s("url"))),
        "WebSearch" => ("fetch", format!("Search the web for {}", s("query"))),
        "TodoWrite" => ("think", "Plan".into()),
        "Task" | "Agent" => ("delegate", s("description").to_string()),
        other => ("other", other.to_string()),
    }
}

/// Whether `path`, absolute, from home (`~/`) or relative to `directory`, is inside `directory`.
fn within(path: &str, directory: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{home}{rest}"),
        // "~user/…": someone else's home, never inside.
        Some(_) => return false,
        None => path.to_string(),
    };
    let (Some(full), Some(root)) = (resolve(&Path::new(directory).join(path)), resolve(Path::new(directory))) else { return false };
    full.starts_with(root)
}

/// A path with links followed the way the system follows them: the part that exists
/// canonicalized (`..` after a link leaves the link's target), then the rest, so a file not made
/// yet resolves too. None when that rest climbs with `..`, which can't be checked.
fn resolve(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut existing = path;
    let mut rest = Vec::new();
    while !existing.exists() {
        let (Some(parent), Some(last)) = (existing.parent(), existing.components().next_back()) else { break };
        rest.push(last);
        existing = parent;
    }
    let mut real = existing.canonicalize().ok()?;
    for part in rest.iter().rev() {
        match part {
            Component::Normal(name) => real.push(name),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(real)
}

/// An earlier chat's record, kept by Claude Code under its projects folder by session id.
fn transcript(id: &str) -> Option<Vec<Value>> {
    let config =
        std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".claude"));
    let name = format!("{id}.jsonl");
    let file = std::fs::read_dir(config.join("projects")).ok()?.flatten().map(|d| d.path().join(&name)).find(|p| p.is_file())?;
    let text = std::fs::read_to_string(file).ok()?;
    Some(text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
}

/// An earlier chat's record as the updates that would have built it.
fn replay(session: &str, records: &[Value]) {
    let send = |update: Value| deliver(Update { session: session.to_string(), update });
    for record in records {
        let skip = ["isSidechain", "isMeta"].iter().any(|k| record.get(*k).and_then(Value::as_bool).unwrap_or(false));
        let message = record.get("message").unwrap_or(&Value::Null);
        if skip {
            continue;
        }
        match record.get("type").and_then(Value::as_str) {
            Some("user") => {
                let texts: Vec<String> = match message.get("content") {
                    Some(Value::String(t)) => vec![t.clone()],
                    Some(Value::Array(blocks)) => {
                        blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str().map(str::to_string)).collect()
                    }
                    _ => Vec::new(),
                };
                // Claude Code's own notes ("[Request interrupted by user]") aren't questions.
                for text in texts.into_iter().filter(|t| !t.starts_with("[Request interrupted")) {
                    send(json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": text}}));
                }
                result_updates(message).into_iter().for_each(send);
            }
            Some("assistant") => assistant_updates(message, false, "completed").into_iter().for_each(send),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_get_kinds_and_stay_in_the_folder() {
        assert_eq!(describe("Grep", &json!({"pattern": "cat"})), ("search", "Search for cat".into()));
        assert_eq!(describe("Bash", &json!({"command": "gh pr comment"})).0, "execute");
        let dir = std::env::temp_dir().join(format!("mrow-claude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let session = Session { id: "s".into(), directory: dir.to_string_lossy().into(), state: Mutex::default() };
        assert!(session.inside("read", &json!({"file_path": dir.join("a.rs").to_string_lossy()})));
        assert!(session.inside("read", &json!({"file_path": "sub/new.rs"})));
        assert!(session.inside("search", &json!({"pattern": "*.rs"})));
        assert!(!session.inside("search", &json!({"pattern": "/etc/*"})));
        assert!(!session.inside("read", &json!({"file_path": "/etc/hosts"})));
        assert!(!session.inside("read", &json!({"file_path": "../../.zshrc"})));
        assert!(!session.inside("read", &json!({"file_path": "~/.ssh/id_rsa"})));
        assert!(!session.inside("read", &json!({"notebook_path": "/tmp/x.ipynb"})));
        assert!(!session.inside("read", &json!({})));
        // A link inside the folder, then `..`: the system leaves the link's target.
        std::os::unix::fs::symlink("/etc/ssl", dir.join("docs")).unwrap();
        assert!(!session.inside("read", &json!({"file_path": dir.join("docs/../hostname").to_string_lossy()})));
        assert!(!session.inside("read", &json!({"file_path": dir.join("docs/x").to_string_lossy()})));
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(claude_tools(), "Read,NotebookRead,Grep,Glob,LS");
    }

    #[test]
    fn a_stopped_turns_result_doesnt_end_the_next() {
        let s = Session { id: "t".into(), directory: "/".into(), state: Mutex::default() };
        let (old, old_rx) = channel();
        s.begin(old).unwrap();
        s.interrupt();
        // Sent again before the stopped turn's result came.
        let (new, new_rx) = channel();
        s.begin(new).unwrap();
        assert!(old_rx.recv().is_err(), "the stopped turn's waiter is let go");
        let stream = json!({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "late"}}});
        s.handle(stream);
        assert!(s.state().streamed.is_empty(), "the stopped turn's words are dropped");
        s.handle(json!({"type": "result", "subtype": "error_during_execution", "is_error": true}));
        assert!(new_rx.try_recv().is_err());
        s.handle(json!({"type": "result", "subtype": "success", "stop_reason": "end_turn"}));
        assert_eq!(new_rx.try_recv().unwrap().unwrap()["stopReason"], "end_turn");
    }

    #[test]
    fn earlier_chats_replay() {
        let records = [
            json!({"type": "user", "message": {"role": "user", "content": "Why?"}}),
            json!({"type": "assistant", "message": {"id": "m", "content": [{"type": "thinking", "thinking": "Hm"}, {"type": "tool_use", "id": "t", "name": "Read", "input": {"file_path": "a.rs"}}]}}),
            json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": "x"}]}}),
            json!({"type": "assistant", "isSidechain": true, "message": {"content": [{"type": "text", "text": "skip"}]}}),
        ];
        let kinds: Vec<Value> = records
            .iter()
            .filter(|r| !r.get("isSidechain").is_some())
            .flat_map(|r| match r["type"].as_str() {
                Some("assistant") => assistant_updates(&r["message"], false, "completed"),
                _ => result_updates(&r["message"]),
            })
            .map(|u| u["sessionUpdate"].clone())
            .collect();
        assert_eq!(kinds, ["agent_thought_chunk", "tool_call", "tool_call_update"]);
    }

    #[test]
    fn options_follow_the_model() {
        let mut st = State {
            models: vec![
                json!({"value": "default", "displayName": "Default (recommended)", "description": "Opus 5.5", "supportedEffortLevels": ["low", "xhigh"]}),
                json!({"value": "claude-haiku-4-5", "displayName": "Haiku 4.5"}),
            ],
            model: "default".into(),
            ..State::default()
        };
        let o = options(&st);
        let (model, effort) = crate::data::ask::model::EffortOptions::from(&o).unwrap();
        assert_eq!((model.as_str(), effort.config_id.as_str(), effort.values[1].name.as_str()), ("default", "effort", "Extra High"));
        st.model = "claude-haiku-4-5".into();
        assert_eq!(options(&st).len(), 1);
        assert_eq!(uuid().len(), 36);
    }
}
