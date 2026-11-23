//! 智能体经 `MULTARA_CTL` 操作当前聊天界面：预览文件、打开浏览器、查看已开的标签。

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::{canvas_bridge, ctl::RunCtx};

const UI_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Debug)]
pub struct SurfaceCtlError {
    pub code: &'static str,
    pub message: String,
}

impl SurfaceCtlError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn to_json(&self) -> Value {
        json!({ "ok": false, "code": self.code, "message": self.message })
    }

    pub fn exit_code(&self) -> i32 {
        match self.code {
            "surface.invalid_args" => 1,
            _ => 2,
        }
    }
}

fn text(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn ui_result(value: Value) -> Result<Value, SurfaceCtlError> {
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(json!({
            "ok": true,
            "value": value.get("value").cloned().unwrap_or(Value::Null),
        }));
    }
    let reason = value.get("reason").and_then(Value::as_str).unwrap_or("ui_timeout");
    let message = value.get("message").and_then(Value::as_str).unwrap_or("");
    let (code, fallback) = match reason {
        "unsupported" => ("surface.unsupported", tr!("当前界面不支持这项操作")),
        "not_open" => ("surface.not_open", tr!("界面没有打开这个会话")),
        "session_closed" | "ui_timeout" => ("surface.ui_timeout", tr!("界面没有在时限内打开终端")),
        _ => ("surface.failed", tr!("当前界面不支持这项操作")),
    };
    Err(SurfaceCtlError::new(
        code,
        if message.is_empty() { fallback } else { message.to_string() },
    ))
}

pub fn dispatch(
    app: &AppHandle,
    ctx: &RunCtx,
    method: &str,
    args: &Value,
) -> Result<Value, SurfaceCtlError> {
    let reply = canvas_bridge::request_frontend(
        app,
        "surface-ctl",
        json!({
            "sessionId": ctx.session_id,
            "method": method,
            "path": text(args, "path"),
            "url": text(args, "url"),
            "line": args.get("line").and_then(Value::as_u64),
        }),
        UI_TIMEOUT,
    );
    ui_result(reply)
}
