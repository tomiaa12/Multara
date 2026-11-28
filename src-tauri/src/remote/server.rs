use std::{
    collections::HashMap,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Sender},
        Arc, Mutex, OnceLock, PoisonError,
    },
    thread,
    time::{Duration, Instant},
};

use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{
    frame::{self, Frame, RemoteError},
    gateway, identity,
    network::{self, LocalNetwork},
    store, tls,
};
use crate::database::AppState;

const ACCEPT_POLL: Duration = Duration::from_millis(200);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// 每条连接同时处理的请求数上限；超过直接拒绝，避免一个设备占满线程。
const MAX_INFLIGHT: usize = 8;
/// 令牌桶：每秒补充的请求数与桶容量。
const RATE_PER_SEC: f64 = 30.0;
const RATE_BURST: f64 = 60.0;
const MAX_UNPAIRED_PER_IP: usize = 2;
pub const DEVICES_CHANGED_EVENT: &str = "remote-devices-changed";

pub struct Conn {
    pub id: u64,
    pub address: String,
    pub fingerprint: String,
    device_id: Mutex<Option<String>>,
    sender: Sender<Frame>,
    stop: AtomicBool,
    inflight: AtomicUsize,
    bucket: Mutex<(f64, Instant)>,
}

impl Conn {
    pub fn device_id(&self) -> Option<String> {
        self.device_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set_device(&self, device_id: &str) {
        *self.device_id.lock().unwrap_or_else(PoisonError::into_inner) = Some(device_id.to_string());
    }

    pub fn ip(&self) -> &str {
        self.address.rsplit_once(':').map_or(self.address.as_str(), |(ip, _)| ip)
    }

    pub fn send(&self, frame: Frame) {
        let _ = self.sender.send(frame);
    }

    fn take_token(&self) -> bool {
        let mut bucket = self.bucket.lock().unwrap_or_else(PoisonError::into_inner);
        let (tokens, last) = *bucket;
        let refilled = (tokens + last.elapsed().as_secs_f64() * RATE_PER_SEC).min(RATE_BURST);
        if refilled < 1.0 {
            *bucket = (refilled, Instant::now());
            return false;
        }
        *bucket = (refilled - 1.0, Instant::now());
        true
    }
}

struct Listener {
    port: u16,
    stop: Arc<AtomicBool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    /// 已放行网段上的本机地址，配对码用第一个。
    pub addresses: Vec<String>,
    pub networks: Vec<LocalNetwork>,
    pub allow_other: bool,
    pub fingerprint: String,
    pub fingerprint_short: String,
    pub error: Option<String>,
}

static LISTENER: OnceLock<Mutex<Option<Listener>>> = OnceLock::new();
static LAST_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static CONNS: OnceLock<Mutex<HashMap<u64, Arc<Conn>>>> = OnceLock::new();
static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

fn listener_slot() -> std::sync::MutexGuard<'static, Option<Listener>> {
    LISTENER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn last_error() -> std::sync::MutexGuard<'static, Option<String>> {
    LAST_ERROR
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn conns() -> std::sync::MutexGuard<'static, HashMap<u64, Arc<Conn>>> {
    CONNS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// 已绑定设备的连接快照，供事件推送使用。
pub fn paired_connections() -> Vec<(String, Arc<Conn>)> {
    conns()
        .values()
        .filter_map(|conn| conn.device_id().map(|device_id| (device_id, Arc::clone(conn))))
        .collect()
}

pub fn online_device_ids() -> Vec<String> {
    paired_connections().into_iter().map(|(id, _)| id).collect()
}

pub fn push_to_device(device_id: &str, frame: Frame) {
    paired_connections()
        .into_iter()
        .filter(|(id, _)| id == device_id)
        .for_each(|(_, conn)| conn.send(frame.clone()));
}

/// 断开某台设备的全部连接（移除、改组、主机关闭时调用）。
pub fn disconnect_device(device_id: &str) {
    conns()
        .values()
        .filter(|conn| conn.device_id().as_deref() == Some(device_id))
        .for_each(|conn| conn.stop.store(true, Ordering::SeqCst));
}

/// 网段开关变化后，断开来源已不被放行的连接。
pub fn disconnect_disallowed(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else {
        return;
    };
    conns()
        .values()
        .filter(|conn| {
            conn.address
                .parse::<SocketAddr>()
                .is_ok_and(|addr| !network::check(&db, addr.ip()).0)
        })
        .for_each(|conn| conn.stop.store(true, Ordering::SeqCst));
}

pub fn app_data(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|error| error.to_string())
}

pub fn status(app: &AppHandle) -> Result<HostStatus, String> {
    let identity = identity::load_or_create(&app_data(app)?)?;
    let state = app.state::<AppState>();
    let (enabled, configured_port, networks, allow_other) = {
        let db = state.lock_db()?;
        (
            store::host_enabled(&db),
            store::host_port(&db),
            network::local_networks(&db),
            network::allow_other(&db),
        )
    };
    let running_port = listener_slot().as_ref().map(|listener| listener.port);
    Ok(HostStatus {
        enabled,
        running: running_port.is_some(),
        port: running_port.unwrap_or(configured_port),
        addresses: networks
            .iter()
            .filter(|network| network.allowed)
            .map(|network| network.address.clone())
            .collect(),
        networks,
        allow_other,
        fingerprint_short: identity::short_fingerprint(&identity.fingerprint),
        fingerprint: identity.fingerprint,
        error: last_error().clone(),
    })
}

/// 启动时按设置决定是否监听。
pub fn start(app: &AppHandle) {
    let enabled = app
        .state::<AppState>()
        .lock_db()
        .map(|db| store::host_enabled(&db))
        .unwrap_or(false);
    if enabled {
        if let Err(error) = listen(app) {
            crate::diag::write(&format!("remote host failed to start: {error}"));
        }
    }
}

pub fn listen(app: &AppHandle) -> Result<(), String> {
    stop();
    let identity = identity::load_or_create(&app_data(app)?)?;
    let config = tls::server_config(&identity)?;
    let port = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::host_port(&db)
    };
    let socket = TcpListener::bind(("0.0.0.0", port)).map_err(|error| {
        let message = tr!("无法监听端口 {port}：{error}", port = port, error = error);
        *last_error() = Some(message.clone());
        message
    })?;
    socket.set_nonblocking(true).map_err(|error| error.to_string())?;
    *last_error() = None;
    let stop_flag = Arc::new(AtomicBool::new(false));
    *listener_slot() = Some(Listener {
        port,
        stop: Arc::clone(&stop_flag),
    });
    let app = app.clone();
    thread::Builder::new()
        .name("multara-remote-host".into())
        .spawn(move || accept_loop(app, socket, config, stop_flag))
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// 停止监听并断开所有连接。
pub fn stop() {
    if let Some(listener) = listener_slot().take() {
        listener.stop.store(true, Ordering::SeqCst);
    }
    conns()
        .values()
        .for_each(|conn| conn.stop.store(true, Ordering::SeqCst));
}

fn accept_loop(app: AppHandle, socket: TcpListener, config: Arc<ServerConfig>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        match socket.accept() {
            Ok((stream, addr)) => {
                let (allowed, cidr) = app
                    .state::<AppState>()
                    .lock_db()
                    .map(|db| network::check(&db, addr.ip()))
                    .unwrap_or((false, None));
                if !allowed {
                    drop(stream);
                    network::record(addr.ip(), cidr, false, None);
                    let _ = app.emit(network::CONNECTIONS_CHANGED_EVENT, ());
                    continue;
                }
                let app = app.clone();
                let config = Arc::clone(&config);
                let _ = thread::Builder::new()
                    .name("multara-remote-conn".into())
                    .spawn(move || {
                        if let Err(error) = handle_connection(&app, stream, addr, cidr, config) {
                            crate::diag::write(&format!("remote connection {addr} ended: {error}"));
                        }
                    });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(error) => {
                crate::diag::write(&format!("remote accept failed: {error}"));
                thread::sleep(ACCEPT_POLL);
            }
        }
    }
}

fn unpaired_count(ip: &str) -> usize {
    conns()
        .values()
        .filter(|conn| conn.device_id().is_none() && conn.ip() == ip)
        .count()
}

fn handle_connection(
    app: &AppHandle,
    mut tcp: TcpStream,
    addr: SocketAddr,
    cidr: Option<String>,
    config: Arc<ServerConfig>,
) -> Result<(), String> {
    tcp.set_nonblocking(false).map_err(|error| error.to_string())?;
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(HANDSHAKE_TIMEOUT)).map_err(|error| error.to_string())?;
    tcp.set_write_timeout(Some(frame::WRITE_TIMEOUT)).map_err(|error| error.to_string())?;
    let mut tls = ServerConnection::new(config).map_err(|error| error.to_string())?;
    while tls.is_handshaking() {
        tls.complete_io(&mut tcp).map_err(|error| error.to_string())?;
    }
    let fingerprint = tls
        .peer_certificates()
        .and_then(|certs| certs.first())
        .map(|cert| identity::fingerprint(cert.as_ref()))
        .ok_or("missing client certificate")?;
    let address = addr.to_string();
    let device = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::device_by_fingerprint(&db, &fingerprint)?
    };
    if device.is_none() && unpaired_count(&addr.ip().to_string()) >= MAX_UNPAIRED_PER_IP {
        return Err("too many unpaired connections".into());
    }
    network::record(addr.ip(), cidr, true, device.as_ref().map(|device| device.name.clone()));
    let _ = app.emit(network::CONNECTIONS_CHANGED_EVENT, ());
    tcp.set_read_timeout(Some(frame::READ_POLL)).map_err(|error| error.to_string())?;
    let (sender, outbound) = mpsc::channel();
    let conn = Arc::new(Conn {
        id: NEXT_CONN.fetch_add(1, Ordering::SeqCst),
        address: address.clone(),
        fingerprint,
        device_id: Mutex::new(device.as_ref().map(|device| device.id.clone())),
        sender,
        stop: AtomicBool::new(false),
        inflight: AtomicUsize::new(0),
        bucket: Mutex::new((RATE_BURST, Instant::now())),
    });
    conns().insert(conn.id, Arc::clone(&conn));
    if let Some(device) = &device {
        on_device_online(app, &device.id, &address);
    }
    let mut stream = StreamOwned::new(tls, tcp);
    let end = frame::run_loop(&mut stream, &outbound, &conn.stop, false, |frame| {
        dispatch(app, &conn, frame)
    });
    conns().remove(&conn.id);
    if let Some(device_id) = conn.device_id() {
        on_device_offline(app, &device_id, &end);
    }
    stream.conn.send_close_notify();
    let _ = stream.conn.complete_io(&mut stream.sock);
    Ok(())
}

fn dispatch(app: &AppHandle, conn: &Arc<Conn>, frame: Frame) -> Option<Frame> {
    let Frame::Req { id, cmd, args, approval } = frame else {
        return None;
    };
    if !conn.take_token() {
        return Some(Frame::fail(id, RemoteError::new("rate_limited", tr!("请求过于频繁"))));
    }
    if conn.inflight.fetch_add(1, Ordering::SeqCst) >= MAX_INFLIGHT {
        conn.inflight.fetch_sub(1, Ordering::SeqCst);
        return Some(Frame::fail(id, RemoteError::new("rate_limited", tr!("请求过于频繁"))));
    }
    let app = app.clone();
    let worker_conn = Arc::clone(conn);
    let spawned = thread::Builder::new()
        .name("multara-remote-req".into())
        .spawn(move || {
            let reply = match gateway::handle(&app, &worker_conn, &cmd, args, approval) {
                Ok(data) => Frame::ok(id, data),
                Err(error) => Frame::fail(id, error),
            };
            worker_conn.inflight.fetch_sub(1, Ordering::SeqCst);
            worker_conn.send(reply);
        });
    if spawned.is_err() {
        conn.inflight.fetch_sub(1, Ordering::SeqCst);
        return Some(Frame::fail(id, RemoteError::new("internal", tr!("主机繁忙"))));
    }
    None
}

pub fn on_device_online(app: &AppHandle, device_id: &str, address: &str) {
    if let Ok(db) = app.state::<AppState>().lock_db() {
        store::touch_device(&db, device_id, address);
        store::audit_device(
            &db,
            device_id,
            "connect",
            None,
            None,
            "ok",
            serde_json::json!({ "address": address }),
        );
    }
    let _ = app.emit(DEVICES_CHANGED_EVENT, ());
}

fn on_device_offline(app: &AppHandle, device_id: &str, end: &frame::LoopEnd) {
    let reason = match end {
        frame::LoopEnd::Closed => "closed".to_string(),
        frame::LoopEnd::Timeout => "timeout".to_string(),
        frame::LoopEnd::Stopped => "stopped".to_string(),
        frame::LoopEnd::Error(error) => error.clone(),
    };
    if let Ok(db) = app.state::<AppState>().lock_db() {
        store::audit_device(
            &db,
            device_id,
            "disconnect",
            None,
            None,
            "ok",
            serde_json::json!({ "reason": reason }),
        );
    }
    if !online_device_ids().iter().any(|id| id == device_id) {
        super::terminals::schedule_cleanup(app, device_id);
    }
    let _ = app.emit(DEVICES_CHANGED_EVENT, ());
}
