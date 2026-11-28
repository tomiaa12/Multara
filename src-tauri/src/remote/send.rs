use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

use super::{
    frame::RemoteError,
    permission::{self, DeviceCtx, Perm},
    rules::{self, arg},
    scope,
};
use crate::{
    commands::{self, MessageSource, PlanActionInput},
    database::AppState,
    models::{MessageAttachmentInput, RunMode},
    session,
};

const REMOTE_SOURCE: &str = "remote";
const GENUI_SOURCE: &str = "genui";
const PASTE_DIR: &str = "multara-paste";

fn internal(error: impl ToString) -> RemoteError {
    RemoteError::new("internal", error.to_string())
}

fn parse<T: serde::de::DeserializeOwned + Default>(args: &Value, key: &str) -> Result<T, RemoteError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(T::default()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|error| RemoteError::new("bad_args", error.to_string())),
    }
}

fn source_meta(ctx: &DeviceCtx, base: Option<&Value>) -> Value {
    let mut meta = base
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    meta.insert("deviceId".into(), json!(ctx.device.id));
    meta.insert("deviceName".into(), json!(ctx.device.name));
    Value::Object(meta)
}

/// 远程只能附带先经 `save_pasted_attachment` 上传到主机临时目录的文件，不能引用主机上的任意路径。
fn check_attachments(attachments: &[MessageAttachmentInput]) -> Result<(), RemoteError> {
    let root = std::env::temp_dir().join(PASTE_DIR).canonicalize().ok();
    let outside = attachments.iter().any(|attachment| {
        let real = PathBuf::from(&attachment.path).canonicalize().ok();
        match (&root, real) {
            (Some(root), Some(real)) => !real.starts_with(root),
            _ => true,
        }
    });
    if outside {
        return Err(RemoteError::new("forbidden", tr!("附件不在允许的位置")));
    }
    Ok(())
}

fn mode_perm(ctx: &DeviceCtx, mode: RunMode) -> Result<Perm, RemoteError> {
    let perm = if mode == RunMode::Plan {
        Perm::AgentPlan
    } else {
        Perm::AgentRun
    };
    if !ctx.can(perm) {
        return Err(rules::forbidden(ctx, perm));
    }
    Ok(perm)
}

/// 会话正在运行时，只有发起这次运行的设备能追加消息。
fn check_busy(db: &Connection, ctx: &DeviceCtx, session_id: &str) -> Result<(), RemoteError> {
    let mut statement = db
        .prepare("SELECT id FROM agent_runs WHERE session_id=?1 AND status IN ('queued', 'running')")
        .map_err(internal)?;
    let active = statement
        .query_map([session_id], |row| row.get::<_, String>(0))
        .map_err(internal)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(internal)?;
    let other = active
        .iter()
        .filter_map(|run_id| permission::run_principal(db, run_id).ok())
        .find(|principal| principal.key() != Some(ctx.principal_key()));
    let Some(other) = other else {
        return Ok(());
    };
    let who = match other {
        permission::Principal::Device(owner) => owner.device.name,
        permission::Principal::Channel { sender_id, .. } => sender_id,
        permission::Principal::Local => tr!("主机本机"),
    };
    Err(RemoteError::new(
        "busy",
        tr!("{who} 正在这个会话里运行，请等待结束后再发送", who = who),
    ))
}

fn check_quota(db: &Connection, ctx: &DeviceCtx) -> Result<(), RemoteError> {
    let running: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE origin_principal=?1 AND status IN ('queued', 'running')",
            [ctx.principal_key()],
            |row| row.get(0),
        )
        .map_err(internal)?;
    if running >= ctx.group.max_running {
        return Err(RemoteError::new(
            "quota",
            tr!(
                "同时运行的任务已达上限（{count} 个），请等待结束后再发送",
                count = ctx.group.max_running
            ),
        ));
    }
    Ok(())
}

fn check_session_target(db: &Connection, ctx: &DeviceCtx, session_id: &str) -> Result<(), RemoteError> {
    let target: String = db
        .query_row(
            "SELECT target_id FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| row.get(0),
        )
        .map_err(|_| RemoteError::new("not_found", tr!("聊天不存在")))?;
    if !scope::agent_in_scope(&ctx.group, &target) {
        return Err(RemoteError::new("forbidden", tr!("这个智能体不在你的授权范围内")));
    }
    Ok(())
}

fn check_referenced(ctx: &DeviceCtx, content: &str) -> Result<(), RemoteError> {
    let outside = commands::referenced_requirement_ids(content)
        .iter()
        .any(|id| !scope::project_in_scope(&ctx.group, id));
    if outside {
        return Err(RemoteError::new("forbidden", tr!("消息里引用了你无权访问的需求")));
    }
    Ok(())
}

fn mark_runs(db: &Connection, ctx: &DeviceCtx, run_ids: &[String]) {
    let key = ctx.principal_key();
    for run_id in run_ids {
        let _ = db.execute(
            "UPDATE agent_runs SET origin_principal=?2 WHERE id=?1 AND origin_principal IS NULL",
            params![run_id, key],
        );
    }
}

/// Runner 的 Agent 模式只能在会话专属 worktree 里运行：第一次运行时按会话关联需求的
/// 每个 git 工作区各建一个 worktree，登记为会话专属工作区（与合并冲突会话同一机制）。
fn ensure_session_worktrees(app: &AppHandle, ctx: &DeviceCtx, session_id: &str, content: &str) -> Result<(), RemoteError> {
    let state = app.state::<AppState>();
    let (sources, git_command, isolation_base) = {
        let db = state.lock_db().map_err(internal)?;
        let dedicated: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE session_id=?1)",
                [session_id],
                |row| row.get(0),
            )
            .map_err(internal)?;
        if dedicated {
            return Ok(());
        }
        let mut linked = session::linked_project_ids(&db, session_id).map_err(internal)?;
        let before = linked.len();
        for id in commands::referenced_requirement_ids(content) {
            if !linked.contains(&id) {
                linked.push(id);
            }
        }
        if linked.len() != before {
            session::replace_linked_projects(&db, session_id, &linked).map_err(internal)?;
        }
        let sources = session::session_workspaces(&db, session_id).map_err(internal)?;
        if sources.iter().any(|workspace| workspace.kind != "git") {
            return Err(RemoteError::new(
                "forbidden",
                tr!("需求里有非 Git 目录，你的分组只能用 Plan 模式"),
            ));
        }
        let configured = crate::remote::store::setting(&db, "isolationRoot").unwrap_or_default();
        (
            sources,
            commands::setting_value(&db, "gitExecutable", "git"),
            commands::expand_user_path(&configured),
        )
    };
    if sources.is_empty() {
        return Ok(());
    }
    let app_data = app.path().app_data_dir().map_err(internal)?;
    let mut prepared = Vec::new();
    for source in &sources {
        match crate::workspace::prepare(
            &app_data,
            &source.project_id,
            &source.work_path,
            "isolated",
            isolation_base.clone(),
            &git_command,
            Some("HEAD"),
            None,
            true,
        ) {
            Ok(item) => prepared.push((source, item)),
            Err(error) => {
                prepared
                    .iter()
                    .for_each(|(_, item)| crate::workspace::cleanup(item, &git_command));
                return Err(RemoteError::new("failed", error));
            }
        }
    }
    let db = state.lock_db().map_err(internal)?;
    for (source, item) in &prepared {
        let workspace = &item.workspace;
        db.execute(
            "INSERT INTO workspaces
             (id, project_id, session_id, source_workspace_id, name, kind, source_path,
              work_path, branch, isolation_mode, created_at, origin_principal)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                workspace.id,
                workspace.project_id,
                session_id,
                source.id,
                format!("{} · {}", source.name, ctx.device.name),
                workspace.kind,
                workspace.source_path,
                workspace.work_path,
                workspace.branch,
                workspace.isolation_mode,
                workspace.created_at,
                ctx.principal_key(),
            ],
        )
        .map_err(internal)?;
    }
    Ok(())
}

pub fn send_message(app: &AppHandle, ctx: &DeviceCtx, args: &Value) -> Result<Value, RemoteError> {
    let session_id = arg(args, "sessionId").unwrap_or_default().to_string();
    let content = arg(args, "content").unwrap_or_default().to_string();
    let attachments: Vec<MessageAttachmentInput> = parse(args, "attachments")?;
    let requested_mode: Option<RunMode> = parse(args, "requestedMode")?;
    check_attachments(&attachments)?;
    check_referenced(ctx, &content)?;
    let state = app.state::<AppState>();
    let mode = {
        let db = state.lock_db().map_err(internal)?;
        check_session_target(&db, ctx, &session_id)?;
        check_busy(&db, ctx, &session_id)?;
        check_quota(&db, ctx)?;
        let default_mode = db
            .query_row(
                "SELECT default_mode FROM chat_sessions WHERE id=?1",
                [&session_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|mode| mode.parse().ok())
            .unwrap_or(RunMode::Auto);
        requested_mode.unwrap_or(default_mode)
    };
    mode_perm(ctx, mode)?;
    if mode != RunMode::Plan && !ctx.can(Perm::AgentRunDirect) {
        ensure_session_worktrees(app, ctx, &session_id, &content)?;
    }
    let ui_source = args.get("source");
    let genui = ui_source.and_then(|source| arg(source, "sourceType")) == Some(GENUI_SOURCE);
    let (source_type, meta) = if genui {
        (GENUI_SOURCE, source_meta(ctx, ui_source.and_then(|source| source.get("meta"))))
    } else {
        (REMOTE_SOURCE, source_meta(ctx, None))
    };
    let outcome = commands::send_channel_message(
        app,
        &session_id,
        &content,
        attachments,
        source_type,
        meta,
        requested_mode,
    )
    .map_err(|error| RemoteError::new("failed", error))?;
    {
        let db = state.lock_db().map_err(internal)?;
        mark_runs(&db, ctx, &outcome.run_ids);
    }
    let messages = commands::list_chat_messages_blocking(&state, session_id).map_err(internal)?;
    serde_json::to_value(messages).map_err(internal)
}

pub fn plan_action(app: &AppHandle, ctx: &DeviceCtx, args: &Value) -> Result<Value, RemoteError> {
    let input: PlanActionInput = serde_json::from_value(args.get("input").cloned().unwrap_or_default())
        .map_err(|error| RemoteError::new("bad_args", error.to_string()))?;
    match input.action.as_str() {
        "approve" => {
            mode_perm(ctx, RunMode::Agent)?;
            let state = app.state::<AppState>();
            {
                let db = state.lock_db().map_err(internal)?;
                check_busy(&db, ctx, &input.session_id)?;
                check_quota(&db, ctx)?;
            }
            if !ctx.can(Perm::AgentRunDirect) {
                ensure_session_worktrees(app, ctx, &input.session_id, "")?;
            }
        }
        "refine" => {
            mode_perm(ctx, RunMode::Plan)?;
        }
        _ => {}
    }
    let source = MessageSource {
        source_type: REMOTE_SOURCE.to_string(),
        meta: source_meta(ctx, None),
    };
    let by = json!({ "type": REMOTE_SOURCE, "deviceId": ctx.device.id });
    let result = commands::plan_action_inner(app, input, by, Some(source))
        .map_err(|error| RemoteError::new("failed", error))?;
    serde_json::to_value(result).map_err(internal)
}
