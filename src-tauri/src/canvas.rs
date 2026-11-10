//! Canvas 实例托管（方案 §5.5、§5.8）：前端首次显示时才起服务（懒托管），每实例一个端口。
//! 不可见且 10 分钟没有请求的实例自动停服务，实例本身不删，下次显示再起。

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    database::AppState,
    ops::canvas::{self as ops, CanvasError},
    ui_host::{self, HostRequest, HostResponse},
};

/// 全局同时托管的实例上限
pub const MAX_HOSTED: usize = 32;
const IDLE_SECS: u64 = 10 * 60;

type Key = (String, String);

struct Hosted {
    site: ui_host::HostedSite,
    last_used: Arc<AtomicU64>,
    /// 当前连着的长连接（SSE）数；有连接就不算空闲
    pub connections: Arc<AtomicU64>,
}

#[derive(Default)]
pub struct CanvasHost {
    sites: Mutex<HashMap<Key, Hosted>>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Canvas 页面的 CSP（方案 §5.5）
pub fn canvas_csp() -> String {
    format!(
        "default-src 'self'; script-src 'self' 'unsafe-inline'; connect-src 'self'; \
         img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; \
         frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'self'; \
         frame-ancestors {}",
        ui_host::frame_ancestors()
    )
}

pub fn app_data(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok()
}

pub fn emit_changed(app: &AppHandle, session_id: &str) {
    let _ = app.emit("canvas-changed", json!({ "sessionId": session_id }));
}

impl CanvasHost {
    pub fn url(&self, session_id: &str, instance_id: &str) -> Option<String> {
        let sites = self.sites.lock().ok()?;
        sites
            .get(&(session_id.to_string(), instance_id.to_string()))
            .filter(|hosted| !hosted.site.is_stopped())
            .map(|hosted| hosted.site.url())
    }

    pub fn stop(&self, session_id: &str, instance_id: &str) {
        if let Ok(mut sites) = self.sites.lock() {
            if let Some(hosted) = sites.remove(&(session_id.to_string(), instance_id.to_string())) {
                hosted.site.stop();
            }
        }
    }

    pub fn stop_session(&self, session_id: &str) {
        if let Ok(mut sites) = self.sites.lock() {
            sites.retain(|(session, _), hosted| {
                if session == session_id {
                    hosted.site.stop();
                    false
                } else {
                    true
                }
            });
        }
    }

    /// 托管的源（用于 `__multara/*` 回调的 Origin 校验等）
    pub fn origin(&self, session_id: &str, instance_id: &str) -> Option<String> {
        let sites = self.sites.lock().ok()?;
        sites
            .get(&(session_id.to_string(), instance_id.to_string()))
            .map(|hosted| hosted.site.origin())
    }

    pub fn connections(&self, session_id: &str, instance_id: &str) -> Option<Arc<AtomicU64>> {
        let sites = self.sites.lock().ok()?;
        sites
            .get(&(session_id.to_string(), instance_id.to_string()))
            .map(|hosted| hosted.connections.clone())
    }
}

/// 起服务（已在跑就复用 URL）。实例必须存在。
pub fn ensure_hosted(app: &AppHandle, session_id: &str, instance_id: &str) -> Result<String, CanvasError> {
    let host = app.state::<CanvasHost>();
    if let Some(url) = host.url(session_id, instance_id) {
        return Ok(url);
    }
    {
        let state = app.state::<AppState>();
        let db = state
            .lock_db()
            .map_err(|error| CanvasError::new("canvas.internal", error))?;
        if !ops::canvases_enabled(&db) {
            return Err(disabled());
        }
        ops::hosting_view(&db, session_id, instance_id)?;
    }
    let mut sites = host
        .sites
        .lock()
        .map_err(|_| CanvasError::new("canvas.internal", "lock"))?;
    sites.retain(|_, hosted| !hosted.site.is_stopped());
    if sites.len() >= MAX_HOSTED {
        return Err(CanvasError::new(
            "canvas.limit",
            tr!("同时显示的 Canvas 太多（上限 {max}），先关掉一些", max = MAX_HOSTED),
        ));
    }
    let last_used = Arc::new(AtomicU64::new(now_secs()));
    let connections = Arc::new(AtomicU64::new(0));
    let handler = request_handler(
        app.clone(),
        session_id.to_string(),
        instance_id.to_string(),
        last_used.clone(),
        connections.clone(),
    );
    let site = ui_host::start(canvas_csp(), None, handler)
        .map_err(|error| CanvasError::new("canvas.internal", error))?;
    let url = site.url();
    sites.insert(
        (session_id.to_string(), instance_id.to_string()),
        Hosted {
            site,
            last_used,
            connections,
        },
    );
    Ok(url)
}

fn disabled() -> CanvasError {
    CanvasError::new("canvas.disabled", tr!("Canvas 未启用（设置 → 实验功能）"))
}

/// 每个请求现读实例定义：智能体改了文件或重开换了输入，刷新即生效
fn request_handler(
    app: AppHandle,
    session_id: String,
    instance_id: String,
    last_used: Arc<AtomicU64>,
    connections: Arc<AtomicU64>,
) -> ui_host::Handler {
    Arc::new(move |request: HostRequest| {
        last_used.store(now_secs(), Ordering::Relaxed);
        let view = {
            let state = app.state::<AppState>();
            let Ok(db) = state.lock_db() else {
                return HostResponse::Status(503);
            };
            if !ops::canvases_enabled(&db) {
                return HostResponse::Status(404);
            }
            match ops::hosting_view(&db, &session_id, &instance_id) {
                Ok(view) => view,
                Err(_) => return HostResponse::Status(404),
            }
        };
        if let Some(endpoint) = request.path.strip_prefix("/__multara/") {
            return crate::canvas_bridge::handle(
                &app,
                &session_id,
                &instance_id,
                &view,
                endpoint,
                &request,
                &connections,
            );
        }
        if request.method != "GET" && request.method != "HEAD" {
            return HostResponse::Status(405);
        }
        let path = if request.path == "/" {
            format!("/{}", view.manifest.entry)
        } else {
            request.path.clone()
        };
        match ui_host::resolve_path(&view.dir, &path, "index.html") {
            Some(file) => HostResponse::File(file),
            None => HostResponse::Status(404),
        }
    })
}

/// 空闲回收：不可见且 10 分钟没有请求、也没有长连接的实例停服务
pub fn start_reaper(app: AppHandle) {
    let _ = thread::Builder::new().name("canvas-reaper".into()).spawn(move || loop {
        thread::sleep(Duration::from_secs(60));
        let host = app.state::<CanvasHost>();
        let idle: Vec<Key> = match host.sites.lock() {
            Ok(sites) => sites
                .iter()
                .filter(|(_, hosted)| {
                    hosted.connections.load(Ordering::Relaxed) == 0
                        && now_secs().saturating_sub(hosted.last_used.load(Ordering::Relaxed)) > IDLE_SECS
                })
                .map(|(key, _)| key.clone())
                .collect(),
            Err(_) => continue,
        };
        if idle.is_empty() {
            continue;
        }
        let hidden: Vec<Key> = {
            let state = app.state::<AppState>();
            let Ok(db) = state.lock_db() else { continue };
            idle.into_iter()
                .filter(|(session, instance)| {
                    db.query_row(
                        "SELECT visible FROM canvas_instances WHERE session_id=?1 AND instance_id=?2",
                        [session, instance],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|visible| visible == 0)
                    .unwrap_or(true)
                })
                .collect()
        };
        for (session, instance) in hidden {
            host.stop(&session, &instance);
        }
    });
}

// ---------- Tauri 命令 ----------

fn err(error: CanvasError) -> String {
    error.message
}

#[tauri::command]
pub fn list_canvas_instances(session_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let _span = crate::diag::span("list_canvas_instances");
    let db = state.lock_db()?;
    if !ops::canvases_enabled(&db) {
        return Ok(json!({ "enabled": false, "instances": [], "available": [] }));
    }
    Ok(json!({
        "enabled": true,
        "instances": ops::list(&db, &session_id, true).map_err(err)?,
        "available": ops::available(&db, &session_id).map_err(err)?,
    }))
}

#[tauri::command]
pub async fn ensure_canvas_hosted(session_id: String, instance_id: String, app: AppHandle) -> Result<String, String> {
    let _span = crate::diag::span("ensure_canvas_hosted");
    tauri::async_runtime::spawn_blocking(move || ensure_hosted(&app, &session_id, &instance_id).map_err(err))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn set_canvas_visible(
    session_id: String,
    instance_id: String,
    visible: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.lock_db()?;
    ops::set_visible(&db, &session_id, &instance_id, visible).map_err(err)?;
    drop(db);
    emit_changed(&app, &session_id);
    Ok(())
}

/// 从聊天（「添加上下文 → Canvas...」/ 卡片按钮）打开已注册的定义；instanceId = canvasId
#[tauri::command]
pub fn open_registered_canvas(
    session_id: String,
    canvas_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let db = state.lock_db()?;
    if !ops::canvases_enabled(&db) {
        return Err(disabled().message);
    }
    let app_data = app_data(&app).ok_or_else(|| tr!("无法读取应用数据目录"))?;
    let opened = ops::open(
        &db,
        &app_data,
        &ops::Caller {
            session_id: session_id.clone(),
            ..Default::default()
        },
        &ops::OpenRequest {
            id: Some(canvas_id),
            ..Default::default()
        },
    )
    .map_err(err)?;
    drop(db);
    emit_changed(&app, &session_id);
    Ok(opened.instance)
}

#[tauri::command]
pub fn close_canvas(
    session_id: String,
    instance_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.lock_db()?;
    ops::close(&db, &session_id, &instance_id).map_err(err)?;
    drop(db);
    app.state::<CanvasHost>().stop(&session_id, &instance_id);
    crate::canvas_bridge::drop_instance(&session_id, &instance_id);
    emit_changed(&app, &session_id);
    Ok(())
}

/// 服务当前地址；没在跑返回 None。不会起服务（前端用来探测断开）
#[tauri::command]
pub fn canvas_hosted_url(session_id: String, instance_id: String, app: AppHandle) -> Option<String> {
    app.state::<CanvasHost>().url(&session_id, &instance_id)
}

/// 固定（pin）到其它会话：固定的实例出现在所有会话的 Canvas 列表里，操作仍归属原会话（五期）
#[tauri::command]
pub fn set_canvas_pinned(
    session_id: String,
    instance_id: String,
    pinned: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.lock_db()?;
    ops::set_pinned(&db, &session_id, &instance_id, pinned).map_err(err)?;
    drop(db);
    // 所有会话的列表都可能变：前端收到后刷新它关心的会话
    let _ = app.emit("canvas-changed", json!({ "sessionId": session_id, "pinned": true }));
    Ok(())
}

/// 新建聊天菜单的 Canvas 子菜单：这个需求的工作区里 `.multara/canvases/` 下可直接打开的定义（五期）
#[tauri::command]
pub fn list_project_canvases(project_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let db = state.lock_db()?;
    if !ops::canvases_enabled(&db) {
        return Ok(Value::Array(Vec::new()));
    }
    Ok(ops::project_definitions(&db, &project_id))
}

/// 前端处理完 `ui-outbound`（填输入框）后的应答
#[tauri::command]
pub fn resolve_ui_outbound(request_id: String, result: Value) {
    crate::canvas_bridge::resolve_outbound(&request_id, result);
}

#[tauri::command]
pub fn get_canvas_context(
    session_id: String,
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let db = state.lock_db()?;
    ops::get_context(&db, &session_id, &instance_id).map_err(err)
}
