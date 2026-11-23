//! 应用侧：实现 `engine::Host`，在 DB 锁外执行引擎返回的动作（派发 agent、跑脚本/HTTP/Git、发事件、通知）。

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::engine::{self, Action, ForkRequest, Host, Job, JobResult, PreparedSlot, SlotRequest};
use super::model::Isolation;
use super::store;
use crate::database::AppState;

const SECRET_PREFIX: &str = "workflow:";

// ---------- Host ----------

pub struct AppHost {
    app: AppHandle,
    git: String,
    secrets: BTreeMap<String, String>,
}

impl AppHost {
    pub fn new(app: &AppHandle, db: &Connection) -> Self {
        let git = db
            .query_row("SELECT value FROM app_settings WHERE key='gitExecutable'", [], |row| row.get::<_, String>(0))
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "git".into());
        let mut secrets = BTreeMap::new();
        for item in store::secret_names(db).unwrap_or_default() {
            if let Some(value) = crate::channel::secret::load(app, &format!("{SECRET_PREFIX}{}", item.name)) {
                secrets.insert(item.name, value);
            }
        }
        AppHost { app: app.clone(), git, secrets }
    }

    fn app_data(&self) -> Option<PathBuf> {
        self.app.path().app_data_dir().ok()
    }
}

fn git_output(git: &str, path: &str, args: &[&str]) -> Result<String, String> {
    let output = crate::process::command(git)
        .arg("-C")
        .arg(path)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

pub fn git_head(git: &str, path: &str) -> Option<String> {
    git_output(git, path, &["rev-parse", "HEAD"]).ok().filter(|head| !head.is_empty())
}

pub fn git_commit_all(git: &str, path: &str, message: &str, skip_verify: bool) -> Result<Option<String>, String> {
    if git_output(git, path, &["rev-parse", "--is-inside-work-tree"]).is_err() {
        return Ok(None);
    }
    git_output(git, path, &["add", "-A"])?;
    let status = git_output(git, path, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Ok(None);
    }
    let mut args = vec!["-c", "user.name=Multara Workflow", "-c", "user.email=workflow@multara.local", "commit", "-q", "-m", message];
    // 用户自己配了身份就用用户的
    if git_output(git, path, &["config", "user.email"]).is_ok() {
        args.drain(0..4);
    }
    if skip_verify {
        args.push("--no-verify");
    }
    git_output(git, path, &args)?;
    Ok(git_head(git, path))
}

impl Host for AppHost {
    fn now(&self) -> String {
        Utc::now().to_rfc3339()
    }

    fn artifacts_root(&self) -> Option<PathBuf> {
        self.app_data().map(|dir| dir.join("workflow-runs"))
    }

    fn secrets(&self) -> BTreeMap<String, String> {
        self.secrets.clone()
    }

    fn git_head(&self, path: &str) -> Option<String> {
        git_head(&self.git, path)
    }

    fn git_commit_all(&self, path: &str, message: &str) -> Result<Option<String>, String> {
        git_commit_all(&self.git, path, message, true)
    }

    fn git_reset_hard(&self, path: &str, commit: &str) -> Result<(), String> {
        git_output(&self.git, path, &["reset", "--hard", commit])?;
        git_output(&self.git, path, &["clean", "-fd"])?;
        Ok(())
    }

    fn prepare_slot(&self, db: &Connection, request: &SlotRequest<'_>) -> Result<PreparedSlot, String> {
        let wanted = request.slot.workspace.as_deref().map(str::trim).filter(|name| !name.is_empty());
        let (source_id, work_path): (String, String) = db
            .query_row(
                "SELECT id, work_path FROM workspaces
                 WHERE project_id=?1 AND session_id IS NULL AND (?2 IS NULL OR name=?2)
                 ORDER BY created_at LIMIT 1",
                params![request.project_id, wanted],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| tr!("需求里没有可用的工作区，请先给需求添加 Git 项目或文件夹"))?;
        if request.slot.isolation == Isolation::Direct {
            return Ok(PreparedSlot { workspace_id: source_id, base_commit: self.git_head(&work_path) });
        }
        let app_data = self.app_data().ok_or_else(|| tr!("无法读取应用数据目录"))?;
        let create_branch = request.slot.isolation == Isolation::WorktreeNew;
        let base_ref = request.slot.base_ref.as_deref().filter(|value| !value.trim().is_empty());
        let prepared = crate::workspace::prepare(
            &app_data,
            request.project_id,
            &work_path,
            "worktree",
            None,
            &self.git,
            base_ref,
            request.branch.as_deref(),
            create_branch,
        )?;
        let item = &prepared.workspace;
        db.execute(
            "INSERT INTO workspaces
             (id, project_id, session_id, source_workspace_id, name, kind, source_path, work_path, branch, isolation_mode, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                item.id, item.project_id, request.overview_session_id, source_id, item.name, item.kind,
                item.source_path, item.work_path, item.branch, item.isolation_mode, item.created_at
            ],
        )
        .map_err(|error| {
            crate::workspace::cleanup(&prepared, &self.git);
            tr!("保存工作区失败：{message}", message = error)
        })?;
        if create_branch {
            crate::merge_flow::record_origin(db, item, base_ref, &self.git);
        }
        Ok(PreparedSlot { workspace_id: item.id.clone(), base_commit: self.git_head(&item.work_path) })
    }

    fn create_fork(&self, db: &Connection, request: &ForkRequest<'_>) -> Result<PreparedSlot, String> {
        let (project_id, work_path): (String, String) = db
            .query_row(
                "SELECT project_id, work_path FROM workspaces WHERE id=?1",
                [request.source_workspace_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| error.to_string())?;
        let head = self.git_head(&work_path);
        let app_data = self.app_data().ok_or_else(|| tr!("无法读取应用数据目录"))?;
        let prepared = crate::workspace::prepare(
            &app_data, &project_id, &work_path, "worktree", None, &self.git, head.as_deref(), Some(&request.branch), true,
        )?;
        let item = &prepared.workspace;
        db.execute(
            "INSERT INTO workspaces
             (id, project_id, session_id, source_workspace_id, name, kind, source_path, work_path, branch, isolation_mode, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                item.id, item.project_id, request.overview_session_id, request.source_workspace_id, item.name, item.kind,
                item.source_path, item.work_path, item.branch, item.isolation_mode, item.created_at
            ],
        )
        .map_err(|error| {
            crate::workspace::cleanup(&prepared, &self.git);
            error.to_string()
        })?;
        let _ = request.run_id;
        Ok(PreparedSlot { workspace_id: item.id.clone(), base_commit: head })
    }
}

// ---------- 执行入口 ----------

/// 拿锁、建 Host、调引擎、放锁后执行动作
pub fn with_engine<T>(app: &AppHandle, f: impl FnOnce(&Connection, &AppHost) -> Result<(T, Vec<Action>), String>) -> Result<T, String> {
    let (value, actions) = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        let host = AppHost::new(app, &db);
        f(&db, &host)?
    };
    apply(app, actions);
    Ok(value)
}

pub fn run_engine(app: &AppHandle, f: impl FnOnce(&Connection, &AppHost) -> Result<Vec<Action>, String>) -> Result<(), String> {
    with_engine(app, |db, host| f(db, host).map(|acts| ((), acts)))
}

pub fn apply(app: &AppHandle, actions: Vec<Action>) {
    let mut dispatch = false;
    for action in actions {
        match action {
            Action::DispatchAgents => dispatch = true,
            Action::Job(job) => spawn_job(app, job),
            Action::StopSession(session_id) => stop_session(app, &session_id),
            Action::KillJob(node_run_id) => kill_job(&node_run_id),
            Action::RunChanged { run_id, workflow_id, status } => {
                let _ = app.emit("workflow-run-changed", json!({ "runId": run_id, "workflowId": workflow_id, "status": status }));
            }
            Action::NodeChanged { run_id, node_run_id, node_id, status } => {
                let _ = app.emit("workflow-node-changed", json!({ "runId": run_id, "nodeRunId": node_run_id, "nodeId": node_id, "status": status }));
            }
            Action::SessionChanged(session_id) => {
                let _ = app.emit("session-messages-changed", &session_id);
                let _ = app.emit("inbox-changed", &session_id);
            }
            Action::FeishuNotify { session_id, text } => {
                let app = app.clone();
                thread::spawn(move || notify_feishu(&app, &session_id, &text));
            }
            Action::RunFinished { run_id, status, trigger_source, trigger_ref, error } => {
                on_run_finished(app, &run_id, &status, &trigger_source, trigger_ref.as_deref(), error.as_deref());
            }
        }
    }
    if dispatch {
        crate::executor::dispatch(app.clone());
    }
}

/// `executor::finish` 之后调用：节点会话的 run 结束了
pub fn on_agent_run_finished(app: &AppHandle, session_id: &str) {
    if !session_id.starts_with(engine::WF_NODE_PREFIX) {
        return;
    }
    let session_id = session_id.to_string();
    let app = app.clone();
    thread::spawn(move || {
        if let Err(error) = run_engine(&app, |db, host| engine::on_session_idle(db, host, &session_id)) {
            crate::diag::write(&format!("workflow on_session_idle failed: {error}"));
        }
    });
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        if let Err(error) = run_engine(&app, |db, host| engine::recover(db, host)) {
            crate::diag::write(&format!("workflow recover failed: {error}"));
        }
    });
}

pub fn tick(app: &AppHandle) {
    if let Err(error) = run_engine(app, |db, host| engine::tick(db, host)) {
        crate::diag::write(&format!("workflow tick failed: {error}"));
    }
}

fn stop_session(app: &AppHandle, session_id: &str) {
    let runs: Vec<String> = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else { return };
        let Ok(mut statement) = db.prepare("SELECT id FROM agent_runs WHERE session_id=?1 AND status IN ('queued','running')") else { return };
        statement
            .query_map([session_id], |row| row.get(0))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    };
    for run_id in runs {
        let _ = crate::executor::stop(app, &run_id);
    }
}

fn on_run_finished(app: &AppHandle, run_id: &str, status: &str, source: &str, trigger_ref: Option<&str>, error: Option<&str>) {
    if source != "automation" {
        return;
    }
    let Some(automation_run_id) = trigger_ref else { return };
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else { return };
    let (automation_status, reason) = match status {
        "succeeded" => ("completed", None),
        "skipped" => ("skipped", Some("already_running")),
        _ => ("failed", Some("workflow_failed")),
    };
    let _ = db.execute(
        "UPDATE automation_runs SET status=?1, completed_at=?2, reason_code=?3, failure_reason=?4 WHERE id=?5",
        params![automation_status, Utc::now().to_rfc3339(), reason, error, automation_run_id],
    );
    let _ = db.execute(
        "UPDATE webhook_deliveries SET status=?1, reason_code=?2 WHERE automation_run_id=?3",
        params![if status == "succeeded" { "dispatched" } else { "failed" }, reason, automation_run_id],
    );
    let _ = run_id;
}

fn notify_feishu(app: &AppHandle, session_id: &str, text: &str) {
    let bindings: Vec<(String, String)> = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else { return };
        // 从飞书 /flow 启动的运行：发回发起的那个聊天
        let origin = store::run_by_overview_session(&db, session_id).ok().flatten().and_then(|run| {
            let channel = run.extra.get("channel")?;
            Some((channel.get("installationId")?.as_str()?.to_string(), channel.get("chatId")?.as_str()?.to_string()))
        });
        if let Some(origin) = origin {
            drop(db);
            if let Ok((_, api)) = crate::channel::feishu::api_for(app, &origin.0) {
                let _ = api.send_text(&origin.1, text);
            }
            return;
        }
        let Ok(mut statement) = db.prepare(
            "SELECT installation_id, chat_id FROM channel_session_bindings WHERE session_id=?1 AND retired_at IS NULL",
        ) else {
            return;
        };
        statement
            .query_map([session_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    };
    for (installation, chat_id) in bindings {
        if let Ok((_, api)) = crate::channel::feishu::api_for(app, &installation) {
            if let Err(error) = api.send_text(&chat_id, text) {
                crate::diag::write(&format!("workflow feishu notify failed: {error}"));
            }
        }
    }
}

// ---------- 后台任务 ----------

fn jobs() -> &'static Mutex<HashMap<String, Arc<Mutex<Child>>>> {
    static JOBS: OnceLock<Mutex<HashMap<String, Arc<Mutex<Child>>>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn kill_job(node_run_id: &str) {
    let child = jobs().lock().ok().and_then(|mut map| map.remove(node_run_id));
    if let Some(child) = child {
        if let Ok(mut child) = child.lock() {
            crate::executor::kill_process(&mut child);
        }
    }
}

fn spawn_job(app: &AppHandle, job: Job) {
    let app = app.clone();
    thread::spawn(move || {
        let node_run_id = job.node_run_id.clone();
        let result = JobResult { outputs: Map::new(), error: Some(("unsupported".into(), tr!("这个节点类型已经删除"))) };
        if let Err(error) = run_engine(&app, |db, host| engine::complete_job(db, host, &node_run_id, result)) {
            crate::diag::write(&format!("workflow complete_job failed: {error}"));
        }
    });
}

// ---------- 密钥 ----------

pub fn set_secret(app: &AppHandle, name: &str, value: &str) -> Result<(), String> {
    crate::channel::secret::store(app, &format!("{SECRET_PREFIX}{name}"), value)
}

pub fn delete_secret(app: &AppHandle, name: &str) {
    crate::channel::secret::delete(app, &format!("{SECRET_PREFIX}{name}"));
}

// ---------- 渠道指令 ----------

/// 飞书里的 `/flow 工作流名 内容`、`/approve`、`/changes 原因`、`/reject`。返回要回复的文字；不是工作流指令时返回 None。
pub fn channel_command(app: &AppHandle, installation_id: &str, chat_id: &str, command: &str, argument: &str) -> Option<String> {
    match command {
        "/flow" | "/workflow" => Some(channel_start(app, installation_id, chat_id, argument).unwrap_or_else(|error| error)),
        "/approve" | "/changes" | "/reject" => Some(channel_decide(app, installation_id, chat_id, command, argument).unwrap_or_else(|error| error)),
        _ => None,
    }
}

fn channel_start(app: &AppHandle, installation_id: &str, chat_id: &str, argument: &str) -> Result<String, String> {
    let (name, content) = match argument.split_once(char::is_whitespace) {
        Some((name, rest)) => (name.trim(), rest.trim()),
        None => (argument.trim(), ""),
    };
    if name.is_empty() {
        return Err(tr!("用法：/flow 工作流名 内容"));
    }
    with_engine(app, |db, host| {
        let workflow_id: String = db
            .query_row(
                "SELECT id FROM workflows WHERE (name=?1 OR slug=?1) AND enabled=1 AND published_version IS NOT NULL
                 ORDER BY updated_at DESC LIMIT 1",
                [name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| tr!("没有找到已发布的工作流「{name}」", name = name))?;
        let version = store::latest_version(db, &workflow_id)?.ok_or_else(|| tr!("工作流还没有发布，请先发布"))?;
        let def = store::definition(db, &workflow_id, version)?;
        let mut inputs = Map::new();
        if let Some(port) = def.inputs.iter().find(|port| port.primary).or_else(|| def.inputs.first()) {
            inputs.insert(port.name.clone(), json!(content));
        }
        let (run_id, acts) = engine::start_run(db, host, engine::StartRun {
            workflow_id: &workflow_id,
            version: None,
            inputs: Value::Object(inputs),
            source: "channel",
            trigger_ref: Some(chat_id),
            parent_node_run_id: None,
            map_index: None,
            extra: json!({ "channel": { "installationId": installation_id, "chatId": chat_id } }),
            untrusted: true,
        })?;
        let run = store::run(db, &run_id)?;
        let text = if run.status == "skipped" {
            tr!("工作流「{name}」已有运行在进行，这次没有启动。", name = name)
        } else {
            tr!("已开始工作流「{name}」。", name = name)
        };
        Ok((text, acts))
    })
}

fn channel_decide(_app: &AppHandle, _installation_id: &str, _chat_id: &str, _command: &str, _comment: &str) -> Result<String, String> {
    Err(tr!("人工确认改在智能体步骤里完成，这些命令已经不用了。"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn git_commit_and_head_work_in_a_real_repo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        git_output("git", &path, &["init", "-q"]).unwrap();
        assert_eq!(git_commit_all("git", &path, "empty", true).unwrap(), None);
        std::fs::write(dir.path().join("a.txt"), "1").unwrap();
        let head = git_commit_all("git", &path, "wf: first", true).unwrap();
        assert!(head.is_some());
        assert_eq!(git_head("git", &path), head);
    }
}
