//! 连一次 MCP 服务器：初始化，列出工具、资源、提示，然后断开。
//! Multara 不常驻运行 MCP（运行时会自己启动），这里只用于检测状态和查看能力。

use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Stdio},
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::http_client;

const PROTOCOL_VERSION: &str = "2025-06-18";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(40);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_LOG: usize = 20_000;
const MAX_PAGES: usize = 10;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpProbe {
    pub ok: bool,
    /// running | needs_auth | needs_input | error | disabled
    pub status: String,
    pub server_name: String,
    pub server_version: String,
    pub protocol_version: String,
    pub instructions: String,
    pub tools: Vec<McpCapability>,
    pub resources: Vec<McpCapability>,
    pub prompts: Vec<McpCapability>,
    pub log: String,
    pub error: String,
    pub duration_ms: u64,
    pub missing_inputs: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpCapability {
    pub name: String,
    pub title: String,
    pub description: String,
    pub uri: String,
}

pub fn probe(server: &str, entry: &Value) -> McpProbe {
    let started = Instant::now();
    let mut result = McpProbe::default();
    if entry.get("disabled").and_then(Value::as_bool) == Some(true) {
        result.status = "disabled".into();
        return result;
    }
    let prepared = crate::mcp_vars::prepare(server, entry);
    if !prepared.missing.is_empty() {
        result.status = "needs_input".into();
        result.error = tr!("缺少输入：{inputs}", inputs = prepared.missing.join(", "));
        result.missing_inputs = prepared.missing;
        return result;
    }
    let log = Arc::new(Mutex::new(String::new()));
    let outcome = connect(&prepared.entry, log.clone())
        .and_then(|mut connection| run(connection.as_mut(), &mut result));
    result.log = log.lock().map(|text| text.clone()).unwrap_or_default();
    match outcome {
        Ok(()) => {
            result.ok = true;
            result.status = "running".into();
        }
        Err(ProbeError::Auth(message)) => {
            result.status = "needs_auth".into();
            result.error = message;
        }
        Err(ProbeError::Other(message)) => {
            result.status = "error".into();
            result.error = message;
        }
    }
    result.duration_ms = started.elapsed().as_millis() as u64;
    result
}

enum ProbeError {
    Auth(String),
    Other(String),
}

impl From<String> for ProbeError {
    fn from(value: String) -> Self {
        Self::Other(value)
    }
}

type ProbeResult<T> = Result<T, ProbeError>;

trait Connection {
    fn send(&mut self, message: &Value) -> ProbeResult<Option<Value>>;
    fn receive(&mut self, deadline: Instant) -> ProbeResult<Value>;
}

fn run(connection: &mut dyn Connection, result: &mut McpProbe) -> ProbeResult<()> {
    let mut client = Client {
        connection,
        next_id: 1,
    };
    let init = client.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "roots": { "listChanged": false } },
            "clientInfo": { "name": "Multara", "version": env!("CARGO_PKG_VERSION") }
        }),
        STARTUP_TIMEOUT,
    )?;
    let text = |value: Option<&Value>| value.and_then(Value::as_str).unwrap_or("").to_string();
    result.server_name = text(init.pointer("/serverInfo/name"));
    result.server_version = text(init.pointer("/serverInfo/version"));
    result.protocol_version = text(init.get("protocolVersion"));
    result.instructions = text(init.get("instructions"));
    client.notify("notifications/initialized")?;
    let capabilities = init.get("capabilities").cloned().unwrap_or(Value::Null);
    if capabilities.get("tools").is_some() {
        result.tools = client.list("tools/list", "tools")?;
    }
    if capabilities.get("resources").is_some() {
        result.resources = client
            .list("resources/list", "resources")
            .unwrap_or_default();
    }
    if capabilities.get("prompts").is_some() {
        result.prompts = client.list("prompts/list", "prompts").unwrap_or_default();
    }
    Ok(())
}

struct Client<'a> {
    connection: &'a mut dyn Connection,
    next_id: u64,
}

impl Client<'_> {
    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> ProbeResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let deadline = Instant::now() + timeout;
        let mut pending = self.connection.send(&message)?;
        loop {
            let reply = match pending.take() {
                Some(reply) => reply,
                None => self.connection.receive(deadline)?,
            };
            // 服务器反过来发请求（如 roots/list、ping）时要回应，否则它会一直等。
            if let (Some(server_method), Some(server_id)) =
                (reply.get("method").and_then(Value::as_str), reply.get("id"))
            {
                let answer = match server_method {
                    "roots/list" => {
                        json!({ "jsonrpc": "2.0", "id": server_id, "result": { "roots": [] } })
                    }
                    "ping" => json!({ "jsonrpc": "2.0", "id": server_id, "result": {} }),
                    _ => {
                        json!({ "jsonrpc": "2.0", "id": server_id, "error": { "code": -32601, "message": "Multara 检测时不支持此请求" } })
                    }
                };
                pending = self.connection.send(&answer)?;
                continue;
            }
            if reply.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| tr!("未知错误"));
                return Err(ProbeError::Other(tr!(
                    "{method} 失败：{message}",
                    method = method,
                    message = message
                )));
            }
            return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn notify(&mut self, method: &str) -> ProbeResult<()> {
        self.connection
            .send(&json!({ "jsonrpc": "2.0", "method": method }))?;
        Ok(())
    }

    fn list(&mut self, method: &str, key: &str) -> ProbeResult<Vec<McpCapability>> {
        let mut items = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = match &cursor {
                Some(cursor) => json!({ "cursor": cursor }),
                None => json!({}),
            };
            let page = self.request(method, params, REQUEST_TIMEOUT)?;
            for item in page
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let field = |name: &str| {
                    item.get(name)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                };
                items.push(McpCapability {
                    name: field("name"),
                    title: item
                        .pointer("/annotations/title")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| field("title")),
                    description: field("description"),
                    uri: field("uri"),
                });
            }
            cursor = page
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(items)
    }
}

fn connect(entry: &Value, log: Arc<Mutex<String>>) -> ProbeResult<Box<dyn Connection>> {
    let kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
    let url = entry
        .get("url")
        .or_else(|| entry.get("serverUrl"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let headers = string_pairs(entry.get("headers"));
    if !url.is_empty() {
        if kind == "sse" {
            return Ok(Box::new(SseConnection::open(url, headers, log)?));
        }
        return Ok(Box::new(HttpConnection {
            url: url.to_string(),
            headers,
            session: None,
            queue: Vec::new(),
            log,
        }));
    }
    Ok(Box::new(StdioConnection::spawn(entry, log)?))
}

fn string_pairs(value: Option<&Value>) -> Vec<(String, String)> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn append_log(log: &Arc<Mutex<String>>, line: &str) {
    if let Ok(mut text) = log.lock() {
        if text.len() < MAX_LOG {
            text.push_str(line);
            if !line.ends_with('\n') {
                text.push('\n');
            }
        }
    }
}

// ---------- stdio ----------

struct StdioConnection {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    log: Arc<Mutex<String>>,
}

impl StdioConnection {
    fn spawn(entry: &Value, log: Arc<Mutex<String>>) -> ProbeResult<Self> {
        let program = entry
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if program.is_empty() {
            return Err(ProbeError::Other(tr!("没有填写启动命令")));
        }
        let resolved = crate::runtime::resolve_runtime_command(program, "").ok_or_else(|| {
            tr!(
                "找不到命令 {program}。请确认已安装并可在终端中运行，或在 MCP 配置中填写绝对路径。",
                program = program
            )
        })?;
        let mut command = crate::process::command(&resolved);
        command.env("PATH", crate::runtime::augmented_path_env());
        for arg in entry
            .get("args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(arg) = arg.as_str() {
                command.arg(arg);
            } else {
                command.arg(arg.to_string());
            }
        }
        for (key, value) in string_pairs(entry.get("env")) {
            command.env(key, value);
        }
        if let Some(cwd) = entry
            .get("cwd")
            .and_then(Value::as_str)
            .filter(|cwd| !cwd.is_empty())
        {
            command.current_dir(cwd);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // `npx` / `uvx` / `sh -c` 包装的真实服务器是孙进程，放进独立进程组才能一起清理
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        append_log(&log, &format!("启动：{program}"));
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                tr!("找不到命令 {program}。请确认已安装并可在终端中运行，或在 MCP 配置中填写绝对路径。", program = program)
            } else {
                tr!("无法启动 MCP 服务器：{error}", error = error)
            }
        })?;
        let stdin = child.stdin.take().ok_or_else(|| tr!("无法写入子进程"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| tr!("无法读取子进程输出"))?;
        if let Some(stderr) = child.stderr.take() {
            let log = log.clone();
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    append_log(&log, &line);
                }
            });
        }
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            lines,
            log,
        })
    }
}

impl Connection for StdioConnection {
    fn send(&mut self, message: &Value) -> ProbeResult<Option<Value>> {
        writeln!(self.stdin, "{message}")
            .and_then(|_| self.stdin.flush())
            .map_err(|error| tr!("写入失败：{error}", error = error))?;
        Ok(None)
    }

    fn receive(&mut self, deadline: Instant) -> ProbeResult<Value> {
        loop {
            let wait = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(wait) {
                Ok(line) => match serde_json::from_str::<Value>(line.trim()) {
                    Ok(value) => return Ok(value),
                    Err(_) if !line.trim().is_empty() => append_log(&self.log, &line),
                    Err(_) => {}
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(ProbeError::Other(tr!("等待服务器响应超时")))
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let code = self
                        .child
                        .try_wait()
                        .ok()
                        .flatten()
                        .and_then(|status| status.code());
                    return Err(ProbeError::Other(match code {
                        Some(code) => tr!("服务器进程已退出（退出码 {code}）", code = code),
                        None => tr!("服务器进程已退出"),
                    }));
                }
            }
        }
    }
}

impl Drop for StdioConnection {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ = crate::process::command("kill")
            .args(["-KILL", &format!("-{}", self.child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------- Streamable HTTP ----------

struct HttpConnection {
    url: String,
    headers: Vec<(String, String)>,
    session: Option<String>,
    queue: Vec<Value>,
    log: Arc<Mutex<String>>,
}

impl Connection for HttpConnection {
    fn send(&mut self, message: &Value) -> ProbeResult<Option<Value>> {
        let mut headers = self.headers.clone();
        headers.push(("Content-Type".into(), "application/json".into()));
        headers.push((
            "Accept".into(),
            "application/json, text/event-stream".into(),
        ));
        headers.push(("MCP-Protocol-Version".into(), PROTOCOL_VERSION.into()));
        if let Some(session) = &self.session {
            headers.push(("Mcp-Session-Id".into(), session.clone()));
        }
        let response = http_client::request(
            "POST",
            &self.url,
            &headers,
            Some(&message.to_string()),
            REQUEST_TIMEOUT.as_secs() as u32,
        )?;
        append_log(
            &self.log,
            &format!("POST {} → HTTP {}", self.url, response.status),
        );
        if let Some(session) = response.header("mcp-session-id") {
            self.session = Some(session.to_string());
        }
        match response.status {
            401 | 403 => {
                return Err(ProbeError::Auth(tr!(
                    "服务器要求授权（HTTP {status}）",
                    status = response.status
                )))
            }
            200..=299 => {}
            status => {
                let body: String = response.body.chars().take(300).collect();
                return Err(ProbeError::Other(format!("HTTP {status}：{body}")));
            }
        }
        let content_type = response
            .header("content-type")
            .unwrap_or("")
            .to_ascii_lowercase();
        if content_type.contains("text/event-stream") {
            self.queue.extend(sse_messages(&response.body));
        } else if !response.body.trim().is_empty() {
            match serde_json::from_str::<Value>(&response.body) {
                Ok(Value::Array(items)) => self.queue.extend(items),
                Ok(value) => self.queue.push(value),
                Err(_) => append_log(&self.log, &response.body),
            }
        }
        Ok(if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        })
    }

    fn receive(&mut self, _deadline: Instant) -> ProbeResult<Value> {
        if self.queue.is_empty() {
            return Err(ProbeError::Other(tr!("服务器没有返回结果")));
        }
        Ok(self.queue.remove(0))
    }
}

impl Drop for HttpConnection {
    fn drop(&mut self) {
        if let Some(session) = &self.session {
            let mut headers = self.headers.clone();
            headers.push(("Mcp-Session-Id".into(), session.clone()));
            let _ = http_client::request("DELETE", &self.url, &headers, None, 5);
        }
    }
}

fn sse_messages(body: &str) -> Vec<Value> {
    sse_events(body)
        .into_iter()
        .filter(|(event, _)| event.is_empty() || event == "message")
        .filter_map(|(_, data)| serde_json::from_str(&data).ok())
        .collect()
}

fn sse_events(body: &str) -> Vec<(String, String)> {
    let mut events = Vec::new();
    for block in body.replace("\r\n", "\n").split("\n\n") {
        let mut event = String::new();
        let mut data = Vec::new();
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                event = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("data:") {
                data.push(value.strip_prefix(' ').unwrap_or(value).to_string());
            }
        }
        if !data.is_empty() {
            events.push((event, data.join("\n")));
        }
    }
    events
}

// ---------- 旧版 SSE ----------

struct SseConnection {
    child: Child,
    endpoint: String,
    headers: Vec<(String, String)>,
    events: Receiver<(String, String)>,
    log: Arc<Mutex<String>>,
    _header_file: tempfile::NamedTempFile,
}

impl SseConnection {
    fn open(
        url: &str,
        headers: Vec<(String, String)>,
        log: Arc<Mutex<String>>,
    ) -> ProbeResult<Self> {
        let parsed = url::Url::parse(url).map_err(|_| tr!("地址无效：{url}", url = url))?;
        let mut command = crate::process::command("curl");
        command.args([
            "--silent",
            "--show-error",
            "-N",
            "--proto",
            "=http,https",
            "--write-out",
            "\n__HTTP_STATUS__%{http_code}\n",
        ]);
        let mut stream_headers = headers.clone();
        stream_headers.push(("Accept".into(), "text/event-stream".into()));
        let header_file = http_client::curl_header_file(&stream_headers)?;
        command
            .arg("-H")
            .arg(format!("@{}", header_file.path().display()));
        command.arg(parsed.as_str());
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|error| tr!("无法调用 curl：{error}", error = error))?;
        let mut stdout = child.stdout.take().ok_or_else(|| tr!("无法读取 SSE 流"))?;
        let (sender, events) = mpsc::channel();
        thread::spawn(move || {
            let mut buffer = String::new();
            let mut chunk = [0u8; 4096];
            while let Ok(size) = stdout.read(&mut chunk) {
                if size == 0 {
                    break;
                }
                buffer.push_str(&String::from_utf8_lossy(&chunk[..size]).replace("\r\n", "\n"));
                if let Some(status) = buffer.split("__HTTP_STATUS__").nth(1) {
                    let _ = sender.send(("__status".into(), status.trim().to_string()));
                    break;
                }
                while let Some(end) = buffer.find("\n\n") {
                    let block: String = buffer.drain(..end + 2).collect();
                    for event in sse_events(&block) {
                        if sender.send(event).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            let wait = deadline.saturating_duration_since(Instant::now());
            match events.recv_timeout(wait) {
                Ok((event, data)) if event == "endpoint" => {
                    let endpoint = parsed
                        .join(data.trim())
                        .map_err(|_| tr!("服务器返回的端点无效：{data}", data = data))?;
                    append_log(&log, &format!("SSE 端点：{endpoint}"));
                    return Ok(Self {
                        child,
                        endpoint: endpoint.to_string(),
                        headers,
                        events,
                        log,
                        _header_file: header_file,
                    });
                }
                Ok((event, status)) if event == "__status" => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(match status.as_str() {
                        "401" | "403" => ProbeError::Auth(tr!(
                            "服务器要求授权（HTTP {status}）",
                            status = status
                        )),
                        _ => {
                            ProbeError::Other(tr!("SSE 连接失败（HTTP {status}）", status = status))
                        }
                    });
                }
                Ok(_) => {}
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ProbeError::Other(tr!("等待 SSE 端点超时")));
                }
            }
        }
    }
}

impl Connection for SseConnection {
    fn send(&mut self, message: &Value) -> ProbeResult<Option<Value>> {
        let mut headers = self.headers.clone();
        headers.push(("Content-Type".into(), "application/json".into()));
        let response = http_client::request(
            "POST",
            &self.endpoint,
            &headers,
            Some(&message.to_string()),
            REQUEST_TIMEOUT.as_secs() as u32,
        )?;
        match response.status {
            401 | 403 => Err(ProbeError::Auth(tr!(
                "服务器要求授权（HTTP {status}）",
                status = response.status
            ))),
            200..=299 => Ok(None),
            status => Err(ProbeError::Other(format!(
                "HTTP {status}：{}",
                response.body.chars().take(300).collect::<String>()
            ))),
        }
    }

    fn receive(&mut self, deadline: Instant) -> ProbeResult<Value> {
        loop {
            let wait = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(wait) {
                Ok((event, data)) if event.is_empty() || event == "message" => {
                    if let Ok(value) = serde_json::from_str(&data) {
                        return Ok(value);
                    }
                    append_log(&self.log, &data);
                }
                Ok((event, status)) if event == "__status" => {
                    return Err(ProbeError::Other(tr!(
                        "SSE 连接已断开（HTTP {status}）",
                        status = status
                    )));
                }
                Ok(_) => {}
                Err(_) => return Err(ProbeError::Other(tr!("等待服务器响应超时"))),
            }
        }
    }
}

impl Drop for SseConnection {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_sse_blocks() {
        let body = "event: message\ndata: {\"id\":1}\n\nevent: ping\ndata: x\n\n";
        let messages = super::sse_messages(body);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["id"], 1);
    }
}
