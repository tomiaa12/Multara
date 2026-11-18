//! 应用侧：实现 `engine::Host`，在 DB 锁外执行引擎返回的动作（派发 agent、跑脚本/HTTP/Git、发事件、通知）。

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::engine::{self, Action, ForkRequest, Host, Job, JobResult, PreparedSlot, SlotRequest};
use super::model::{Isolation, NodeKind};
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
        let git = {
            let state = app.state::<AppState>();
            state
                .lock_db()
                .ok()
                .and_then(|db| db.query_row("SELECT value FROM app_settings WHERE key='gitExecutable'", [], |row| row.get::<_, String>(0)).ok())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "git".into())
        };
        let inputs = {
            let state = app.state::<AppState>();
            state.lock_db().ok().and_then(|db| store::node_run(&db, &node_run_id).ok()).map(|nr| nr.inputs).unwrap_or(Value::Null)
        };
        let result = match job.kind {
            NodeKind::Script => run_script(&app, &job, &inputs),
            NodeKind::Http => run_http(&job),
            NodeKind::Git => run_git(&git, &job),
            _ => JobResult { outputs: Map::new(), error: Some(("internal".into(), "unsupported job".into())) },
        };
        let mut redacted = result;
        let secrets: BTreeMap<String, String> = job.secret_env.clone();
        if let Some((_, message)) = redacted.error.as_mut() {
            *message = redact_values(message, &secrets);
        }
        for value in redacted.outputs.values_mut() {
            if let Value::String(text) = value {
                *text = redact_values(text, &secrets);
            }
        }
        if let Err(error) = run_engine(&app, |db, host| engine::complete_job(db, host, &node_run_id, redacted)) {
            crate::diag::write(&format!("workflow complete_job failed: {error}"));
        }
    });
}

fn redact_values(text: &str, secrets: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    for value in secrets.values() {
        if value.len() >= 6 && (value.chars().any(|c| c.is_ascii_digit()) || value.len() >= 16) {
            out = out.replace(value.as_str(), "<redacted>");
        }
    }
    out
}

fn tail_utf8(bytes: &[u8], limit: usize) -> String {
    if bytes.len() <= limit {
        return String::from_utf8_lossy(bytes).to_string();
    }
    let mut start = bytes.len() - limit;
    while start < bytes.len() && (bytes[start] & 0xC0) == 0x80 {
        start += 1;
    }
    format!("…{}", String::from_utf8_lossy(&bytes[start..]))
}

fn shell_command(shell: &str, command: &str) -> std::process::Command {
    let shell = if shell == "default" {
        if cfg!(windows) { "pwsh" } else { "sh" }
    } else {
        shell
    };
    match shell {
        "bash" => {
            let mut cmd = crate::process::command("bash");
            cmd.arg("-lc").arg(command);
            cmd
        }
        "pwsh" | "powershell" => {
            let mut cmd = crate::process::command("powershell");
            cmd.args(["-NoProfile", "-NonInteractive", "-Command", command]);
            cmd
        }
        "cmd" => {
            let mut cmd = crate::process::command("cmd");
            cmd.arg("/C").arg(command);
            cmd
        }
        _ => {
            let mut cmd = crate::process::command("sh");
            cmd.arg("-c").arg(command);
            cmd
        }
    }
}

fn run_script(app: &AppHandle, job: &Job, inputs: &Value) -> JobResult {
    let spec = &job.spec;
    let command = spec["command"].as_str().unwrap_or("").to_string();
    let shell = spec["shell"].as_str().unwrap_or("default").to_string();
    let success: Vec<i64> = spec["successExitCodes"].as_array().map(|codes| codes.iter().filter_map(Value::as_i64).collect()).unwrap_or_else(|| vec![0]);
    let fail_on_non_zero = spec["failOnNonZero"].as_bool().unwrap_or(true);
    let parse = spec["parseStdout"].as_str().unwrap_or("none").to_string();
    let max_kb = spec["maxOutputKB"].as_u64().unwrap_or(256).clamp(4, 4096) as usize;

    let fallback_dir = std::env::temp_dir().join("multara-workflow").join(&job.node_run_id);
    let input_dir = job.input_dir.clone().unwrap_or_else(|| fallback_dir.join("inputs"));
    let output_dir = job.output_dir.clone().unwrap_or_else(|| fallback_dir.join("outputs"));
    let _ = std::fs::create_dir_all(&input_dir);
    let _ = std::fs::create_dir_all(&output_dir);
    let log_path = output_dir.parent().unwrap_or(&output_dir).join("script.log");

    let mut cmd = shell_command(&shell, &command);
    let cwd = job.cwd.clone().map(PathBuf::from).unwrap_or_else(|| fallback_dir.clone());
    let _ = std::fs::create_dir_all(&cwd);
    cmd.current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("WF_RUN_ID", &job.run_id)
        .env("WF_NODE_ID", &job.node_id)
        .env("WF_INPUT_DIR", &input_dir)
        .env("WF_OUTPUT_DIR", &output_dir);
    if let Some(map) = inputs.as_object() {
        for (port, value) in map {
            let text = match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            let _ = std::fs::write(input_dir.join(format!("{port}.json")), value.to_string());
            if text.len() <= 32 * 1024 {
                cmd.env(format!("WF_IN_{}", port.to_uppercase()), text);
            }
        }
    }
    let slots = {
        let state = app.state::<AppState>();
        let slots = match state.lock_db() {
            Ok(db) => store::run_workspaces(&db, &job.run_id).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        slots
    };
    for slot in slots {
        cmd.env(format!("WF_WS_{}", slot.slot_key.to_uppercase()), slot.path);
    }
    for (key, value) in &job.secret_env {
        cmd.env(key, value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let started = Instant::now();
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => return JobResult { outputs: Map::new(), error: Some(("script_exit".into(), tr!("无法启动命令：{error}", error = error))) },
    };
    let child = Arc::new(Mutex::new(child));
    if let Ok(mut map) = jobs().lock() {
        map.insert(job.node_run_id.clone(), child.clone());
    }
    let (stdout, stderr) = {
        let mut guard = child.lock().unwrap();
        (guard.stdout.take(), guard.stderr.take())
    };
    let log = Arc::new(Mutex::new(std::fs::File::create(&log_path).ok()));
    let reader = |stream: Option<Box<dyn Read + Send>>, name: &'static str| {
        let app = app.clone();
        let log = log.clone();
        let node_run_id = job.node_run_id.clone();
        thread::spawn(move || {
            let mut collected: Vec<u8> = Vec::new();
            let Some(stream) = stream else { return collected };
            let mut reader = BufReader::new(stream);
            let mut pending = String::new();
            let mut last_emit = Instant::now();
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                collected.extend_from_slice(&line);
                if collected.len() > 8 * 1024 * 1024 {
                    collected.drain(0..collected.len() - 4 * 1024 * 1024);
                }
                if let Ok(mut file) = log.lock() {
                    if let Some(file) = file.as_mut() {
                        let _ = file.write_all(&line);
                    }
                }
                pending.push_str(&String::from_utf8_lossy(&line));
                if last_emit.elapsed() > Duration::from_millis(100) || pending.len() > 16 * 1024 {
                    let _ = app.emit("workflow-node-log", json!({ "nodeRunId": node_run_id, "stream": name, "chunk": pending }));
                    pending.clear();
                    last_emit = Instant::now();
                }
            }
            if !pending.is_empty() {
                let _ = app.emit("workflow-node-log", json!({ "nodeRunId": node_run_id, "stream": name, "chunk": pending }));
            }
            collected
        })
    };
    let out_thread = reader(stdout.map(|s| Box::new(s) as Box<dyn Read + Send>), "stdout");
    let err_thread = reader(stderr.map(|s| Box::new(s) as Box<dyn Read + Send>), "stderr");
    let timeout = job.timeout_minutes.map(|minutes| Duration::from_secs(minutes as u64 * 60));
    let mut timed_out = false;
    let status = loop {
        let polled = child.lock().ok().and_then(|mut guard| guard.try_wait().ok()).flatten();
        if let Some(status) = polled {
            break Some(status);
        }
        if !jobs().lock().map(|map| map.contains_key(&job.node_run_id)).unwrap_or(false) {
            // 被取消
            break child.lock().ok().and_then(|mut guard| guard.wait().ok());
        }
        if timeout.is_some_and(|limit| started.elapsed() > limit) {
            timed_out = true;
            if let Ok(mut guard) = child.lock() {
                crate::executor::kill_process(&mut guard);
            }
            break child.lock().ok().and_then(|mut guard| guard.wait().ok());
        }
        thread::sleep(Duration::from_millis(200));
    };
    if let Ok(mut map) = jobs().lock() {
        map.remove(&job.node_run_id);
    }
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    let code = status.and_then(|status| status.code()).unwrap_or(-1);
    let stdout_text = tail_utf8(&stdout, max_kb * 1024);
    let stderr_text = tail_utf8(&stderr, max_kb * 1024);
    let ok = success.contains(&(code as i64));
    let mut outputs = Map::new();
    outputs.insert("exit_code".into(), json!(code));
    outputs.insert("stdout".into(), json!(stdout_text));
    outputs.insert("stderr".into(), json!(stderr_text));
    outputs.insert("log".into(), json!(log_path.to_string_lossy()));
    outputs.insert("ok".into(), json!(ok));
    let full_stdout = String::from_utf8_lossy(&stdout).to_string();
    let parsed = match parse.as_str() {
        "text" => Some(json!(full_stdout.trim())),
        "json" => serde_json::from_str::<Value>(full_stdout.trim()).ok(),
        "lines" => Some(json!(full_stdout.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>())),
        _ => None,
    };
    if let Some(parsed) = &parsed {
        outputs.insert("parsed".into(), parsed.clone());
    }
    outputs.insert(
        "$result".into(),
        json!({ "exit_code": code, "ok": ok, "stdout": stdout_text, "stderr": stderr_text, "log": log_path.to_string_lossy(), "parsed": parsed }),
    );
    collect_output_dir(&output_dir, &mut outputs);
    if timed_out {
        return JobResult { outputs, error: Some(("timeout".into(), tr!("节点运行超时"))) };
    }
    if parse == "json" && parsed.is_none() && ok {
        return JobResult { outputs, error: Some(("output_invalid".into(), tr!("stdout 不是合法的 JSON"))) };
    }
    if !ok && fail_on_non_zero {
        let tail: String = stderr_text.chars().rev().take(1500).collect::<Vec<_>>().into_iter().rev().collect();
        return JobResult { outputs, error: Some(("script_exit".into(), tr!("命令退出码 {code}：{tail}", code = code, tail = tail))) };
    }
    JobResult { outputs, error: None }
}

/// 脚本写在 $WF_OUTPUT_DIR 里的 `<port>.json` / `<port>.txt`
fn collect_output_dir(dir: &Path, outputs: &mut Map<String, Value>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string) else { continue };
        if stem.starts_with('$') {
            continue;
        }
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => {
                if let Ok(value) = std::fs::read_to_string(&path).map(|raw| serde_json::from_str::<Value>(&raw)) {
                    if let Ok(value) = value {
                        outputs.insert(stem, value);
                    }
                }
            }
            Some("txt") | Some("md") => {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    outputs.insert(stem, json!(text.trim_end()));
                }
            }
            _ => {}
        }
    }
}

fn run_http(job: &Job) -> JobResult {
    let spec = &job.spec;
    let method = spec["method"].as_str().unwrap_or("GET").to_uppercase();
    let url = job.secret_env.get("url").cloned().unwrap_or_default();
    let mut headers: Vec<(String, String)> = job
        .secret_env
        .iter()
        .filter_map(|(key, value)| key.strip_prefix("header:").map(|name| (name.to_string(), value.clone())))
        .collect();
    let body_type = spec["bodyType"].as_str().unwrap_or("none");
    let body = job.secret_env.get("body").cloned();
    if body.is_some() && !headers.iter().any(|(key, _)| key.eq_ignore_ascii_case("content-type")) {
        let content_type = match body_type {
            "json" => "application/json",
            "form" => "application/x-www-form-urlencoded",
            _ => "text/plain; charset=utf-8",
        };
        headers.push(("Content-Type".into(), content_type.into()));
    }
    let timeout = job.timeout_minutes.unwrap_or(2).max(1) * 60;
    let response = match crate::http_client::request(&method, &url, &headers, body.as_deref().filter(|_| body_type != "none"), timeout) {
        Ok(response) => response,
        Err(error) => return JobResult { outputs: Map::new(), error: Some(("http_status".into(), error)) },
    };
    let expect: Vec<u16> = spec["expectStatus"].as_array().map(|items| items.iter().filter_map(|v| v.as_u64()).map(|v| v as u16).collect()).unwrap_or_default();
    let ok = if expect.is_empty() { (200..300).contains(&response.status) } else { expect.contains(&response.status) };
    let parsed_body = if spec["responseType"].as_str() == Some("text") {
        json!(response.body)
    } else {
        serde_json::from_str::<Value>(&response.body).unwrap_or(json!(response.body))
    };
    let headers_json: Map<String, Value> = response.headers.iter().map(|(k, v)| (k.to_lowercase(), json!(v))).collect();
    let mut outputs = Map::new();
    outputs.insert("status".into(), json!(response.status));
    outputs.insert("headers".into(), Value::Object(headers_json.clone()));
    outputs.insert("body".into(), parsed_body.clone());
    outputs.insert("ok".into(), json!(ok));
    outputs.insert("$result".into(), json!({ "status": response.status, "ok": ok, "headers": headers_json, "body": parsed_body }));
    if !ok {
        let snippet: String = response.body.chars().take(500).collect();
        return JobResult { outputs, error: Some(("http_status".into(), tr!("HTTP 状态码 {status}：{body}", status = response.status, body = snippet))) };
    }
    JobResult { outputs, error: None }
}

fn run_git(git: &str, job: &Job) -> JobResult {
    let spec = &job.spec;
    let Some(path) = job.cwd.clone() else {
        return JobResult { outputs: Map::new(), error: Some(("git_failed".into(), tr!("没有工作区目录"))) };
    };
    let op = spec["op"].as_str().unwrap_or("");
    let message = spec["message"].as_str().unwrap_or("").trim().to_string();
    let skip_verify = spec["skipVerify"].as_bool().unwrap_or(false);
    let base = spec["base"].as_str().map(str::to_string);
    let result: Result<Map<String, Value>, String> = (|| {
        let mut outputs = Map::new();
        match op {
            "commit" => {
                let message = if message.is_empty() { tr!("工作流提交") } else { message.clone() };
                git_commit_all(git, &path, &message, skip_verify)?;
            }
            "squash" => {
                let base = base.clone().ok_or_else(|| tr!("不知道从哪个提交开始压缩"))?;
                let message = if message.is_empty() { tr!("工作流提交") } else { message.clone() };
                git_output(git, &path, &["add", "-A"])?;
                git_output(git, &path, &["reset", "--soft", &base])?;
                git_commit_all(git, &path, &message, skip_verify)?;
            }
            "push" => {
                let remote = spec["remote"].as_str().filter(|value| !value.is_empty()).unwrap_or("origin");
                let branch = git_output(git, &path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
                let mut args = vec!["push"];
                if spec["setUpstream"].as_bool().unwrap_or(false) {
                    args.push("-u");
                }
                if skip_verify {
                    args.push("--no-verify");
                }
                args.push(remote);
                args.push(&branch);
                git_output(git, &path, &args)?;
            }
            "merge" => {
                let branch = spec["branch"].as_str().filter(|value| !value.is_empty()).ok_or_else(|| tr!("没有指定要合入的分支"))?;
                git_output(git, &path, &["merge", "--no-ff", "--no-edit", branch])?;
            }
            "create_branch" => {
                let branch = spec["branch"].as_str().filter(|value| !value.is_empty()).ok_or_else(|| tr!("没有指定分支名"))?;
                git_output(git, &path, &["checkout", "-b", branch])?;
            }
            "diff" => {
                let range = match &base {
                    Some(base) => format!("{base}..HEAD"),
                    None => "HEAD".into(),
                };
                let patch = git_output(git, &path, &["diff", &range])?;
                let files = git_output(git, &path, &["diff", "--name-only", &range])?;
                let dir = job.output_dir.clone().unwrap_or_else(|| std::env::temp_dir().join("multara-workflow").join(&job.node_run_id));
                let _ = std::fs::create_dir_all(&dir);
                let file = dir.join("diff.patch");
                std::fs::write(&file, &patch).map_err(|error| error.to_string())?;
                outputs.insert("diff".into(), json!(file.to_string_lossy()));
                outputs.insert("changed_files".into(), json!(files.lines().filter(|line| !line.trim().is_empty()).collect::<Vec<_>>()));
            }
            other => return Err(tr!("不支持的 Git 操作 {op}", op = other)),
        }
        outputs.insert("head".into(), json!(git_head(git, &path)));
        outputs.insert("branch".into(), json!(git_output(git, &path, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()));
        let mut result = outputs.clone();
        result.remove("$result");
        outputs.insert("$result".into(), Value::Object(result));
        Ok(outputs)
    })();
    match result {
        Ok(outputs) => JobResult { outputs, error: None },
        Err(error) => JobResult { outputs: Map::new(), error: Some(("git_failed".into(), error)) },
    }
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
            tr!("已开始工作流「{name}」。需要审批时回复 /approve、/changes 原因 或 /reject。", name = name)
        };
        Ok((text, acts))
    })
}

fn channel_decide(app: &AppHandle, installation_id: &str, chat_id: &str, command: &str, comment: &str) -> Result<String, String> {
    with_engine(app, |db, host| {
        let mut statement = db
            .prepare(
                "SELECT id FROM workflow_runs
                 WHERE json_extract(extra_json, '$.channel.chatId')=?1
                   AND json_extract(extra_json, '$.channel.installationId')=?2
                   AND status IN ('running','waiting')
                 ORDER BY created_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let runs = statement
            .query_map(params![chat_id, installation_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        for run_id in runs {
            let Some(gate) = engine::waiting_gates(db, &run_id)?.into_iter().next() else { continue };
            let run = store::run(db, &gate.run_id)?;
            let def = store::definition(db, &run.workflow_id, run.version)?;
            let node = def.node(&gate.node_id).ok_or_else(|| tr!("节点不存在"))?;
            let cfg: super::model::GateConfig = super::model::config(node)?;
            let decisions = cfg.effective_decisions();
            let port = match command {
                "/approve" => decisions.iter().find(|d| d.primary).or_else(|| decisions.iter().find(|d| d.port == "approve")),
                "/changes" => decisions.iter().find(|d| d.port == "changes" || d.require_comment),
                _ => decisions.iter().find(|d| d.port == "reject"),
            }
            .map(|d| d.port.clone())
            .ok_or_else(|| {
                let labels: Vec<String> = decisions.iter().map(|d| d.label.clone()).collect();
                tr!("这个关卡没有对应的选项，可选：{labels}", labels = labels.join(" / "))
            })?;
            let mut acts = engine::decide_gate(db, host, &gate.id, &port, comment, Value::Null, "feishu")?;
            acts.extend(engine::advance(db, host, &gate.run_id)?);
            if gate.run_id != run_id {
                acts.extend(engine::advance(db, host, &run_id)?);
            }
            return Ok((tr!("已记录审批。"), acts));
        }
        Err(tr!("这个聊天里没有等待审批的工作流。"))
    })
}
