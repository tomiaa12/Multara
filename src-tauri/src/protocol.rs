use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc, Mutex,
};
use std::time::Duration;

pub struct ByteSender(pub Sender<Vec<u8>>);

impl Write for ByteSender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.send(buf.to_vec()).map_err(|_| io::Error::other("event channel closed"))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct ByteReceiver(pub Receiver<Vec<u8>>, pub Vec<u8>);

impl Read for ByteReceiver {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.1.is_empty() {
            match self.0.recv() {
                Ok(bytes) => self.1 = bytes,
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.1.len());
        out[..n].copy_from_slice(&self.1[..n]);
        self.1.drain(..n);
        Ok(n)
    }
}

use serde_json::{json, Value};

pub type PermissionNotifier = Arc<dyn Fn(&str, Option<&Value>) + Send + Sync>;

#[derive(Clone)]
pub struct PermissionGate {
    pub prompt: bool,
    pub pending: Arc<Mutex<HashMap<String, Sender<String>>>>,
    /// 需要用户确认时通知外部（例如把卡片发到聊天渠道）。
    pub notify: Option<PermissionNotifier>,
}

impl PermissionGate {
    pub fn decide(&self, request_id: &str, params: Option<&Value>) -> String {
        let fallback = permission_option(params);
        if !self.prompt {
            return fallback;
        }
        if let Some(notify) = &self.notify {
            notify(request_id, params);
        }
        let (tx, rx) = mpsc::channel();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(request_id.to_string(), tx);
        }
        rx.recv_timeout(Duration::from_secs(300)).unwrap_or(fallback)
    }
}

/// Speaks ACP or the DSH stdio protocol and writes one normalized JSON event per line.
pub fn drive<R: Read, W: Write, E: Write>(
    kind: &str,
    stdout: R,
    stdin: &mut W,
    prompt: &str,
    cwd: &str,
    events: &mut E,
    gate: Option<PermissionGate>,
) {
    let mut reader = BufReader::new(stdout);
    if kind == "dsh" {
        drive_dsh(&mut reader, stdin, prompt, cwd, events);
    } else {
        drive_acp(&mut reader, stdin, prompt, cwd, events, gate.as_ref());
    }
}

fn drive_acp<R: BufRead, W: Write, E: Write>(
    reader: &mut R,
    stdin: &mut W,
    prompt: &str,
    cwd: &str,
    events: &mut E,
    gate: Option<&PermissionGate>,
) {
    if call(
        reader,
        stdin,
        1,
        "initialize",
        json!({
            "protocolVersion": 1,
            "clientCapabilities": {},
            "clientInfo": { "name": "multara", "version": "0.1.0" }
        }),
        events,
        gate,
    )
    .is_none()
    {
        return;
    }
    let Some(created) = call(
        reader,
        stdin,
        2,
        "session/new",
        json!({ "cwd": cwd, "mcpServers": [] }),
        events,
        gate,
    ) else {
        return;
    };
    let session_id = created
        .pointer("/sessionId")
        .or_else(|| created.pointer("/session/id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if write_rpc(
        stdin,
        Some(3),
        "session/prompt",
        json!({
            "sessionId": session_id,
            "prompt": [{ "type": "text", "text": prompt }]
        }),
    )
    .is_err()
    {
        return;
    }
    let mut line = String::new();
    while reader.read_line(&mut line).ok().filter(|n| *n > 0).is_some() {
        if let Ok(value) = serde_json::from_str::<Value>(line.trim()) {
            let _ = handle_agent_message(&value, stdin, events, gate);
            if value.get("id").and_then(Value::as_u64) == Some(3) {
                break;
            }
        }
        line.clear();
    }
}

fn drive_dsh<R: BufRead, W: Write, E: Write>(
    reader: &mut R,
    stdin: &mut W,
    prompt: &str,
    cwd: &str,
    events: &mut E,
) {
    let command = json!({
        "v": 1,
        "type": "execute",
        "request_id": "multara",
        "cwd": cwd,
        "prompt": prompt,
        "mcp_servers": []
    });
    if writeln!(stdin, "{command}").is_err() {
        return;
    }
    let _ = stdin.flush();
    let mut line = String::new();
    while reader.read_line(&mut line).ok().filter(|n| *n > 0).is_some() {
        if let Ok(value) = serde_json::from_str::<Value>(line.trim()) {
            emit_dsh(&value, events);
            if value.get("type").and_then(Value::as_str) == Some("result") {
                break;
            }
        }
        line.clear();
    }
}

fn emit_dsh<E: Write>(value: &Value, events: &mut E) {
    match value.get("type").and_then(Value::as_str).unwrap_or_default() {
        "text" => emit(events, json!({"type":"text","text": value.get("content").and_then(Value::as_str).unwrap_or_default()})),
        "tool_call" => emit(events, json!({
            "type": "tool_use",
            "id": value.get("call_id").and_then(Value::as_str).unwrap_or_default(),
            "name": value.get("name").and_then(Value::as_str).unwrap_or("tool"),
            "input": value.get("arguments").cloned().unwrap_or(Value::Null)
        })),
        "tool_result" => emit(events, json!({
            "type": "tool_result",
            "call_id": value.get("call_id").and_then(Value::as_str).unwrap_or_default(),
            "output": value.get("output").and_then(Value::as_str).unwrap_or_default()
        })),
        "error" => emit(events, json!({
            "type": "error",
            "error": value.pointer("/error/message").and_then(Value::as_str).or_else(|| value.get("message").and_then(Value::as_str)).unwrap_or("运行时返回错误")
        })),
        _ => {}
    }
}

fn emit<E: Write>(events: &mut E, value: Value) {
    if let Ok(line) = serde_json::to_string(&value) {
        let _ = writeln!(events, "{line}");
        let _ = events.flush();
    }
}

fn call<R: BufRead, W: Write, E: Write>(
    reader: &mut R,
    stdin: &mut W,
    id: u64,
    method: &str,
    params: Value,
    events: &mut E,
    gate: Option<&PermissionGate>,
) -> Option<Value> {
    if write_rpc(stdin, Some(id), method, params).is_err() {
        return None;
    }
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok().filter(|n| *n > 0).is_none() {
            return None;
        }
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if value.get("method").is_some() {
            let _ = handle_agent_message(&value, stdin, events, gate);
            continue;
        }
        if value.get("id").and_then(Value::as_u64) == Some(id)
            || value.get("id").and_then(Value::as_i64) == Some(id as i64)
        {
            if value.get("error").is_some() {
                return None;
            }
            return Some(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

fn handle_agent_message<W: Write, E: Write>(
    value: &Value,
    stdin: &mut W,
    events: &mut E,
    gate: Option<&PermissionGate>,
) -> bool {
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return false;
    };
    if method == "session/update" {
        emit_acp_update(value.get("params"), events);
        return value.get("id").is_none();
    }
    let Some(id) = value.get("id") else {
        return true;
    };
    let result = if method == "session/request_permission" {
        let params = value.get("params");
        let request_id = match id {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            other => other.to_string(),
        };
        emit(
            events,
            json!({
                "type": "permission",
                "id": request_id,
                "tool": params
                    .and_then(|params| params.pointer("/toolCall/title").or_else(|| params.pointer("/toolCall/name")))
                    .and_then(Value::as_str)
                    .unwrap_or("tool"),
                "input": params.and_then(|params| params.get("toolCall")).cloned().unwrap_or(Value::Null),
                "options": params.and_then(|params| params.get("options")).cloned().unwrap_or(Value::Null)
            }),
        );
        let option_id = match gate {
            Some(gate) => gate.decide(&request_id, params),
            None => permission_option(params),
        };
        json!({ "outcome": { "outcome": "selected", "optionId": option_id } })
    } else {
        return write_rpc_error(stdin, id, -32601, &format!("method not found: {method}")).is_ok();
    };
    write_result(stdin, id, result).is_ok()
}

fn permission_option(params: Option<&Value>) -> String {
    let options = params
        .and_then(|params| params.get("options"))
        .and_then(Value::as_array);
    let options = options.map(Vec::as_slice).unwrap_or(&[]);
    for want in ["allow_session", "approve_for_session"] {
        if let Some(id) = options.iter().find_map(|option| {
            let id = option.get("optionId").and_then(Value::as_str)?;
            let kind = option.get("kind").and_then(Value::as_str).unwrap_or("");
            (id == want && kind.to_ascii_lowercase().starts_with("allow")).then_some(id)
        }) {
            return id.to_string();
        }
    }
    if let Some(id) = options.iter().find_map(|option| {
        let kind = option.get("kind").and_then(Value::as_str).unwrap_or("");
        (kind.eq_ignore_ascii_case("allow_once"))
            .then(|| option.get("optionId").and_then(Value::as_str))
            .flatten()
    }) {
        return id.to_string();
    }
    options
        .iter()
        .find_map(|option| {
            let kind = option.get("kind").and_then(Value::as_str).unwrap_or("");
            kind.eq_ignore_ascii_case("reject_once")
                .then(|| option.get("optionId").and_then(Value::as_str))
                .flatten()
        })
        .unwrap_or("reject_once")
        .to_string()
}

fn emit_acp_update<E: Write>(params: Option<&Value>, events: &mut E) {
    let Some(update) = params.and_then(|params| params.get("update")) else {
        return;
    };
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match kind.as_str() {
        "agent_message_chunk" | "agentmessagechunk" => {
            let text = update
                .pointer("/content/text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !text.is_empty() {
                emit(events, json!({"type":"text","text": text}));
            }
        }
        "agent_thought_chunk" | "agentthoughtchunk" => {
            let text = update
                .pointer("/content/text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !text.is_empty() {
                emit(events, json!({"type":"thinking","thinking": text}));
            }
        }
        "tool_call" | "toolcall" => emit(events, json!({
            "type": "tool_use",
            "id": update.get("toolCallId").and_then(Value::as_str).unwrap_or_default(),
            "name": update.get("name").or_else(|| update.get("title")).and_then(Value::as_str).unwrap_or("tool"),
            "input": update.get("parameters").or_else(|| update.get("rawInput")).cloned().unwrap_or(Value::Null),
            "status": update.get("status").and_then(Value::as_str).unwrap_or("pending")
        })),
        "tool_call_update" | "toolcallupdate" => {
            let call_id = update.get("toolCallId").and_then(Value::as_str).unwrap_or_default();
            let status = update.get("status").and_then(Value::as_str).unwrap_or("");
            let output = update
                .get("output")
                .and_then(Value::as_str)
                .or_else(|| update.pointer("/content/0/content/text").and_then(Value::as_str))
                .or_else(|| update.pointer("/content/0/text").and_then(Value::as_str))
                .unwrap_or("");
            if status.eq_ignore_ascii_case("completed")
                || status.eq_ignore_ascii_case("failed")
                || !output.is_empty()
            {
                emit(events, json!({
                    "type": "tool_result",
                    "call_id": call_id,
                    "output": output,
                    "status": if status.eq_ignore_ascii_case("failed") || status.eq_ignore_ascii_case("error") {
                        "error"
                    } else {
                        "ok"
                    }
                }));
            } else if !status.is_empty() {
                emit(events, json!({
                    "type": "tool_use",
                    "id": call_id,
                    "status": status
                }));
            }
        }
        "plan" => emit(events, json!({
            "type": "plan",
            "input": update.get("entries").cloned().unwrap_or(Value::Null)
        })),
        "available_commands_update" | "availablecommandsupdate" => emit(events, json!({
            "type": "commands",
            "input": update.get("availableCommands").or_else(|| update.get("available_commands")).cloned().unwrap_or(Value::Null)
        })),
        "current_mode_update" | "currentmodeupdate" => emit(events, json!({
            "type": "mode",
            "content": update.get("currentModeId").or_else(|| update.get("modeId")).and_then(Value::as_str).unwrap_or_default()
        })),
        _ => {}
    }
}

fn write_rpc<W: Write>(stdin: &mut W, id: Option<u64>, method: &str, params: Value) -> std::io::Result<()> {
    let mut body = json!({ "jsonrpc": "2.0", "method": method, "params": params });
    if let Some(id) = id {
        body["id"] = json!(id);
    }
    writeln!(stdin, "{body}")?;
    stdin.flush()
}

fn write_result<W: Write>(stdin: &mut W, id: &Value, result: Value) -> std::io::Result<()> {
    let body = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    writeln!(stdin, "{body}")?;
    stdin.flush()
}

fn write_rpc_error<W: Write>(stdin: &mut W, id: &Value, code: i32, message: &str) -> std::io::Result<()> {
    let body = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } });
    writeln!(stdin, "{body}")?;
    stdin.flush()
}
