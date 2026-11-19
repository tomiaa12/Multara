//! 智能体经 `MULTARA_CTL` 打开集成终端并写入命令（与 canvas 控制通道同套 token）。
//!
//! 方法：`open` / `run` / `write` / `focus`。实际开 PTY、写输入、展开面板由前端完成并回执。

use std::time::Duration;

use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::{
    canvas_bridge,
    ctl::RunCtx,
    database::AppState,
    session,
};

const UI_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCtlError {
    pub code: &'static str,
    pub message: String,
}

impl TerminalCtlError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.code {
            "terminal.invalid_args" => 1,
            "terminal.app_not_running" | "terminal.no_workspace" | "terminal.ui_timeout" => 2,
            _ => 3,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "ok": false, "code": self.code, "message": self.message })
    }
}

fn text(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn flag(args: &Value, key: &str) -> bool {
    match args.get(key) {
        Some(Value::Bool(true)) => true,
        Some(Value::String(value)) => {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true")
        }
        _ => false,
    }
}

struct Target {
    workspace_id: String,
    project_id: String,
    cwd: String,
    title: String,
}

fn pick_target(
    app: &AppHandle,
    session_id: &str,
    args: &Value,
) -> Result<Target, TerminalCtlError> {
    let state = app.state::<AppState>();
    let db = state
        .lock_db()
        .map_err(|error| TerminalCtlError::new("terminal.internal", error))?;
    let workspaces = session::session_workspaces(&db, session_id)
        .map_err(|error| TerminalCtlError::new("terminal.internal", error))?;
    if workspaces.is_empty() {
        return Err(TerminalCtlError::new(
            "terminal.no_workspace",
            tr!("当前会话没有可打开的工作区终端"),
        ));
    }

    let wanted_id = text(args, "workspace");
    let wanted_cwd = text(args, "cwd").map(|cwd| {
        std::path::PathBuf::from(&cwd)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(&cwd))
    });

    let chosen = if let Some(id) = wanted_id.as_ref() {
        workspaces
            .iter()
            .find(|item| &item.id == id)
            .ok_or_else(|| {
                TerminalCtlError::new(
                    "terminal.invalid_args",
                    tr!("工作区不在当前会话里：{id}", id = id),
                )
            })?
    } else if let Some(cwd) = wanted_cwd.as_ref() {
        workspaces
            .iter()
            .find(|item| {
                let root = std::path::Path::new(if item.work_path.is_empty() {
                    item.source_path.as_str()
                } else {
                    item.work_path.as_str()
                });
                let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
                cwd.starts_with(&root)
            })
            .ok_or_else(|| {
                TerminalCtlError::new(
                    "terminal.invalid_args",
                    tr!("cwd 不在当前会话的工作区内"),
                )
            })?
    } else {
        workspaces
            .first()
            .ok_or_else(|| {
                TerminalCtlError::new(
                    "terminal.no_workspace",
                    tr!("当前会话没有可打开的工作区终端"),
                )
            })?
    };

    let default_cwd = if chosen.work_path.is_empty() {
        chosen.source_path.clone()
    } else {
        chosen.work_path.clone()
    };
    let cwd = text(args, "cwd").unwrap_or(default_cwd);
    if !std::path::Path::new(&cwd).is_dir() {
        return Err(TerminalCtlError::new(
            "terminal.invalid_args",
            tr!("终端目录不存在"),
        ));
    }
    let title = text(args, "title").unwrap_or_else(|| chosen.name.clone());
    Ok(Target {
        workspace_id: chosen.id.clone(),
        project_id: chosen.project_id.clone(),
        cwd,
        title,
    })
}

fn ui_result(value: Value) -> Result<Value, TerminalCtlError> {
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(json!({
            "ok": true,
            "value": value.get("value").cloned().unwrap_or(Value::Null),
        }));
    }
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("ui_timeout");
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("");
    let (code, fallback) = match reason {
        "no_workspace" => (
            "terminal.no_workspace",
            tr!("当前会话没有可打开的工作区终端"),
        ),
        "not_found" => ("terminal.not_found", tr!("没有打开的终端")),
        "session_closed" | "ui_timeout" => (
            "terminal.ui_timeout",
            tr!("界面没有在时限内打开终端"),
        ),
        _ => ("terminal.failed", tr!("打开集成终端失败")),
    };
    Err(TerminalCtlError::new(
        code,
        if message.is_empty() {
            fallback
        } else {
            message.to_string()
        },
    ))
}

fn ensure_not_plan(app: &AppHandle, run_id: &str, method: &str) -> Result<(), TerminalCtlError> {
    if !matches!(method, "run" | "write") {
        return Ok(());
    }
    let state = app.state::<AppState>();
    let db = state
        .lock_db()
        .map_err(|error| TerminalCtlError::new("terminal.internal", error))?;
    let mode: Option<String> = db
        .query_row(
            "SELECT mode FROM agent_runs WHERE id=?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| TerminalCtlError::new("terminal.internal", error.to_string()))?;
    if mode.as_deref() == Some("plan") {
        return Err(TerminalCtlError::new(
            "terminal.invalid_args",
            tr!(
                "规划模式下不能执行写命令：multara terminal {method}",
                method = method
            ),
        ));
    }
    Ok(())
}

/// 在应用进程里执行一个 terminal 控制方法。会话一律取自 token。
pub fn dispatch(
    app: &AppHandle,
    ctx: &RunCtx,
    method: &str,
    args: &Value,
) -> Result<Value, TerminalCtlError> {
    ensure_not_plan(app, &ctx.run_id, method)?;
    match method {
        "open" | "run" => {
            let target = pick_target(app, &ctx.session_id, args)?;
            let command = if method == "run" {
                Some(text(args, "command").ok_or_else(|| {
                    TerminalCtlError::new("terminal.invalid_args", tr!("缺少 --command"))
                })?)
            } else {
                text(args, "command")
            };
            let reply = canvas_bridge::request_frontend(
                app,
                "terminal-ctl",
                json!({
                    "sessionId": ctx.session_id,
                    "method": method,
                    "workspaceId": target.workspace_id,
                    "projectId": target.project_id,
                    "cwd": target.cwd,
                    "title": target.title,
                    "command": command,
                    "newTab": flag(args, "new"),
                }),
                UI_TIMEOUT,
            );
            ui_result(reply)
        }
        "write" => {
            let id = text(args, "id").ok_or_else(|| {
                TerminalCtlError::new("terminal.invalid_args", tr!("缺少终端 id"))
            })?;
            let data = text(args, "text")
                .or_else(|| text(args, "data"))
                .ok_or_else(|| {
                    TerminalCtlError::new("terminal.invalid_args", tr!("缺少 --text"))
                })?;
            let reply = canvas_bridge::request_frontend(
                app,
                "terminal-ctl",
                json!({
                    "sessionId": ctx.session_id,
                    "method": "write",
                    "id": id,
                    "text": data,
                }),
                UI_TIMEOUT,
            );
            ui_result(reply)
        }
        "focus" => {
            let id = text(args, "id");
            let target = if id.is_none() {
                Some(pick_target(app, &ctx.session_id, args)?)
            } else {
                None
            };
            let reply = canvas_bridge::request_frontend(
                app,
                "terminal-ctl",
                json!({
                    "sessionId": ctx.session_id,
                    "method": "focus",
                    "id": id,
                    "workspaceId": target.as_ref().map(|item| item.workspace_id.clone()),
                    "projectId": target.as_ref().map(|item| item.project_id.clone()),
                    "cwd": target.as_ref().map(|item| item.cwd.clone()),
                    "title": target.as_ref().map(|item| item.title.clone()),
                }),
                UI_TIMEOUT,
            );
            ui_result(reply)
        }
        _ => Err(TerminalCtlError::new(
            "terminal.unknown_method",
            tr!("不支持的方法：{method}", method = method),
        )),
    }
}
