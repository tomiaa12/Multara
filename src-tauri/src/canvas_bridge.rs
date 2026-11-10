//! Canvas 页面与 Multara 之间的 `__multara/*` 端点（方案 §5.5、§6）。
//!
//! | 端点 | 方向 |
//! |---|---|
//! | `GET  state` | 页面读实例信息 |
//! | `GET  stream` | SSE：智能体的 `action`、重开后的 `input` |
//! | `POST result` | 页面回 action 结果 |
//! | `POST message` | 页面想往输入框里填一段话（只填不发，§6） |
//! | `POST context` | 页面挂一份上下文，下一轮随消息带上（只覆盖最新一份） |
//! | `GET/PUT/DELETE storage/<key>` | 持久化键值，原子写、带格式标记、损坏不覆盖 |
//!
//! 同一实例同时只有一个活页面（前端单视图认领，R16）：新的 SSE 连接顶掉旧的，action 只投给它。

use std::{
    collections::HashMap,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    database::AppState,
    ops::canvas::{self as ops, CanvasError, HostingView},
    ui_host::{HostRequest, HostResponse},
};

const KEEPALIVE: Duration = Duration::from_secs(15);
const STORAGE_FORMAT: &str = "multara-canvas-storage";
const MAX_STORAGE_VALUE: usize = 1024 * 1024;
const MAX_STORAGE_TOTAL: u64 = 20 * 1024 * 1024;
const MAX_CONTEXT_BYTES: usize = 64 * 1024;
/// 前端处理 `message` 的等待时间：没人应答视为会话没在任何视图打开
const OUTBOUND_REPLY_TIMEOUT: Duration = Duration::from_secs(3);

type Key = (String, String);

struct Stream {
    id: u64,
    sender: Sender<String>,
}

#[derive(Default)]
struct Bridge {
    streams: HashMap<Key, Stream>,
    pending: HashMap<String, (Key, Sender<Value>)>,
    outbound: HashMap<String, Sender<Value>>,
    next_id: u64,
}

fn bridge() -> &'static Mutex<Bridge> {
    static BRIDGE: OnceLock<Mutex<Bridge>> = OnceLock::new();
    BRIDGE.get_or_init(Default::default)
}

fn key(session_id: &str, instance_id: &str) -> Key {
    (session_id.to_string(), instance_id.to_string())
}

fn sse(event: &str, data: &Value) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

/// SSE 响应体：从通道里取事件，15 秒没事件写一行注释保活；通道断开即结束
struct SseBody {
    receiver: Receiver<String>,
    buffer: Vec<u8>,
    position: usize,
    key: Key,
    id: u64,
    connections: Arc<AtomicU64>,
}

impl Read for SseBody {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.buffer.len() {
            let next = match self.receiver.recv_timeout(KEEPALIVE) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => ": keepalive\n\n".to_string(),
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            };
            self.buffer = next.into_bytes();
            self.position = 0;
        }
        let count = (self.buffer.len() - self.position).min(out.len());
        out[..count].copy_from_slice(&self.buffer[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

impl Drop for SseBody {
    fn drop(&mut self) {
        self.connections.fetch_sub(1, Ordering::Relaxed);
        if let Ok(mut bridge) = bridge().lock() {
            if bridge.streams.get(&self.key).is_some_and(|stream| stream.id == self.id) {
                bridge.streams.remove(&self.key);
            }
        }
    }
}

fn open_stream(session_id: &str, instance_id: &str, connections: &Arc<AtomicU64>) -> HostResponse {
    let (sender, receiver) = mpsc::channel();
    let _ = sender.send("retry: 1000\n: connected\n\n".to_string());
    let Ok(mut bridge) = bridge().lock() else {
        return HostResponse::Status(503);
    };
    bridge.next_id += 1;
    let id = bridge.next_id;
    // 新连接顶掉旧的：旧 sender 被丢弃，旧连接读到 EOF 后关闭
    bridge.streams.insert(key(session_id, instance_id), Stream { id, sender });
    connections.fetch_add(1, Ordering::Relaxed);
    HostResponse::Stream {
        content_type: "text/event-stream; charset=utf-8",
        body: Box::new(SseBody {
            receiver,
            buffer: Vec::new(),
            position: 0,
            key: key(session_id, instance_id),
            id,
            connections: connections.clone(),
        }),
    }
}

/// 当前有没有活页面连着
pub fn is_connected(session_id: &str, instance_id: &str) -> bool {
    bridge()
        .lock()
        .map(|bridge| bridge.streams.contains_key(&key(session_id, instance_id)))
        .unwrap_or(false)
}

/// 同实例重开换了输入：推给已连接的页面
pub fn push_input(session_id: &str, instance_id: &str, input: &Value) {
    if let Ok(bridge) = bridge().lock() {
        if let Some(stream) = bridge.streams.get(&key(session_id, instance_id)) {
            let _ = stream.sender.send(sse("input", &json!({ "input": input })));
        }
    }
}

/// 把 action 投给唯一的活页面并等结果（方案 §5.6）
pub fn deliver_action(
    session_id: &str,
    instance_id: &str,
    name: &str,
    input: &Value,
    timeout: Duration,
) -> Result<Value, CanvasError> {
    let call_id = uuid::Uuid::new_v4().simple().to_string();
    let (sender, receiver) = mpsc::channel();
    {
        let mut bridge = bridge()
            .lock()
            .map_err(|_| CanvasError::new("canvas.internal", "lock"))?;
        let Some(stream) = bridge.streams.get(&key(session_id, instance_id)) else {
            return Err(CanvasError::new(
                "canvas.not_connected",
                tr!("Canvas 未显示，请让用户打开后重试"),
            ));
        };
        let event = sse("action", &json!({ "callId": call_id, "name": name, "input": input }));
        if stream.sender.send(event).is_err() {
            return Err(CanvasError::new(
                "canvas.not_connected",
                tr!("Canvas 未显示，请让用户打开后重试"),
            ));
        }
        bridge
            .pending
            .insert(call_id.clone(), (key(session_id, instance_id), sender));
    }
    let result = receiver.recv_timeout(timeout);
    if let Ok(mut bridge) = bridge().lock() {
        bridge.pending.remove(&call_id);
    }
    let value = match result {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => {
            return Err(CanvasError::new(
                "canvas.timeout",
                tr!("页面 {secs} 秒内没有回应", secs = timeout.as_secs()),
            ))
        }
        Err(RecvTimeoutError::Disconnected) => {
            return Err(CanvasError::new("canvas.not_connected", tr!("页面已关闭")))
        }
    };
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(value.get("value").cloned().unwrap_or(Value::Null));
    }
    let code = value.get("code").and_then(Value::as_str).unwrap_or("error");
    let message = value.get("message").and_then(Value::as_str).unwrap_or("");
    Err(CanvasError::new(
        "canvas.action_failed",
        format!("{code}: {message}"),
    ))
}

/// 关掉实例：断开页面，挂起的 action 立即失败
pub fn drop_instance(session_id: &str, instance_id: &str) {
    if let Ok(mut bridge) = bridge().lock() {
        let target = key(session_id, instance_id);
        bridge.streams.remove(&target);
        bridge.pending.retain(|_, (owner, _)| owner != &target);
    }
}

/// 前端对 `ui-outbound` 的应答（`fillComposerFromUi` 的结果）
pub fn resolve_outbound(request_id: &str, result: Value) {
    if let Ok(mut bridge) = bridge().lock() {
        if let Some(sender) = bridge.outbound.remove(request_id) {
            let _ = sender.send(result);
        }
    }
}

fn wait_outbound(app: &AppHandle, payload: Value) -> Value {
    let request_id = uuid::Uuid::new_v4().simple().to_string();
    let (sender, receiver) = mpsc::channel();
    if let Ok(mut bridge) = bridge().lock() {
        bridge.outbound.insert(request_id.clone(), sender);
    }
    let mut payload = payload;
    payload["requestId"] = json!(request_id);
    let _ = app.emit("ui-outbound", payload);
    let result = receiver
        .recv_timeout(OUTBOUND_REPLY_TIMEOUT)
        .unwrap_or_else(|_| json!({ "ok": false, "reason": "session_closed" }));
    if let Ok(mut bridge) = bridge().lock() {
        bridge.outbound.remove(&request_id);
    }
    result
}

fn bad(message: impl Into<String>) -> HostResponse {
    HostResponse::Json(400, json!({ "ok": false, "code": "canvas.invalid_input", "message": message.into() }))
}

fn body_json(request: &HostRequest) -> Result<Value, HostResponse> {
    serde_json::from_slice(&request.body).map_err(|error| bad(error.to_string()))
}

pub fn handle(
    app: &AppHandle,
    session_id: &str,
    instance_id: &str,
    view: &HostingView,
    endpoint: &str,
    request: &HostRequest,
    connections: &Arc<AtomicU64>,
) -> HostResponse {
    let method = request.method.as_str();
    match (method, endpoint) {
        ("GET", "state") => HostResponse::Json(
            200,
            json!({
                "instanceId": instance_id,
                "canvasId": view.canvas_id,
                "title": view.title,
                "input": view.input,
                "actions": view.manifest.actions.iter().map(|action| json!({
                    "name": action.name,
                    "description": action.description,
                })).collect::<Vec<_>>(),
            }),
        ),
        ("GET", "stream") => open_stream(session_id, instance_id, connections),
        ("POST", "result") => {
            let body = match body_json(request) {
                Ok(body) => body,
                Err(response) => return response,
            };
            let Some(call_id) = body.get("callId").and_then(Value::as_str) else {
                return bad("callId");
            };
            let sender = bridge().lock().ok().and_then(|mut bridge| {
                let owned = bridge
                    .pending
                    .get(call_id)
                    .is_some_and(|(owner, _)| owner == &key(session_id, instance_id));
                owned.then(|| bridge.pending.remove(call_id)).flatten()
            });
            match sender {
                Some((_, sender)) => {
                    let _ = sender.send(body);
                    HostResponse::Json(200, json!({ "ok": true }))
                }
                None => HostResponse::Json(404, json!({ "ok": false, "code": "canvas.unknown_call" })),
            }
        }
        ("POST", "message") => {
            let body = match body_json(request) {
                Ok(body) => body,
                Err(response) => return response,
            };
            let text = body.get("text").and_then(Value::as_str).unwrap_or("").trim();
            if text.is_empty() || text.len() > MAX_CONTEXT_BYTES {
                return bad("text");
            }
            let result = wait_outbound(
                app,
                json!({
                    "sessionId": session_id,
                    "source": "canvas",
                    "sourceId": instance_id,
                    "title": view.title,
                    "kind": "message",
                    "text": text,
                }),
            );
            HostResponse::Json(200, result)
        }
        ("POST", "context") => {
            let body = match body_json(request) {
                Ok(body) => body,
                Err(response) => return response,
            };
            if body.to_string().len() > MAX_CONTEXT_BYTES {
                return HostResponse::Json(413, json!({ "ok": false, "code": "canvas.too_large" }));
            }
            let text = body.get("text").and_then(Value::as_str).map(str::to_string);
            let data = body.get("data").cloned().filter(|value| !value.is_null());
            if text.is_none() && data.is_none() {
                return bad("text / data");
            }
            let context = json!({ "text": text, "data": data });
            {
                let state = app.state::<AppState>();
                let Ok(db) = state.lock_db() else {
                    return HostResponse::Status(503);
                };
                if let Err(error) = ops::set_context(&db, session_id, instance_id, &context) {
                    return HostResponse::Json(400, error.to_json());
                }
            }
            let _ = app.emit(
                "ui-outbound",
                json!({
                    "sessionId": session_id,
                    "source": "canvas",
                    "sourceId": instance_id,
                    "title": view.title,
                    "kind": "context",
                    "text": text,
                    "data": data,
                }),
            );
            HostResponse::Json(200, json!({ "ok": true }))
        }
        (_, endpoint) if endpoint.starts_with("storage/") => {
            let Some(app_data) = crate::canvas::app_data(app) else {
                return HostResponse::Status(503);
            };
            let key = &endpoint["storage/".len()..];
            storage(&ops::data_root(&app_data, &view.canvas_id), key, request)
        }
        _ => HostResponse::Status(405),
    }
}

// ---------- storage ----------

fn valid_key(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// 版本号：文件修改时间（纳秒）。没有文件时为 "0"
fn version_of(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|| "0".to_string())
}

fn dir_size(dir: &Path, except: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path() != except)
                .filter_map(|entry| entry.metadata().ok())
                .filter(|meta| meta.is_file())
                .map(|meta| meta.len())
                .sum()
        })
        .unwrap_or(0)
}

fn corrupt() -> HostResponse {
    HostResponse::Json(
        409,
        json!({ "ok": false, "code": "canvas.storage_corrupt", "message": tr!("存储的数据损坏或格式不认识，未覆盖") }),
    )
}

fn read_wrapped(path: &Path) -> Result<Option<Value>, HostResponse> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(HostResponse::Status(503)),
    };
    let wrapped: Value = serde_json::from_slice(&raw).map_err(|_| corrupt())?;
    if wrapped.get("format").and_then(Value::as_str) != Some(STORAGE_FORMAT)
        || wrapped.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Err(corrupt());
    }
    Ok(Some(wrapped.get("value").cloned().unwrap_or(Value::Null)))
}

pub fn storage(dir: &Path, key: &str, request: &HostRequest) -> HostResponse {
    if !valid_key(key) {
        return bad("key");
    }
    let path: PathBuf = dir.join(format!("{key}.json"));
    let expected = request.header("if-match").map(|value| value.trim_matches('"').to_string());
    let check_version = || -> Option<HostResponse> {
        let expected = expected.as_ref()?;
        (expected != &version_of(&path)).then(|| {
            HostResponse::Json(
                412,
                json!({ "ok": false, "code": "canvas.storage_conflict", "version": version_of(&path) }),
            )
        })
    };
    match request.method.as_str() {
        "GET" => match read_wrapped(&path) {
            Ok(value) => HostResponse::Json(200, json!({ "ok": true, "value": value, "version": version_of(&path) })),
            Err(response) => response,
        },
        "PUT" => {
            if request.body.len() > MAX_STORAGE_VALUE + 64 {
                return HostResponse::Json(413, json!({ "ok": false, "code": "canvas.too_large" }));
            }
            let body: Value = match serde_json::from_slice(&request.body) {
                Ok(body) => body,
                Err(error) => return bad(error.to_string()),
            };
            let Some(value) = body.get("value") else {
                return bad("value");
            };
            // 现有文件损坏时不覆盖，让页面先处理
            if let Err(response) = read_wrapped(&path) {
                return response;
            }
            if let Some(conflict) = check_version() {
                return conflict;
            }
            let wrapped = json!({ "format": STORAGE_FORMAT, "version": 1, "value": value }).to_string();
            if wrapped.len() > MAX_STORAGE_VALUE + 64 {
                return HostResponse::Json(413, json!({ "ok": false, "code": "canvas.too_large" }));
            }
            if fs::create_dir_all(dir).is_err() {
                return HostResponse::Status(503);
            }
            if dir_size(dir, &path) + wrapped.len() as u64 > MAX_STORAGE_TOTAL {
                return HostResponse::Json(413, json!({ "ok": false, "code": "canvas.quota" }));
            }
            // 同目录临时文件 + rename：并发读不会看到写了一半的文件
            let written = tempfile::NamedTempFile::new_in(dir).and_then(|mut file| {
                file.write_all(wrapped.as_bytes())?;
                file.as_file().sync_all()?;
                file.persist(&path).map_err(|error| error.error)?;
                Ok(())
            });
            match written {
                Ok(()) => HostResponse::Json(200, json!({ "ok": true, "version": version_of(&path) })),
                Err(_) => HostResponse::Status(503),
            }
        }
        "DELETE" => {
            if let Some(conflict) = check_version() {
                return conflict;
            }
            match fs::remove_file(&path) {
                Ok(()) => HostResponse::Json(200, json!({ "ok": true })),
                Err(error) if error.kind() == io::ErrorKind::NotFound => HostResponse::Json(200, json!({ "ok": true })),
                Err(_) => HostResponse::Status(503),
            }
        }
        _ => HostResponse::Status(405),
    }
}

#[allow(dead_code)]
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, body: &str, if_match: Option<&str>) -> HostRequest {
        let mut headers = HashMap::new();
        if let Some(version) = if_match {
            headers.insert("if-match".to_string(), version.to_string());
        }
        HostRequest {
            method: method.into(),
            path: String::new(),
            query: String::new(),
            headers,
            body: body.as_bytes().to_vec(),
        }
    }

    fn json_of(response: HostResponse) -> (u16, Value) {
        match response {
            HostResponse::Json(code, value) => (code, value),
            HostResponse::Status(code) => (code, Value::Null),
            _ => panic!("unexpected response"),
        }
    }

    #[test]
    fn storage_round_trip_and_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("board");
        let (code, empty) = json_of(storage(&root, "items", &request("GET", "", None)));
        assert_eq!(code, 200);
        assert!(empty["value"].is_null());
        assert_eq!(empty["version"], "0");
        let (code, saved) = json_of(storage(&root, "items", &request("PUT", r#"{"value":[1,2]}"#, Some("0"))));
        assert_eq!(code, 200, "{saved}");
        let version = saved["version"].as_str().unwrap().to_string();
        let (_, read) = json_of(storage(&root, "items", &request("GET", "", None)));
        assert_eq!(read["value"], json!([1, 2]));
        // 版本不对：412
        let (code, _) = json_of(storage(&root, "items", &request("PUT", r#"{"value":3}"#, Some("1"))));
        assert_eq!(code, 412);
        let (code, _) = json_of(storage(&root, "items", &request("PUT", r#"{"value":3}"#, Some(&version))));
        assert_eq!(code, 200);
        // 文件外包格式标记
        let raw: Value = serde_json::from_str(&fs::read_to_string(root.join("items.json")).unwrap()).unwrap();
        assert_eq!(raw["format"], STORAGE_FORMAT);
        assert_eq!(raw["version"], 1);
        let (code, _) = json_of(storage(&root, "items", &request("DELETE", "", None)));
        assert_eq!(code, 200);
        assert!(!root.join("items.json").exists());
    }

    #[test]
    fn corrupt_storage_is_reported_and_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("items.json"), "{oops").unwrap();
        let (code, body) = json_of(storage(dir.path(), "items", &request("GET", "", None)));
        assert_eq!(code, 409);
        assert_eq!(body["code"], "canvas.storage_corrupt");
        let (code, _) = json_of(storage(dir.path(), "items", &request("PUT", r#"{"value":1}"#, None)));
        assert_eq!(code, 409);
        assert_eq!(fs::read_to_string(dir.path().join("items.json")).unwrap(), "{oops");
        fs::write(dir.path().join("other.json"), r#"{"format":"x","version":1,"value":1}"#).unwrap();
        assert_eq!(json_of(storage(dir.path(), "other", &request("GET", "", None))).0, 409);
    }

    #[test]
    fn storage_rejects_bad_keys_and_quota() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(json_of(storage(dir.path(), "../x", &request("GET", "", None))).0, 400);
        assert_eq!(json_of(storage(dir.path(), "a.b", &request("GET", "", None))).0, 400);
        let big = format!(r#"{{"value":"{}"}}"#, "x".repeat(MAX_STORAGE_VALUE + 10));
        assert_eq!(json_of(storage(dir.path(), "big", &request("PUT", &big, None))).0, 413);
    }

    #[test]
    fn actions_need_a_live_page() {
        let error = deliver_action("s-none", "i", "go", &Value::Null, Duration::from_millis(50)).unwrap_err();
        assert_eq!(error.code, "canvas.not_connected");
    }

    #[test]
    fn action_round_trip_through_the_stream() {
        let connections = Arc::new(AtomicU64::new(0));
        let HostResponse::Stream { mut body, .. } = open_stream("s-rt", "i", &connections) else {
            panic!("expected stream");
        };
        assert!(is_connected("s-rt", "i"));
        assert_eq!(connections.load(Ordering::Relaxed), 1);
        let worker = std::thread::spawn(|| {
            deliver_action("s-rt", "i", "go", &json!({ "n": 1 }), Duration::from_secs(5))
        });
        // 读出 connected 注释和 action 事件
        let mut seen = String::new();
        let mut buffer = [0u8; 1024];
        while !seen.contains("event: action") {
            let read = body.read(&mut buffer).unwrap();
            seen.push_str(&String::from_utf8_lossy(&buffer[..read]));
        }
        let data_line = seen.lines().find(|line| line.starts_with("data: ")).unwrap();
        let event: Value = serde_json::from_str(&data_line["data: ".len()..]).unwrap();
        assert_eq!(event["name"], "go");
        let call_id = event["callId"].as_str().unwrap().to_string();
        let sender = bridge().lock().unwrap().pending.remove(&call_id).unwrap().1;
        sender.send(json!({ "callId": call_id, "ok": true, "value": 42 })).unwrap();
        assert_eq!(worker.join().unwrap().unwrap(), json!(42));
        drop(body);
        assert!(!is_connected("s-rt", "i"));
        assert_eq!(connections.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_new_stream_replaces_the_old_one() {
        let connections = Arc::new(AtomicU64::new(0));
        let HostResponse::Stream { body: mut first, .. } = open_stream("s-re", "i", &connections) else {
            panic!();
        };
        let HostResponse::Stream { body: _second, .. } = open_stream("s-re", "i", &connections) else {
            panic!();
        };
        // 旧连接的发送端被丢弃：读完缓冲后 EOF
        let mut all = Vec::new();
        first.read_to_end(&mut all).unwrap();
        assert!(String::from_utf8_lossy(&all).contains("connected"));
        drop(first);
        assert!(is_connected("s-re", "i"), "dropping the replaced stream must not unregister the new one");
    }

    #[test]
    fn page_errors_become_action_failed() {
        let connections = Arc::new(AtomicU64::new(0));
        let HostResponse::Stream { mut body, .. } = open_stream("s-err", "i", &connections) else {
            panic!();
        };
        let worker = std::thread::spawn(|| deliver_action("s-err", "i", "go", &Value::Null, Duration::from_secs(5)));
        let mut seen = String::new();
        let mut buffer = [0u8; 1024];
        while !seen.contains("event: action") {
            let read = body.read(&mut buffer).unwrap();
            seen.push_str(&String::from_utf8_lossy(&buffer[..read]));
        }
        let data_line = seen.lines().find(|line| line.starts_with("data: ")).unwrap();
        let call_id = serde_json::from_str::<Value>(&data_line[6..]).unwrap()["callId"].as_str().unwrap().to_string();
        let sender = bridge().lock().unwrap().pending.remove(&call_id).unwrap().1;
        sender.send(json!({ "ok": false, "code": "bad_item", "message": "no such item" })).unwrap();
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(error.code, "canvas.action_failed");
        assert!(error.message.contains("bad_item"));
    }

    #[test]
    fn timeouts_are_reported() {
        let connections = Arc::new(AtomicU64::new(0));
        let HostResponse::Stream { body: _body, .. } = open_stream("s-to", "i", &connections) else {
            panic!();
        };
        let error = deliver_action("s-to", "i", "go", &Value::Null, Duration::from_millis(100)).unwrap_err();
        assert_eq!(error.code, "canvas.timeout");
    }
}
