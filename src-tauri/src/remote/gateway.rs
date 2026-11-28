use std::sync::Arc;

use rusqlite::Connection;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::{
    frame::{Frame, RemoteError},
    identity, ipc,
    pairing,
    permission::{DeviceCtx, ALL_PERMS},
    rules::{self, arg, Exec},
    send,
    server::{self, Conn},
    store, terminals,
};
use crate::database::AppState;

const APPROVAL_TTL_MINUTES: i64 = 5;
pub const APPROVALS_CHANGED_EVENT: &str = "remote-approvals-changed";
pub const REQUESTS_CHANGED_EVENT: &str = "remote-requests-changed";

fn internal(error: impl ToString) -> RemoteError {
    RemoteError::new("internal", error.to_string())
}

fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Multara".to_string())
}

pub fn handle(
    app: &AppHandle,
    conn: &Arc<Conn>,
    cmd: &str,
    args: Value,
    approval: Option<String>,
) -> Result<Value, RemoteError> {
    if cmd == "remote.pair" {
        return pair(app, conn, &args);
    }
    let device_id = conn
        .device_id()
        .ok_or_else(|| RemoteError::new("unpaired", tr!("设备尚未配对")))?;
    let state = app.state::<AppState>();
    let ctx = {
        let db = state.lock_db().map_err(internal)?;
        DeviceCtx::load(&db, &device_id).map_err(internal)?
    };
    let Some(ctx) = ctx else {
        server::disconnect_device(&device_id);
        return Err(RemoteError::new("revoked", tr!("设备已被主机移除")));
    };
    if let Some(command) = cmd.strip_prefix("remote.") {
        return remote_command(app, &ctx, command, &args);
    }
    let plan = {
        let db = state.lock_db().map_err(internal)?;
        let authorized = rules::authorize(&db, &ctx, cmd, &args)
            .and_then(|plan| check_approval(&db, &ctx, cmd, &args, approval.as_deref(), plan));
        if let Err(error) = &authorized {
            audit(&db, &ctx, cmd, None, outcome_of(error), json!({ "error": error.message }));
        }
        authorized?
    };
    let result = match plan.exec {
        Exec::Ipc => ipc::dispatch(app, cmd, device_scoped_args(&ctx, cmd, args.clone())),
        Exec::Send => send::send_message(app, &ctx, &args),
        Exec::PlanAction => send::plan_action(app, &ctx, &args),
        Exec::QueueList => Ok(Value::Array(Vec::new())),
        Exec::QueueSave => Ok(Value::Null),
    };
    let db = state.lock_db().map_err(internal)?;
    let resource = plan.resource.as_ref().map(|(kind, id)| (*kind, id.as_str()));
    let value = match result {
        Ok(value) => value,
        Err(error) => {
            if !rules::is_read(plan.perm) {
                audit(&db, &ctx, cmd, resource, "error", json!({ "error": error.message }));
            }
            return Err(error);
        }
    };
    if cmd == "terminal_close" {
        if let Some(id) = arg(&args, "id") {
            terminals::release(id);
        }
    }
    if !rules::is_read(plan.perm) {
        audit(&db, &ctx, cmd, resource, "ok", Value::Null);
    }
    rules::post_process(&db, &ctx, &plan.post, value).map_err(internal)
}

/// 文件监听按 owner 维护列表；远程设备的 owner 加上设备前缀，不会覆盖主机本机界面的监听。
fn device_scoped_args(ctx: &DeviceCtx, cmd: &str, mut args: Value) -> Value {
    if cmd == "explorer_watch_workspaces" {
        let owner = format!("{}:{}", ctx.principal_key(), arg(&args, "owner").unwrap_or_default());
        if let Some(object) = args.as_object_mut() {
            object.insert("owner".into(), Value::String(owner));
        }
    }
    args
}

fn outcome_of(error: &RemoteError) -> &'static str {
    match error.code.as_str() {
        "approval_required" => "pending",
        "internal" | "failed" | "timeout" => "error",
        _ => "denied",
    }
}

fn audit(
    db: &Connection,
    ctx: &DeviceCtx,
    cmd: &str,
    resource: Option<(&str, &str)>,
    outcome: &str,
    detail: Value,
) {
    store::audit_device(db, &ctx.device.id, "invoke", Some(cmd), resource, outcome, detail);
}

/// 分组开启了审批时，高危操作必须带一张已批准、参数一致、未过期未使用的票据。
fn check_approval(
    db: &Connection,
    ctx: &DeviceCtx,
    cmd: &str,
    args: &Value,
    approval: Option<&str>,
    plan: rules::Plan,
) -> Result<rules::Plan, RemoteError> {
    if !ctx.group.require_approval || !plan.perm.high_risk() {
        return Ok(plan);
    }
    let hash = rules::args_hash(cmd, args);
    match approval {
        Some(id) => {
            store::consume_approval(db, id, &ctx.device.id, cmd, &hash)
                .map_err(|error| RemoteError::new("approval_invalid", error))?;
            Ok(plan)
        }
        None => Err(RemoteError {
            perm: Some(plan.perm.as_str().to_string()),
            ..RemoteError::new("approval_required", tr!("这个操作需要主机端审批"))
        }),
    }
}

fn pair(app: &AppHandle, conn: &Arc<Conn>, args: &Value) -> Result<Value, RemoteError> {
    if conn.device_id().is_some() {
        return Err(RemoteError::new("paired", tr!("设备已经配对过")));
    }
    let identity = identity::load_or_create(&server::app_data(app).map_err(internal)?).map_err(internal)?;
    let device_name = arg(args, "deviceName").unwrap_or("Multara").trim().chars().take(64).collect::<String>();
    let audit_pair = |outcome: &str, error: &str| {
        if let Ok(db) = app.state::<AppState>().lock_db() {
            store::insert_audit(
                &db,
                &store::AuditEntry {
                    device_name: device_name.clone(),
                    action: "pair".into(),
                    outcome: outcome.into(),
                    detail: json!({ "address": conn.address, "error": error }),
                    ..store::AuditEntry::default()
                },
            );
        }
    };
    let host_proof = pairing::verify(
        &conn.address,
        &identity.fingerprint,
        &conn.fingerprint,
        arg(args, "proof").unwrap_or_default(),
    )
    .map_err(|error| {
        audit_pair("denied", &error);
        RemoteError::new("pair_failed", error)
    })?;
    let device = pairing::await_confirmation(app, &device_name, &conn.fingerprint, &conn.address).map_err(|error| {
        audit_pair("denied", &error);
        RemoteError::new("pair_rejected", error)
    })?;
    if let Ok(db) = app.state::<AppState>().lock_db() {
        store::audit_device(&db, &device.id, "pair", None, None, "ok", json!({ "address": conn.address }));
    }
    conn.set_device(&device.id);
    server::on_device_online(app, &device.id, &conn.address);
    Ok(json!({
        "hostName": host_name(),
        "hostProof": host_proof,
        "deviceId": device.id,
    }))
}

fn hello(ctx: &DeviceCtx) -> Value {
    json!({
        "hostName": host_name(),
        "device": { "id": ctx.device.id, "name": ctx.device.name },
        "group": {
            "id": ctx.group.id,
            "name": ctx.group.name,
            "role": ctx.role(),
            "allProjects": ctx.group.all_projects,
            "shareInGroup": ctx.group.share_in_group,
            "requireApproval": ctx.group.require_approval,
            "maxRunning": ctx.group.max_running,
            "pages": ctx.group.pages,
        },
        "perms": ctx.perms(),
    })
}

fn remote_command(app: &AppHandle, ctx: &DeviceCtx, command: &str, args: &Value) -> Result<Value, RemoteError> {
    let state = app.state::<AppState>();
    let db = state.lock_db().map_err(internal)?;
    match command {
        "hello" => Ok(hello(ctx)),
        "request_permission" => {
            let permission = arg(args, "permission")
                .filter(|name| ALL_PERMS.iter().any(|perm| perm.as_str() == *name) || *name == "project")
                .ok_or_else(|| RemoteError::new("bad_args", tr!("申请的权限无效")))?;
            let project_id = arg(args, "projectId");
            let note = arg(args, "note").unwrap_or_default().chars().take(500).collect::<String>();
            let request = store::insert_request(&db, &ctx.device.id, permission, project_id, &note).map_err(internal)?;
            audit(&db, ctx, "remote.request_permission", None, "pending", json!({ "permission": permission }));
            let _ = app.emit(REQUESTS_CHANGED_EVENT, ());
            serde_json::to_value(request).map_err(internal)
        }
        "cancel_request" => {
            let id = arg(args, "id").unwrap_or_default();
            let owned = store::get_request(&db, id)
                .map_err(internal)?
                .is_some_and(|request| request.device_id == ctx.device.id);
            if !owned {
                return Err(RemoteError::new("not_found", tr!("申请不存在")));
            }
            store::set_request_status(&db, id, "cancelled").map_err(|error| RemoteError::new("failed", error))?;
            let _ = app.emit(REQUESTS_CHANGED_EVENT, ());
            Ok(Value::Null)
        }
        "list_my_requests" => {
            serde_json::to_value(store::list_requests(&db, Some(&ctx.device.id)).map_err(internal)?).map_err(internal)
        }
        "request_approval" => {
            let cmd = arg(args, "cmd").ok_or_else(|| RemoteError::new("bad_args", tr!("缺少参数：cmd")))?;
            let call_args = args.get("args").cloned().unwrap_or_default();
            let plan = rules::authorize(&db, ctx, cmd, &call_args)?;
            if !plan.perm.high_risk() {
                return Err(RemoteError::new("bad_args", tr!("这个操作不需要审批")));
            }
            let summary = approval_summary(cmd, &call_args);
            let approval = store::insert_approval(
                &db,
                &ctx.device.id,
                cmd,
                &rules::args_hash(cmd, &call_args),
                &summary,
                APPROVAL_TTL_MINUTES,
            )
            .map_err(internal)?;
            audit(&db, ctx, cmd, None, "pending", json!({ "approvalId": approval.id }));
            let _ = app.emit(APPROVALS_CHANGED_EVENT, ());
            serde_json::to_value(approval).map_err(internal)
        }
        _ => Err(RemoteError::new("not_allowed", tr!("这个操作不支持远程调用"))),
    }
}

/// 主机端审批卡片上显示的说明：命令 + 关键参数（不含文件内容这类大字段）。
fn approval_summary(cmd: &str, args: &Value) -> String {
    let keys = ["workspaceId", "projectId", "path", "cwd", "upstream"];
    let detail = keys
        .iter()
        .filter_map(|key| arg(args, key).map(|value| format!("{key}={value}")))
        .chain(
            args.get("paths")
                .and_then(Value::as_array)
                .map(|paths| format!("paths={}", paths.len())),
        )
        .collect::<Vec<_>>()
        .join(" ");
    format!("{cmd} {detail}").trim().to_string()
}

/// 主机端处理审批后通知发起设备。
pub fn notify_approval(device_id: &str, approval: &store::Approval) {
    server::push_to_device(
        device_id,
        Frame::Evt {
            event: "remote-approval".into(),
            payload: serde_json::to_value(approval).unwrap_or_default(),
        },
    );
}

/// 分组或设备变更后通知相关在线设备重新拉取权限与数据。
pub fn notify_permissions_changed(device_ids: &[String]) {
    for device_id in device_ids {
        server::push_to_device(
            device_id,
            Frame::Evt {
                event: "remote-permissions-changed".into(),
                payload: Value::Null,
            },
        );
    }
}

pub fn notify_request(device_id: &str, request: &store::PermissionRequest) {
    server::push_to_device(
        device_id,
        Frame::Evt {
            event: "remote-request-updated".into(),
            payload: serde_json::to_value(request).unwrap_or_default(),
        },
    );
}