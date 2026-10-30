use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::sync::{
    mpsc::{self, Receiver, RecvTimeoutError, Sender},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

const PERMISSION_TIMEOUT: Duration = Duration::from_secs(300);
/// `initialize` / `session/new` 的应答期限；npx 首次拉包会慢，给足余量。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(120);
/// 读线程最多攒这么多行；等权限确认时智能体输出会在这里积压，满了就反压。
const LINE_QUEUE_CAPACITY: usize = 1024;
const INITIALIZE_REQUEST_ID: u64 = 1;
const SESSION_NEW_REQUEST_ID: u64 = 2;
const PROMPT_REQUEST_ID: u64 = 3;

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
        if !self.prompt {
            return permission_option(params);
        }
        if let Some(notify) = &self.notify {
            notify(request_id, params);
        }
        let (tx, rx) = mpsc::channel();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(request_id.to_string(), tx);
        }
        // 超时或通道断开时没人确认过，不能当成允许。
        rx.recv_timeout(PERMISSION_TIMEOUT).unwrap_or_else(|_| {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(request_id);
            }
            reject_option(params)
        })
    }
}

/// Speaks ACP or the DSH stdio protocol and writes one normalized JSON event per line.
pub fn drive<R: Read + Send + 'static, W: Write, E: Write>(
    kind: &str,
    stdout: R,
    stdin: &mut W,
    prompt: &str,
    cwd: &str,
    events: &mut E,
    gate: Option<PermissionGate>,
) {
    let (line_tx, lines) = mpsc::sync_channel::<String>(LINE_QUEUE_CAPACITY);
    thread::spawn(move || {
        for line in crate::process::BoundedLines(BufReader::new(stdout)) {
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    if kind == "dsh" {
        drive_dsh(&lines, stdin, prompt, cwd, events);
    } else {
        drive_acp(&lines, stdin, prompt, cwd, events, gate.as_ref());
    }
}

fn drive_acp<W: Write, E: Write>(
    lines: &Receiver<String>,
    stdin: &mut W,
    prompt: &str,
    cwd: &str,
    events: &mut E,
    gate: Option<&PermissionGate>,
) {
    if call(
        lines,
        stdin,
        INITIALIZE_REQUEST_ID,
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
        lines,
        stdin,
        SESSION_NEW_REQUEST_ID,
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
        Some(PROMPT_REQUEST_ID),
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
    for line in lines.iter() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        // 智能体发来的请求（如权限确认）也可能恰好用 id 3，只有不带 method 的才是本轮应答。
        if value.get("method").is_some() {
            let _ = handle_agent_message(&value, stdin, events, gate);
            continue;
        }
        if id_matches(&value, PROMPT_REQUEST_ID) {
            emit_rpc_error(&value, "session/prompt", events);
            break;
        }
    }
}

fn drive_dsh<W: Write, E: Write>(
    lines: &Receiver<String>,
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
    for line in lines.iter() {
        if let Ok(value) = serde_json::from_str::<Value>(line.trim()) {
            emit_dsh(&value, events);
            if value.get("type").and_then(Value::as_str) == Some("result") {
                break;
            }
        }
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

fn call<W: Write, E: Write>(
    lines: &Receiver<String>,
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
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let line = match lines.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                emit(events, json!({"type":"error","error": format!("{method} 超时：运行时没有应答")}));
                return None;
            }
            Err(RecvTimeoutError::Disconnected) => return None,
        };
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if value.get("method").is_some() {
            let _ = handle_agent_message(&value, stdin, events, gate);
            continue;
        }
        if !id_matches(&value, id) {
            continue;
        }
        if emit_rpc_error(&value, method, events) {
            return None;
        }
        return Some(value.get("result").cloned().unwrap_or(Value::Null));
    }
}

/// 应答的 id 可能被回显成数字或字符串。
fn id_matches(value: &Value, id: u64) -> bool {
    match value.get("id") {
        Some(Value::Number(number)) => number.as_u64() == Some(id),
        Some(Value::String(text)) => text.trim().parse::<u64>().ok() == Some(id),
        _ => false,
    }
}

/// 应答带 `error` 时发出一条错误事件，返回是否出错。
fn emit_rpc_error<E: Write>(value: &Value, method: &str, events: &mut E) -> bool {
    let Some(error) = value.get("error") else {
        return false;
    };
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| error.to_string());
    emit(events, json!({"type":"error","error": format!("{method} 失败：{message}")}));
    true
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
    reject_option(params)
}

fn reject_option(params: Option<&Value>) -> String {
    params
        .and_then(|params| params.get("options"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
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
            let failed = status.eq_ignore_ascii_case("failed") || status.eq_ignore_ascii_case("error");
            let finished = failed || status.eq_ignore_ascii_case("completed");
            // in_progress 带着中间输出时不能当成结果，否则会反复发出 ok 的 tool_result。
            if finished || (status.is_empty() && !output.is_empty()) {
                emit(events, json!({
                    "type": "tool_result",
                    "call_id": call_id,
                    "output": output,
                    "status": if failed { "error" } else { "ok" }
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

#[cfg(test)]
mod tests {
    use super::{drive, id_matches, reject_option};
    use serde_json::json;
    use std::io::Cursor;

    fn run_acp(lines: &[&str]) -> String {
        let stdout = Cursor::new(lines.join("\n").into_bytes());
        let mut stdin = Vec::new();
        let mut events = Vec::new();
        drive("acp", stdout, &mut stdin, "hi", "/tmp", &mut events, None);
        String::from_utf8(events).unwrap()
    }

    #[test]
    fn matches_numeric_and_string_ids() {
        assert!(id_matches(&json!({"id": 3}), 3));
        assert!(id_matches(&json!({"id": "3"}), 3));
        assert!(!id_matches(&json!({"id": "x3"}), 3));
        assert!(!id_matches(&json!({"method": "a"}), 3));
    }

    #[test]
    fn agent_request_with_id_three_does_not_end_the_turn() {
        let events = run_acp(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":"2","result":{"sessionId":"s1"}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"session/request_permission","params":{"options":[{"optionId":"ok","kind":"allow_once"}],"toolCall":{"title":"Edit"}}}"#,
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"text":"still running"}}}}"#,
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"in_progress","content":[{"text":"partial"}]}}}"#,
            r#"{"jsonrpc":"2.0","id":"3","error":{"message":"prompt too long"}}"#,
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"text":"after end"}}}}"#,
        ]);
        assert!(events.contains("still running"));
        assert!(!events.contains("after end"));
        assert!(events.contains("session/prompt 失败：prompt too long"));
        assert!(!events.contains("tool_result"));
    }

    #[test]
    fn handshake_error_is_reported() {
        let events = run_acp(&[r#"{"jsonrpc":"2.0","id":1,"error":{"message":"unsupported"}}"#]);
        assert!(events.contains("initialize 失败：unsupported"));
    }

    #[test]
    fn completed_tool_update_becomes_result() {
        let events = run_acp(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s1"}}"#,
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"failed","content":[{"text":"boom"}]}}}"#,
            r#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#,
        ]);
        assert!(events.contains(r#""type":"tool_result""#));
        assert!(events.contains(r#""status":"error""#));
    }

    #[test]
    fn fallback_after_timeout_rejects() {
        let params = json!({"options":[
            {"optionId":"yes","kind":"allow_once"},
            {"optionId":"no","kind":"reject_once"}
        ]});
        assert_eq!(reject_option(Some(&params)), "no");
        assert_eq!(reject_option(None), "reject_once");
    }
}
