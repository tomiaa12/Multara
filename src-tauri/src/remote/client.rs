use std::{
    collections::HashMap,
    net::{TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Sender},
        Arc, Mutex, OnceLock, PoisonError,
    },
    thread,
    time::{Duration, Instant},
};

use rustls::{ClientConnection, StreamOwned};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::{
    frame::{self, Frame, RemoteError},
    identity, pairing, server, store, tls,
};
use crate::database::AppState;

pub const EVENT: &str = "remote-event";
pub const STATUS_EVENT: &str = "remote-host-status";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_WAIT: Duration = Duration::from_secs(8);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
const BACKOFF_SECS: [u64; 5] = [1, 2, 5, 10, 30];
const WAIT_STEP: Duration = Duration::from_millis(100);
const PAIR_REQUEST_ID: u64 = 1;

type Stream = StreamOwned<ClientConnection, TcpStream>;
type Reply = Result<Value, RemoteError>;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnState {
    Connecting,
    Online,
    Offline,
    Error,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostConnStatus {
    pub host_id: String,
    pub state: ConnState,
    pub error: Option<String>,
}

struct Client {
    host_id: String,
    sender: Mutex<Option<Sender<Frame>>>,
    pending: Mutex<HashMap<u64, Sender<Reply>>>,
    next_id: AtomicU64,
    stop: Arc<AtomicBool>,
    retry_now: AtomicBool,
    status: Mutex<(ConnState, Option<String>)>,
}

impl Client {
    fn state(&self) -> ConnState {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).0
    }

    fn snapshot(&self) -> HostConnStatus {
        let (state, error) = self.status.lock().unwrap_or_else(PoisonError::into_inner).clone();
        HostConnStatus {
            host_id: self.host_id.clone(),
            state,
            error,
        }
    }

    fn set_status(&self, app: &AppHandle, state: ConnState, error: Option<String>) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = (state, error);
        let _ = app.emit(STATUS_EVENT, self.snapshot());
    }

    fn fail_pending(&self, message: &str) {
        let pending: Vec<_> = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .collect();
        pending.into_iter().for_each(|(_, reply)| {
            let _ = reply.send(Err(RemoteError::new("offline", message)));
        });
    }

    fn resolve(&self, id: u64, reply: Reply) {
        let waiter = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
        if let Some(waiter) = waiter {
            let _ = waiter.send(reply);
        }
    }
}

static CLIENTS: OnceLock<Mutex<HashMap<String, Arc<Client>>>> = OnceLock::new();

fn clients() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Client>>> {
    CLIENTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn ensure(app: &AppHandle, host_id: &str) -> Arc<Client> {
    let mut map = clients();
    if let Some(client) = map.get(host_id) {
        client.retry_now.store(true, Ordering::SeqCst);
        return Arc::clone(client);
    }
    let client = Arc::new(Client {
        host_id: host_id.to_string(),
        sender: Mutex::new(None),
        pending: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        stop: Arc::new(AtomicBool::new(false)),
        retry_now: AtomicBool::new(false),
        status: Mutex::new((ConnState::Connecting, None)),
    });
    map.insert(host_id.to_string(), Arc::clone(&client));
    let app = app.clone();
    let supervised = Arc::clone(&client);
    let _ = thread::Builder::new()
        .name("multara-remote-client".into())
        .spawn(move || supervise(&app, &supervised));
    client
}

pub fn connect(app: &AppHandle, host_id: &str) {
    ensure(app, host_id);
}

pub fn disconnect(host_id: &str) {
    if let Some(client) = clients().remove(host_id) {
        client.stop.store(true, Ordering::SeqCst);
        client.fail_pending(&tr!("已断开"));
    }
}

pub fn statuses() -> Vec<HostConnStatus> {
    clients().values().map(|client| client.snapshot()).collect()
}

pub fn invoke(
    app: &AppHandle,
    host_id: &str,
    cmd: &str,
    args: Value,
    approval: Option<String>,
) -> Reply {
    let client = ensure(app, host_id);
    let deadline = Instant::now() + CONNECT_WAIT;
    while client.state() != ConnState::Online {
        if Instant::now() >= deadline {
            let message = client.snapshot().error.unwrap_or_else(|| tr!("主机不在线"));
            return Err(RemoteError::new("offline", message));
        }
        thread::sleep(WAIT_STEP);
    }
    let id = client.next_id.fetch_add(1, Ordering::SeqCst);
    let (reply, receive) = mpsc::channel();
    client
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id, reply);
    let sent = client
        .sender
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|sender| {
            sender
                .send(Frame::Req {
                    id,
                    cmd: cmd.to_string(),
                    args,
                    approval,
                })
                .is_ok()
        });
    if !sent {
        client.resolve(id, Err(RemoteError::new("offline", tr!("主机不在线"))));
    }
    receive.recv_timeout(REQUEST_TIMEOUT).unwrap_or_else(|_| {
        client.resolve(id, Err(RemoteError::new("timeout", tr!("请求超时"))));
        Err(RemoteError::new("timeout", tr!("请求超时")))
    })
}

fn wait_backoff(client: &Client, attempt: usize) {
    let delay = Duration::from_secs(BACKOFF_SECS[attempt.min(BACKOFF_SECS.len() - 1)]);
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline
        && !client.stop.load(Ordering::SeqCst)
        && !client.retry_now.swap(false, Ordering::SeqCst)
    {
        thread::sleep(WAIT_STEP);
    }
}

fn supervise(app: &AppHandle, client: &Arc<Client>) {
    let mut attempt = 0usize;
    while !client.stop.load(Ordering::SeqCst) {
        let host = app
            .state::<AppState>()
            .lock_db()
            .ok()
            .and_then(|db| store::get_host(&db, &client.host_id).ok().flatten());
        let Some(host) = host else {
            client.set_status(app, ConnState::Error, Some(tr!("主机不存在")));
            break;
        };
        client.set_status(app, ConnState::Connecting, None);
        match open(app, &host.address, host.port, Some(host.fingerprint.clone())) {
            Ok((mut stream, _)) => {
                attempt = 0;
                if let Ok(db) = app.state::<AppState>().lock_db() {
                    store::touch_host(&db, &host.id);
                }
                let (sender, outbound) = mpsc::channel();
                *client.sender.lock().unwrap_or_else(PoisonError::into_inner) = Some(sender);
                client.set_status(app, ConnState::Online, None);
                let end = frame::run_loop(&mut stream, &outbound, &client.stop, true, |frame| {
                    on_frame(app, client, frame);
                    None
                });
                *client.sender.lock().unwrap_or_else(PoisonError::into_inner) = None;
                client.fail_pending(&tr!("与主机的连接已断开"));
                let error = match end {
                    frame::LoopEnd::Error(error) => Some(error),
                    frame::LoopEnd::Timeout => Some(tr!("主机无响应")),
                    _ => None,
                };
                client.set_status(app, ConnState::Offline, error);
            }
            Err(error) => {
                client.set_status(app, ConnState::Error, Some(error));
                attempt += 1;
            }
        }
        wait_backoff(client, attempt);
    }
}

fn on_frame(app: &AppHandle, client: &Client, frame: Frame) {
    match frame {
        Frame::Res { id, ok, data, error } => {
            let reply = if ok {
                Ok(data.unwrap_or(Value::Null))
            } else {
                Err(error.unwrap_or_else(|| RemoteError::new("internal", tr!("主机返回了错误"))))
            };
            client.resolve(id, reply);
        }
        Frame::Evt { event, payload } => {
            let _ = app.emit(
                EVENT,
                json!({ "hostId": client.host_id, "event": event, "payload": payload }),
            );
        }
        _ => {}
    }
}

/// 建立 TLS 连接，返回流和主机证书指纹。
fn open(
    app: &AppHandle,
    address: &str,
    port: u16,
    expected_fingerprint: Option<String>,
) -> Result<(Stream, String), String> {
    let identity = identity::load_or_create(&server::app_data(app)?)?;
    let pinned = expected_fingerprint.is_some();
    let (config, seen) = tls::client_config(&identity, expected_fingerprint)?;
    let target = (address.trim(), port)
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| tr!("无法解析主机地址"))?;
    let mut tcp = TcpStream::connect_timeout(&target, CONNECT_TIMEOUT)
        .map_err(|error| tr!("无法连接主机：{error}", error = error))?;
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(CONNECT_TIMEOUT)).map_err(|error| error.to_string())?;
    tcp.set_write_timeout(Some(frame::WRITE_TIMEOUT)).map_err(|error| error.to_string())?;
    let mut tls = ClientConnection::new(config, tls::server_name()).map_err(|error| error.to_string())?;
    while tls.is_handshaking() {
        tls.complete_io(&mut tcp).map_err(|error| {
            if pinned && error.to_string().contains("fingerprint mismatch") {
                tr!("主机身份已变化，可能被冒充；如确认主机重装过，请删除后重新配对")
            } else {
                tr!("TLS 握手失败：{error}", error = error)
            }
        })?;
    }
    tcp.set_read_timeout(Some(frame::READ_POLL)).map_err(|error| error.to_string())?;
    let fingerprint = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or_else(|| tr!("主机没有出示证书"))?;
    Ok((StreamOwned::new(tls, tcp), fingerprint))
}

/// 用配对码与主机配对：双向 HMAC 证明 + 等待主机所有者确认。
pub fn pair(
    app: &AppHandle,
    address: &str,
    port: u16,
    code: &str,
    expected_fingerprint: Option<String>,
    device_name: &str,
) -> Result<store::Host, String> {
    let identity = identity::load_or_create(&server::app_data(app)?)?;
    let (mut stream, host_fingerprint) = open(app, address, port, expected_fingerprint)?;
    let (sender, outbound) = mpsc::channel();
    sender
        .send(Frame::Req {
            id: PAIR_REQUEST_ID,
            cmd: "remote.pair".into(),
            args: json!({
                "deviceName": device_name,
                "proof": pairing::proof(code, &host_fingerprint, &identity.fingerprint),
            }),
            approval: None,
        })
        .map_err(|error| error.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let watchdog = Arc::clone(&stop);
    thread::spawn(move || {
        thread::sleep(pairing::CONFIRM_TIMEOUT + CONNECT_TIMEOUT);
        watchdog.store(true, Ordering::SeqCst);
    });
    let mut result: Option<Reply> = None;
    frame::run_loop(&mut stream, &outbound, &stop, true, |frame| {
        if let Frame::Res { id: PAIR_REQUEST_ID, ok, data, error } = frame {
            result = Some(if ok {
                Ok(data.unwrap_or(Value::Null))
            } else {
                Err(error.unwrap_or_else(|| RemoteError::new("internal", tr!("配对失败"))))
            });
            stop.store(true, Ordering::SeqCst);
        }
        None
    });
    stream.conn.send_close_notify();
    let _ = stream.conn.complete_io(&mut stream.sock);
    let data = result
        .ok_or_else(|| tr!("等待主机确认超时"))?
        .map_err(|error| error.message)?;
    let expected_host_proof = pairing::host_proof(code, &host_fingerprint, &identity.fingerprint);
    let host_proof = data.get("hostProof").and_then(Value::as_str).unwrap_or_default();
    if !host_proof.eq_ignore_ascii_case(&expected_host_proof) {
        return Err(tr!("主机身份校验失败，配对已取消"));
    }
    let host_name = data
        .get("hostName")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(address);
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    store::insert_host(&db, host_name, address.trim(), port, &host_fingerprint)
}
