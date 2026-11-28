use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use super::{
    frame::RemoteError,
    permission::{self, DeviceCtx, Perm},
    scope::{self, Access, Filter},
    store,
};

/// 网关对一个已白名单命令的处理方案：需要的权限、怎么执行、结果怎么过滤。
pub struct Plan {
    pub perm: Perm,
    pub exec: Exec,
    pub post: Post,
    pub resource: Option<(&'static str, String)>,
}

pub enum Exec {
    Ipc,
    Send,
    PlanAction,
    QueueList,
    QueueSave,
}

pub enum Post {
    None,
    Bootstrap,
    ProjectsAndWorkspaces,
    AgentsAndSquads,
    BySession,
    Nested,
    ActiveDiff,
    Runtimes,
    /// 结果是新建的会话：记下发起设备。
    OwnSession,
    /// 结果是新建的任务：给任务会话记下发起设备。
    OwnTaskSession,
    ProjectCreated,
    ProjectDeleted(String),
    TerminalOpened,
    Terminals,
}

/// 远程能看到的 Runtime 字段：不含命令行、路径、接口地址。
const RUNTIME_FIELDS: &[&str] = &[
    "id",
    "name",
    "provider",
    "version",
    "status",
    "models",
    "kind",
    "description",
    "capabilities",
];

fn internal(error: String) -> RemoteError {
    RemoteError::new("internal", error)
}

pub fn forbidden(ctx: &DeviceCtx, perm: Perm) -> RemoteError {
    RemoteError {
        perm: Some(perm.as_str().to_string()),
        ..RemoteError::new(
            "forbidden",
            tr!(
                "没有「{perm}」权限（当前分组：{group}）",
                perm = perm.as_str(),
                group = ctx.group.name
            ),
        )
    }
}

fn not_found() -> RemoteError {
    RemoteError::new("not_found", tr!("资源不存在或无权访问"))
}

fn not_allowed() -> RemoteError {
    RemoteError::new("not_allowed", tr!("这个操作不支持远程调用"))
}

fn page(ctx: &DeviceCtx, page: &str) -> Result<(), RemoteError> {
    if ctx.group.page_visible(page) {
        Ok(())
    } else {
        Err(not_allowed())
    }
}

fn read_only() -> RemoteError {
    RemoteError::new("read_only", tr!("这个会话对你只读"))
}

fn bad_args(key: &str) -> RemoteError {
    RemoteError::new("bad_args", tr!("缺少参数：{key}", key = key))
}

/// 按点分路径取字符串参数，例如 `input.sessionId`。
pub fn arg<'a>(args: &'a Value, path: &str) -> Option<&'a str> {
    path.split('.')
        .try_fold(args, |value, key| value.get(key))?
        .as_str()
        .filter(|value| !value.is_empty())
}

fn required<'a>(args: &'a Value, path: &str) -> Result<&'a str, RemoteError> {
    arg(args, path).ok_or_else(|| bad_args(path))
}

fn need(ctx: &DeviceCtx, perm: Perm) -> Result<(), RemoteError> {
    if ctx.can(perm) {
        Ok(())
    } else {
        Err(forbidden(ctx, perm))
    }
}

fn check_access(access: Access, write: bool) -> Result<(), RemoteError> {
    match access {
        Access::Hidden => Err(not_found()),
        Access::Read if write => Err(read_only()),
        _ => Ok(()),
    }
}

fn session(db: &Connection, ctx: &DeviceCtx, id: &str, write: bool) -> Result<(), RemoteError> {
    check_access(scope::session_access(db, ctx, id).map_err(internal)?, write)
}

fn session_of(
    lookup: fn(&Connection, &str) -> Result<Option<String>, String>,
    db: &Connection,
    id: &str,
) -> Result<String, RemoteError> {
    lookup(db, id).map_err(internal)?.ok_or_else(not_found)
}

fn workspace(
    db: &Connection,
    ctx: &DeviceCtx,
    id: &str,
    write: bool,
    paths: &[Option<&str>],
) -> Result<(), RemoteError> {
    check_access(scope::workspace_access(db, ctx, id).map_err(internal)?, write)?;
    let info = scope::workspace_info(db, id)
        .map_err(internal)?
        .ok_or_else(not_found)?;
    paths
        .iter()
        .flatten()
        .try_for_each(|path| scope::check_relative_path(&info.work_path, path))
        .map_err(|error| RemoteError::new("forbidden", error))
}

fn project(db: &Connection, ctx: &DeviceCtx, id: &str) -> Result<(), RemoteError> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(|error| internal(error.to_string()))?;
    if exists && scope::project_in_scope(&ctx.group, id) {
        Ok(())
    } else {
        Err(not_found())
    }
}

fn agent(ctx: &DeviceCtx, id: &str) -> Result<(), RemoteError> {
    if scope::agent_in_scope(&ctx.group, id) {
        Ok(())
    } else {
        Err(RemoteError::new("forbidden", tr!("这个智能体不在你的授权范围内")))
    }
}

pub fn owns_session(db: &Connection, ctx: &DeviceCtx, session_id: &str) -> bool {
    db.query_row(
        "SELECT origin_principal FROM chat_sessions WHERE id=?1",
        [session_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .ok()
    .flatten()
    .flatten()
    .is_some_and(|origin| origin == ctx.principal_key())
}

pub fn owns_run(db: &Connection, ctx: &DeviceCtx, run_id: &str) -> bool {
    permission::run_principal(db, run_id)
        .ok()
        .and_then(|principal| principal.key())
        .is_some_and(|key| key == ctx.principal_key())
}

/// 自己发起的用 `own`，否则需要 `TaskManage`。
fn own_or_manage(ctx: &DeviceCtx, owned: bool, own: Perm) -> Result<Perm, RemoteError> {
    let perm = if owned { own } else { Perm::TaskManage };
    need(ctx, perm)?;
    Ok(perm)
}

fn task(db: &Connection, ctx: &DeviceCtx, task_id: &str) -> Result<Perm, RemoteError> {
    let session_id = session_of(scope::session_of_task, db, task_id)?;
    session(db, ctx, &session_id, false)?;
    own_or_manage(ctx, owns_session(db, ctx, &session_id), Perm::TaskCreate)
}

fn merge_session(db: &Connection, merge_id: &str) -> Result<(String, bool), RemoteError> {
    db.query_row(
        "SELECT workspace_id, push FROM merge_sessions WHERE id=?1",
        [merge_id],
        |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
    )
    .optional()
    .map_err(|error| internal(error.to_string()))?
    .ok_or_else(not_found)
}

fn plan(perm: Perm, post: Post) -> Plan {
    Plan {
        perm,
        exec: Exec::Ipc,
        post,
        resource: None,
    }
}

fn on(mut plan: Plan, kind: &'static str, id: &str) -> Plan {
    plan.resource = Some((kind, id.to_string()));
    plan
}

/// 白名单：不在这里的命令一律拒绝。权限、资源范围、路径在这里一次校验完。
pub fn authorize(db: &Connection, ctx: &DeviceCtx, cmd: &str, args: &Value) -> Result<Plan, RemoteError> {
    let ws = |write: bool, paths: &[&str]| -> Result<String, RemoteError> {
        let id = required(args, "workspaceId")?;
        let values: Vec<Option<&str>> = paths.iter().map(|path| arg(args, path)).collect();
        workspace(db, ctx, id, write, &values)?;
        Ok(id.to_string())
    };
    let result = match cmd {
        "bootstrap" => plan(Perm::ProjectRead, Post::Bootstrap),
        "list_projects_and_workspaces" => plan(Perm::ProjectRead, Post::ProjectsAndWorkspaces),
        "list_agents_and_squads" => plan(Perm::TaskRead, Post::AgentsAndSquads),
        "list_runtimes" => plan(Perm::TaskRead, Post::Runtimes),
        "list_active_runs" => plan(Perm::TaskRead, Post::BySession),
        "list_task_inbox_surfaces" => plan(Perm::TaskRead, Post::Nested),
        "load_active_diff_data" => plan(Perm::TaskRead, Post::ActiveDiff),
        "list_message_queue" => Plan {
            exec: Exec::QueueList,
            ..plan(Perm::TaskRead, Post::None)
        },
        "save_message_queue" => Plan {
            exec: Exec::QueueSave,
            ..plan(Perm::TaskRead, Post::None)
        },

        "create_project" => plan(Perm::ProjectWrite, Post::ProjectCreated),
        "update_project" => {
            let id = required(args, "id")?;
            project(db, ctx, id)?;
            on(plan(Perm::ProjectWrite, Post::None), "project", id)
        }
        "set_project_archived" => {
            let id = required(args, "projectId")?;
            project(db, ctx, id)?;
            on(plan(Perm::ProjectWrite, Post::None), "project", id)
        }
        "delete_project" => {
            let id = required(args, "projectId")?;
            project(db, ctx, id)?;
            on(plan(Perm::ProjectDelete, Post::ProjectDeleted(id.to_string())), "project", id)
        }

        "create_chat_session" => {
            if let Some(id) = arg(args, "input.projectId") {
                project(db, ctx, id)?;
            }
            agent(ctx, required(args, "input.targetId")?)?;
            plan(Perm::TaskCreate, Post::OwnSession)
        }
        "create_task" => {
            agent(ctx, required(args, "input.agentId")?)?;
            if let Some(id) = arg(args, "input.projectId") {
                project(db, ctx, id)?;
            }
            args.pointer("/input/projectIds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .try_for_each(|id| project(db, ctx, id))?;
            if let Some(id) = arg(args, "input.sourceSessionId") {
                session(db, ctx, id, true)?;
            }
            if let Some(id) = arg(args, "input.parentTaskId") {
                session(db, ctx, &session_of(scope::session_of_task, db, id)?, false)?;
            }
            plan(Perm::TaskCreate, Post::OwnTaskSession)
        }
        "fork_chat_session" => {
            let id = required(args, "sessionId")?;
            session(db, ctx, id, false)?;
            on(plan(Perm::TaskCreate, Post::OwnSession), "session", id)
        }
        "list_chat_messages" | "load_session_diff_data" | "read_session_scratch_file" => {
            let id = required(args, "sessionId")?;
            session(db, ctx, id, false)?;
            plan(Perm::TaskRead, Post::None)
        }
        "send_message" => {
            let id = required(args, "sessionId")?;
            session(db, ctx, id, true)?;
            Plan {
                exec: Exec::Send,
                ..on(plan(Perm::TaskCreate, Post::None), "session", id)
            }
        }
        "plan_action" => {
            let id = required(args, "input.sessionId")?;
            session(db, ctx, id, true)?;
            let run_session = session_of(scope::session_of_run, db, required(args, "input.planRunId")?)?;
            if run_session != id {
                return Err(not_found());
            }
            Plan {
                exec: Exec::PlanAction,
                ..on(plan(Perm::TaskCreate, Post::None), "session", id)
            }
        }
        "update_chat_session_title" | "set_session_default_mode" | "compact_chat_session" => {
            let id = required(args, "sessionId")?;
            session(db, ctx, id, true)?;
            on(plan(Perm::TaskCreate, Post::None), "session", id)
        }
        "save_pasted_attachment" => plan(Perm::TaskCreate, Post::None),
        "set_message_feedback" => {
            let id = session_of(scope::session_of_message, db, required(args, "messageId")?)?;
            session(db, ctx, &id, true)?;
            plan(Perm::TaskCreate, Post::None)
        }
        "load_attachment_preview" => {
            let id = session_of(scope::session_of_attachment, db, required(args, "attachmentId")?)?;
            session(db, ctx, &id, false)?;
            plan(Perm::TaskRead, Post::None)
        }
        "list_task_runs" => {
            let id = session_of(scope::session_of_task, db, required(args, "taskId")?)?;
            session(db, ctx, &id, false)?;
            plan(Perm::TaskRead, Post::None)
        }
        "list_agent_runs" => {
            agent(ctx, required(args, "agentId")?)?;
            plan(Perm::TaskRead, Post::BySession)
        }
        "stop_run" => {
            let run_id = required(args, "runId")?;
            session(db, ctx, &session_of(scope::session_of_run, db, run_id)?, false)?;
            let perm = own_or_manage(ctx, owns_run(db, ctx, run_id), Perm::TaskCreate)?;
            on(plan(perm, Post::None), "run", run_id)
        }
        "resolve_permission" => {
            let run_id = required(args, "runId")?;
            session(db, ctx, &session_of(scope::session_of_run, db, run_id)?, false)?;
            let perm = own_or_manage(ctx, owns_run(db, ctx, run_id), Perm::TaskCreate)?;
            on(plan(perm, Post::None), "run", run_id)
        }
        "update_task_status" | "update_task_meta" | "acknowledge_task_review" | "delete_task" | "update_task" => {
            let task_id = required(args, "taskId")?;
            let perm = task(db, ctx, task_id)?;
            if let Some(id) = arg(args, "agentId") {
                agent(ctx, id)?;
            }
            on(plan(perm, Post::None), "task", task_id)
        }

        "get_turn_snapshots" | "get_snapshot_content" => {
            let id = session_of(scope::session_of_message, db, required(args, "messageId")?)?;
            session(db, ctx, &id, false)?;
            ws(false, &["filePath"])?;
            plan(Perm::TaskRead, Post::None)
        }
        "update_chunk_state" => {
            let id = session_of(scope::session_of_message, db, required(args, "messageId")?)?;
            session(db, ctx, &id, true)?;
            let workspace_id = ws(true, &["filePath"])?;
            on(plan(Perm::FileWrite, Post::None), "workspace", &workspace_id)
        }
        "save_file_baseline" | "delete_file_baseline" => {
            let workspace_id = ws(true, &["filePath"])?;
            on(plan(Perm::FileWrite, Post::None), "workspace", &workspace_id)
        }

        "list_workspace_tree" | "list_quick_open_files" | "list_workspace_directory" | "read_workspace_file"
        | "search_workspace" => {
            ws(false, &["path"])?;
            plan(Perm::FileRead, Post::None)
        }
        "explorer_watch_workspaces" => {
            args.get("workspaceIds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .try_for_each(|id| workspace(db, ctx, id, false, &[]))?;
            plan(Perm::FileRead, Post::None)
        }
        "write_workspace_file" | "create_workspace_file" | "create_workspace_directory" => {
            let workspace_id = ws(true, &["path"])?;
            on(plan(Perm::FileWrite, Post::None), "workspace", &workspace_id)
        }
        "rename_workspace_entry" => {
            let workspace_id = ws(true, &["from", "to"])?;
            on(plan(Perm::FileWrite, Post::None), "workspace", &workspace_id)
        }
        "delete_workspace_entry" => {
            let workspace_id = ws(true, &["path"])?;
            on(plan(Perm::FileDelete, Post::None), "workspace", &workspace_id)
        }

        "list_project_git_status" => {
            project(db, ctx, required(args, "projectId")?)?;
            plan(Perm::GitRead, Post::None)
        }
        "git_log_workspace" | "git_head_file" | "git_index_file" | "git_blame_file" | "git_blame_line"
        | "git_revision_file" | "git_compare_refs_workspace" | "git_commit_files" | "git_merge_preview" => {
            ws(false, &["path"])?;
            plan(Perm::GitRead, Post::None)
        }
        "git_stage_files" | "git_stage_contents" | "git_stage_repository" | "git_commit_workspace"
        | "git_switch_workspace_branch" | "git_rename_workspace_branch" | "git_merge_workspace_branch"
        | "git_fetch_workspace" | "git_pull_workspace" | "git_abort_workspace_merge" | "git_graph_commit_action"
        | "generate_git_commit_message" => {
            let workspace_id = ws(true, &["path"])?;
            on(plan(Perm::GitWrite, Post::None), "workspace", &workspace_id)
        }
        "git_discard_files" => {
            let workspace_id = ws(true, &[])?;
            on(plan(Perm::FileDelete, Post::None), "workspace", &workspace_id)
        }
        "git_push_workspace" | "git_sync_workspace" | "git_set_branch_upstream" => {
            let workspace_id = ws(true, &[])?;
            on(plan(Perm::GitPush, Post::None), "workspace", &workspace_id)
        }
        "git_commit_multiple" => {
            args.get("inputs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .try_for_each(|input| workspace(db, ctx, required(input, "workspaceId")?, true, &[]))?;
            plan(Perm::GitWrite, Post::None)
        }
        "generate_branch_name" => plan(Perm::GitWrite, Post::None),
        "git_merge_into_branch" => {
            let workspace_id = required(args, "input.workspaceId")?;
            workspace(db, ctx, workspace_id, true, &[])?;
            agent(ctx, required(args, "input.chatTargetId")?)?;
            let pushes = args.pointer("/input/push").and_then(Value::as_bool) == Some(true);
            let perm = if pushes { Perm::GitPush } else { Perm::GitWrite };
            on(plan(perm, Post::OwnSession), "workspace", workspace_id)
        }
        "git_pull_merge_session" | "git_finish_merge_session" | "git_abort_merge_session" => {
            let (workspace_id, pushes) = merge_session(db, required(args, "mergeId")?)?;
            workspace(db, ctx, &workspace_id, true, &[])?;
            let perm = if pushes && cmd == "git_finish_merge_session" {
                Perm::GitPush
            } else {
                Perm::GitWrite
            };
            on(plan(perm, Post::None), "workspace", &workspace_id)
        }

        "terminal_profiles" => plan(Perm::Terminal, Post::None),
        "terminal_list_open" => plan(Perm::Terminal, Post::Terminals),
        "terminal_open" => {
            need(ctx, Perm::Terminal)?;
            scope::check_cwd(db, ctx, required(args, "cwd")?)
                .map_err(|error| RemoteError::new("forbidden", error))?;
            plan(Perm::Terminal, Post::TerminalOpened)
        }
        "terminal_write" | "terminal_resize" | "terminal_close" | "terminal_read_screen" => {
            let id = required(args, "id")?;
            if !super::terminals::owned_by(id, &ctx.device.id) {
                return Err(not_found());
            }
            on(plan(Perm::Terminal, Post::None), "terminal", id)
        }
        // 主机配置类页面：分组勾选后只读开放，对应的写命令不在白名单里。
        "list_workflows" | "get_workflow" | "list_workflow_runs" | "get_workflow_run" | "get_workflow_node_values"
        | "get_workflow_node_log" | "list_workflow_events" | "list_node_templates" | "list_workflow_templates" => {
            page(ctx, store::PAGE_WORKFLOWS)?;
            plan(Perm::ProjectRead, Post::None)
        }
        "list_automation_runs" => {
            page(ctx, store::PAGE_AUTOMATION)?;
            plan(Perm::ProjectRead, Post::None)
        }
        "get_run_statistics" => {
            page(ctx, store::PAGE_STATS)?;
            if !ctx.group.all_projects {
                project(db, ctx, required(args, "projectId")?)?;
            }
            plan(Perm::ProjectRead, Post::None)
        }
        _ => return Err(not_allowed()),
    };
    need(ctx, result.perm)?;
    Ok(result)
}

fn set_session_origin(db: &Connection, session_id: &str, key: &str) -> Result<(), String> {
    db.execute(
        "UPDATE chat_sessions SET origin_principal=?2 WHERE id=?1 AND origin_principal IS NULL",
        [session_id, key],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// 执行后的副作用与结果过滤。列表结果一律在主机端按授权裁剪后再返回。
pub fn post_process(db: &Connection, ctx: &DeviceCtx, post: &Post, value: Value) -> Result<Value, String> {
    let filter = || Filter::new(db, ctx);
    Ok(match post {
        Post::None => value,
        Post::Bootstrap => filter()?.bootstrap(value),
        Post::ProjectsAndWorkspaces => filter()?.projects_and_workspaces(value),
        Post::AgentsAndSquads => filter()?.agents_and_squads(value),
        Post::BySession => filter()?.session_list(value, "sessionId"),
        Post::Nested => filter()?.nested_sessions(value),
        Post::ActiveDiff => filter()?.active_diff(value),
        Post::Runtimes => match value {
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|item| match item {
                        Value::Object(map) => Value::Object(
                            map.into_iter()
                                .filter(|(key, _)| RUNTIME_FIELDS.contains(&key.as_str()))
                                .collect(),
                        ),
                        other => other,
                    })
                    .collect(),
            ),
            other => other,
        },
        Post::OwnSession => {
            if let Some(id) = arg(&value, "id").or_else(|| arg(&value, "sessionId")) {
                set_session_origin(db, id, &ctx.principal_key())?;
            }
            value
        }
        Post::OwnTaskSession => {
            if let Some(id) = arg(&value, "sessionId") {
                set_session_origin(db, id, &ctx.principal_key())?;
            }
            value
        }
        Post::ProjectCreated => {
            if let Some(id) = arg(&value, "id") {
                if !ctx.group.all_projects {
                    store::add_project_to_group(db, &ctx.group.id, id)?;
                }
            }
            value
        }
        Post::ProjectDeleted(id) => {
            store::forget_project(db, id)?;
            value
        }
        Post::TerminalOpened => {
            if let Some(id) = arg(&value, "id") {
                super::terminals::claim(id, &ctx.device.id);
            }
            value
        }
        Post::Terminals => match value {
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .filter(|item| {
                        arg(item, "id").is_some_and(|id| super::terminals::owned_by(id, &ctx.device.id))
                    })
                    .collect(),
            ),
            other => other,
        },
    })
}

/// 审批票据绑定的参数哈希：键排序后再算，避免对象键顺序不同导致不一致。
pub fn args_hash(cmd: &str, args: &Value) -> String {
    use sha2::{Digest, Sha256};
    fn canonical(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut entries: Vec<_> = map.iter().collect();
                entries.sort_by(|left, right| left.0.cmp(right.0));
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key.clone(), canonical(value)))
                        .collect(),
                )
            }
            Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
            other => other.clone(),
        }
    }
    let digest = Sha256::digest(format!("{cmd}\n{}", canonical(args)).as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 只读类权限不写审计，避免日志被浏览操作淹没；拒绝的请求一律记录。
pub fn is_read(perm: Perm) -> bool {
    matches!(
        perm,
        Perm::ProjectRead | Perm::TaskRead | Perm::FileRead | Perm::GitRead
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::permission::Role;
    use serde_json::json;

    fn database(work_path: &str) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO projects (id, name, created_at, opened_at) VALUES
               ('p1', 'in', 'now', 'now'), ('p2', 'out', 'now', 'now');
             INSERT INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('a1', 'Agent', '', '', '', 'cursor', 'auto', 1);
             INSERT INTO chat_sessions (id, project_id, title, target_type, target_id, created_at, updated_at, origin_principal) VALUES
               ('local', 'p1', 't', 'agent', 'a1', 'now', 'now', NULL),
               ('outside', 'p2', 't', 'agent', 'a1', 'now', 'now', NULL),
               ('mine', 'p1', 't', 'agent', 'a1', 'now', 'now', 'device:d1'),
               ('peer', 'p1', 't', 'agent', 'a1', 'now', 'now', 'device:d2');
             INSERT INTO session_projects (session_id, project_id, sort_order) VALUES
               ('local', 'p1', 0), ('outside', 'p2', 0), ('mine', 'p1', 0), ('peer', 'p1', 0);
             INSERT INTO agent_runs (id, session_id, agent_id, runtime_id, assignment, status, created_at, origin_principal) VALUES
               ('run-mine', 'mine', 'a1', 'cursor', '', 'running', 'now', 'device:d1'),
               ('run-local', 'local', 'a1', 'cursor', '', 'running', 'now', NULL);
             INSERT INTO remote_devices (id, name, fingerprint, group_id, paired_at) VALUES
               ('d1', 'me', 'f1', 'g', 'now'), ('d2', 'peer', 'f2', 'g', 'now');",
        )
        .unwrap();
        db.execute(
            "INSERT INTO workspaces (id, project_id, name, kind, source_path, work_path, isolation_mode, created_at)
             VALUES ('w1', 'p1', 'repo', 'folder', ?1, ?1, 'direct', 'now')",
            [work_path],
        )
        .unwrap();
        db
    }

    fn ctx(role: Role, share_in_group: bool) -> DeviceCtx {
        DeviceCtx {
            device: store::Device {
                id: "d1".into(),
                name: "me".into(),
                owner_note: String::new(),
                fingerprint: "f1".into(),
                group_id: "g".into(),
                paired_at: String::new(),
                last_seen_at: None,
                last_address: None,
            },
            group: store::Group {
                id: "g".into(),
                name: "g".into(),
                role,
                all_projects: false,
                all_agents: true,
                share_in_group,
                require_approval: false,
                max_running: 2,
                is_default: false,
                project_ids: vec!["p1".into()],
                agent_ids: Vec::new(),
                device_count: 2,
                pages: vec![store::PAGE_WORKFLOWS.into()],
            },
        }
    }

    fn code(result: Result<Plan, RemoteError>) -> String {
        result.err().map(|error| error.code).unwrap_or_else(|| "ok".into())
    }

    #[test]
    fn rejects_commands_outside_the_whitelist() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(&dir.path().to_string_lossy());
        let owner = ctx(Role::Owner, false);
        for cmd in ["save_settings", "add_workspace", "save_automation", "read_external_file", "open_external_url"] {
            assert_eq!(code(authorize(&db, &owner, cmd, &json!({}))), "not_allowed", "{cmd}");
        }
    }

    #[test]
    fn pages_are_read_only_and_follow_group_switches() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(&dir.path().to_string_lossy());
        let viewer = ctx(Role::Viewer, false);
        let args = json!({ "workflowId": "w1", "automationId": "a1" });
        assert_eq!(code(authorize(&db, &viewer, "list_workflows", &args)), "ok");
        assert_eq!(code(authorize(&db, &viewer, "get_workflow", &args)), "ok");
        assert_eq!(code(authorize(&db, &viewer, "list_automation_runs", &args)), "not_allowed");
        assert_eq!(code(authorize(&db, &viewer, "get_run_statistics", &json!({ "projectId": "p1" }))), "not_allowed");
        let mut stats = ctx(Role::Viewer, false);
        stats.group.pages.push(store::PAGE_STATS.into());
        let statistics = |project: Value| code(authorize(&db, &stats, "get_run_statistics", &json!({ "days": 30, "projectId": project })));
        assert_eq!(statistics(json!("p1")), "ok");
        assert_eq!(statistics(json!("p2")), "not_found");
        assert_eq!(statistics(Value::Null), "bad_args");
        for cmd in ["save_workflow_draft", "publish_workflow", "start_workflow_run", "save_automation", "run_automation"] {
            assert_eq!(code(authorize(&db, &ctx(Role::Owner, false), cmd, &args)), "not_allowed", "{cmd}");
        }
    }

    #[test]
    fn hides_sessions_outside_scope_or_owned_by_other_devices() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(&dir.path().to_string_lossy());
        let runner = ctx(Role::Runner, false);
        let read = |id: &str| code(authorize(&db, &runner, "list_chat_messages", &json!({ "sessionId": id })));
        assert_eq!(read("mine"), "ok");
        assert_eq!(read("local"), "ok");
        assert_eq!(read("outside"), "not_found");
        assert_eq!(read("peer"), "not_found");
        let shared = ctx(Role::Runner, true);
        assert_eq!(
            code(authorize(&db, &shared, "list_chat_messages", &json!({ "sessionId": "peer" }))),
            "ok"
        );
        assert_eq!(
            code(authorize(&db, &shared, "send_message", &json!({ "sessionId": "peer", "content": "x" }))),
            "read_only"
        );
    }

    #[test]
    fn runner_can_only_write_its_own_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(&dir.path().to_string_lossy());
        let runner = ctx(Role::Runner, false);
        let send = |id: &str| code(authorize(&db, &runner, "send_message", &json!({ "sessionId": id, "content": "x" })));
        assert_eq!(send("mine"), "ok");
        assert_eq!(send("local"), "read_only");
        let developer = ctx(Role::Developer, false);
        assert_eq!(
            code(authorize(&db, &developer, "send_message", &json!({ "sessionId": "local", "content": "x" }))),
            "ok"
        );
    }

    #[test]
    fn viewer_cannot_write_and_paths_cannot_escape() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(&dir.path().to_string_lossy());
        let viewer = ctx(Role::Viewer, false);
        assert_eq!(
            code(authorize(&db, &viewer, "send_message", &json!({ "sessionId": "mine", "content": "x" }))),
            "forbidden"
        );
        assert_eq!(
            code(authorize(&db, &viewer, "read_workspace_file", &json!({ "workspaceId": "w1", "path": "a.txt" }))),
            "ok"
        );
        assert_eq!(
            code(authorize(&db, &viewer, "read_workspace_file", &json!({ "workspaceId": "w1", "path": "../x" }))),
            "forbidden"
        );
        let developer = ctx(Role::Developer, false);
        assert_eq!(
            code(authorize(&db, &developer, "terminal_open", &json!({ "cwd": dir.path() }))),
            "forbidden"
        );
    }

    #[test]
    fn stopping_another_principals_run_needs_task_manage() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(&dir.path().to_string_lossy());
        let developer = ctx(Role::Developer, false);
        assert_eq!(code(authorize(&db, &developer, "stop_run", &json!({ "runId": "run-mine" }))), "ok");
        assert_eq!(code(authorize(&db, &developer, "stop_run", &json!({ "runId": "run-local" }))), "forbidden");
        let owner = ctx(Role::Owner, false);
        assert_eq!(code(authorize(&db, &owner, "stop_run", &json!({ "runId": "run-local" }))), "ok");
    }

    #[test]
    fn approval_hash_ignores_key_order() {
        let left = args_hash("git_push_workspace", &json!({ "workspaceId": "w", "skipVerify": false }));
        let right = args_hash("git_push_workspace", &json!({ "skipVerify": false, "workspaceId": "w" }));
        assert_eq!(left, right);
        assert_ne!(left, args_hash("git_push_workspace", &json!({ "workspaceId": "x", "skipVerify": false })));
    }
}
