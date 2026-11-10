//! CLI 到运行中应用的控制通道（方案 §5.4）。
//!
//! - 应用启动时在 `127.0.0.1:0` 起端点；每个 run 一个 128 位以上随机 token，登记
//!   `token → { run_id, session_id, agent_id }`，注入 `MULTARA_CTL=http://127.0.0.1:<port>/<token>`。
//!   run 结束即注销（R9）。端口和 token 不写盘、不进日志。
//! - 只接受 `POST /<token>/canvas.<method>`；会话从 token 推出，忽略请求体里的会话 id。

use std::{
    collections::HashMap,
    io::Read,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::{
    canvas::{self, CanvasHost},
    canvas_bridge,
    database::AppState,
    ops::canvas::{self as ops, CanvasError, Caller, OpenRequest},
};

const MAX_BODY: usize = 1024 * 1024;
const MAX_ACTION_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone)]
pub struct RunCtx {
    pub run_id: String,
    pub session_id: String,
    pub agent_id: String,
}

struct Ctl {
    port: u16,
    runs: Mutex<HashMap<String, RunCtx>>,
}

static CTL: OnceLock<Ctl> = OnceLock::new();

pub fn start(app: AppHandle) {
    let Ok(server) = Server::http("127.0.0.1:0") else {
        return;
    };
    let Some(port) = server.server_addr().to_ip().map(|addr| addr.port()) else {
        return;
    };
    if CTL
        .set(Ctl {
            port,
            runs: Mutex::new(HashMap::new()),
        })
        .is_err()
    {
        return;
    }
    let _ = thread::Builder::new().name("multara-ctl".into()).spawn(move || {
        for request in server.incoming_requests() {
            let app = app.clone();
            // action 要等页面回结果，一个请求一个线程
            let _ = thread::Builder::new()
                .name("multara-ctl-req".into())
                .spawn(move || handle_request(&app, request, port));
        }
    });
}

/// 为一次运行登记 token，返回注入给 CLI 的 URL；控制通道没起来时返回 None
pub fn register_run(run_id: &str, session_id: &str, agent_id: &str) -> Option<String> {
    let ctl = CTL.get()?;
    let token = crate::ui_host::random_token();
    let mut runs = ctl.runs.lock().ok()?;
    runs.retain(|_, ctx| ctx.run_id != run_id);
    runs.insert(
        token.clone(),
        RunCtx {
            run_id: run_id.to_string(),
            session_id: session_id.to_string(),
            agent_id: agent_id.to_string(),
        },
    );
    Some(format!("http://127.0.0.1:{}/{token}", ctl.port))
}

pub fn unregister_run(run_id: &str) {
    if let Some(ctl) = CTL.get() {
        if let Ok(mut runs) = ctl.runs.lock() {
            runs.retain(|_, ctx| ctx.run_id != run_id);
        }
    }
}

fn lookup(token: &str) -> Option<RunCtx> {
    CTL.get()?.runs.lock().ok()?.get(token).cloned()
}

fn respond(request: tiny_http::Request, code: u16, body: Value) {
    // 不走分块编码，CLI 侧的极简客户端按 Content-Length 读
    let response = Response::from_string(body.to_string())
        .with_chunked_threshold(usize::MAX)
        .with_status_code(StatusCode(code))
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..])
                .expect("header"),
        );
    let _ = request.respond(response);
}

fn handle_request(app: &AppHandle, mut request: tiny_http::Request, port: u16) {
    let host_ok = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Host"))
        .is_some_and(|header| header.value.as_str() == format!("127.0.0.1:{port}"));
    if !host_ok {
        return respond(request, 421, json!({ "ok": false, "code": "canvas.forbidden" }));
    }
    if request.method() != &Method::Post {
        return respond(request, 405, json!({ "ok": false, "code": "canvas.forbidden" }));
    }
    let url = request.url().to_string();
    let mut parts = url.trim_start_matches('/').splitn(2, '/');
    let token = parts.next().unwrap_or("").to_string();
    let method = parts.next().unwrap_or("").to_string();
    let Some(ctx) = lookup(&token) else {
        return respond(request, 403, json!({ "ok": false, "code": "canvas.forbidden", "message": tr!("控制通道 token 无效或运行已结束") }));
    };
    let Some(method) = method.strip_prefix("canvas.").map(str::to_string) else {
        return respond(request, 404, json!({ "ok": false, "code": "canvas.unknown_method" }));
    };
    let mut body = Vec::new();
    if request
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut body)
        .is_err()
        || body.len() > MAX_BODY
    {
        return respond(request, 413, json!({ "ok": false, "code": "canvas.too_large" }));
    }
    let args: Value = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(args) => args,
            Err(error) => {
                return respond(request, 400, CanvasError::new("canvas.invalid_args", error.to_string()).to_json())
            }
        }
    };
    let result = dispatch(app, &ctx, &method, &args);
    let body = match result {
        Ok(value) => value,
        Err(error) => error.to_json(),
    };
    respond(request, 200, body);
}

fn text(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn required(args: &Value, key: &str) -> Result<String, CanvasError> {
    text(args, key).ok_or_else(|| CanvasError::new("canvas.invalid_args", tr!("缺少 {key}", key = key)))
}

fn app_data(app: &AppHandle) -> Result<PathBuf, CanvasError> {
    canvas::app_data(app).ok_or_else(|| CanvasError::new("canvas.internal", tr!("无法读取应用数据目录")))
}

/// 在应用进程里执行一个 canvas 方法。会话一律取自 token。
pub fn dispatch(app: &AppHandle, ctx: &RunCtx, method: &str, args: &Value) -> Result<Value, CanvasError> {
    let state = app.state::<AppState>();
    let caller = Caller {
        session_id: ctx.session_id.clone(),
        run_id: Some(ctx.run_id.clone()),
        agent_id: Some(ctx.agent_id.clone()),
    };
    {
        let db = state
            .lock_db()
            .map_err(|error| CanvasError::new("canvas.internal", error))?;
        if !ops::canvases_enabled(&db) {
            return Err(CanvasError::new("canvas.disabled", tr!("Canvas 未启用（设置 → 实验功能）")));
        }
    }
    let session = ctx.session_id.as_str();
    match method {
        "list" => {
            let db = state.lock_db().map_err(|error| CanvasError::new("canvas.internal", error))?;
            let mut list = ops::list(&db, session, false)?;
            for item in list.as_array_mut().into_iter().flatten() {
                let instance = item["instanceId"].as_str().unwrap_or("").to_string();
                item["connected"] = json!(canvas_bridge::is_connected(session, &instance));
            }
            Ok(json!({ "ok": true, "value": list }))
        }
        "open" => {
            let request = OpenRequest {
                dir: text(args, "dir"),
                id: text(args, "id"),
                instance: text(args, "instance"),
                title: text(args, "title"),
                input: args.get("input").cloned(),
            };
            let app_data = app_data(app)?;
            let opened = {
                let db = state.lock_db().map_err(|error| CanvasError::new("canvas.internal", error))?;
                ops::open(&db, &app_data, &caller, &request)?
            };
            if opened.reopened {
                let instance = opened.instance["instanceId"].as_str().unwrap_or("");
                canvas_bridge::push_input(session, instance, &opened.instance["input"]);
            }
            canvas::emit_changed(app, session);
            Ok(json!({ "ok": true, "value": opened.instance, "warnings": opened.warnings }))
        }
        "close" => {
            let instance = required(args, "instance")?;
            {
                let db = state.lock_db().map_err(|error| CanvasError::new("canvas.internal", error))?;
                ops::close(&db, session, &instance)?;
            }
            app.state::<CanvasHost>().stop(session, &instance);
            canvas_bridge::drop_instance(session, &instance);
            canvas::emit_changed(app, session);
            Ok(json!({ "ok": true, "value": null }))
        }
        "status" => {
            let instance = required(args, "instance")?;
            let status = text(args, "text").unwrap_or_default();
            let value = {
                let db = state.lock_db().map_err(|error| CanvasError::new("canvas.internal", error))?;
                ops::set_status(&db, session, &instance, &status)?
            };
            canvas::emit_changed(app, session);
            Ok(json!({ "ok": true, "value": value }))
        }
        "context" => {
            let instance = required(args, "instance")?;
            let db = state.lock_db().map_err(|error| CanvasError::new("canvas.internal", error))?;
            Ok(json!({ "ok": true, "value": ops::get_context(&db, session, &instance)? }))
        }
        "action" => {
            let instance = required(args, "instance")?;
            let name = required(args, "name")?;
            let input = args.get("input").cloned().unwrap_or(Value::Null);
            let timeout = args
                .get("timeout")
                .and_then(Value::as_u64)
                .unwrap_or(ops::DEFAULT_ACTION_TIMEOUT_SECS)
                .clamp(1, MAX_ACTION_TIMEOUT_SECS);
            {
                // 宿主侧先按 canvas.json 校验，再投递；等结果时不持有数据库锁
                let db = state.lock_db().map_err(|error| CanvasError::new("canvas.internal", error))?;
                let view = ops::hosting_view(&db, session, &instance)?;
                ops::check_action(&view.manifest, &name, &input)?;
            }
            let value = canvas_bridge::deliver_action(session, &instance, &name, &input, Duration::from_secs(timeout))?;
            Ok(json!({ "ok": true, "value": value }))
        }
        _ => Err(CanvasError::new("canvas.unknown_method", tr!("不支持的方法：{method}", method = method))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_per_run_and_expire() {
        let _ = CTL.set(Ctl {
            port: 1,
            runs: Mutex::new(HashMap::new()),
        });
        let url = register_run("run-a", "s1", "miku").unwrap();
        let token = url.rsplit('/').next().unwrap().to_string();
        assert!(token.len() >= 32);
        assert_eq!(lookup(&token).unwrap().session_id, "s1");
        // 同一个 run 重新登记：旧 token 作废
        let again = register_run("run-a", "s1", "miku").unwrap();
        assert_ne!(again, url);
        assert!(lookup(&token).is_none());
        let other = register_run("run-b", "s2", "miku").unwrap();
        let other_token = other.rsplit('/').next().unwrap().to_string();
        assert_eq!(lookup(&other_token).unwrap().session_id, "s2");
        unregister_run("run-a");
        assert!(lookup(again.rsplit('/').next().unwrap()).is_none());
        assert!(lookup(&other_token).is_some());
        unregister_run("run-b");
        assert!(lookup(&other_token).is_none());
    }
}
