//! 工作流相关的 Tauri 命令（技术方案第 9 节）。

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, State};

use super::engine::{self, StartRun};
use super::model::WorkflowDef;
use super::runtime::{self, with_engine};
use super::store::{self, DbLookup};
use super::validate::{self, Diagnostic};
use super::templates;
use crate::database::AppState;

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn parse_def(value: Value) -> Result<WorkflowDef, String> {
    serde_json::from_value(value).map_err(|error| tr!("工作流定义无法解析：{error}", error = error))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSummary {
    #[serde(flatten)]
    pub workflow: store::Workflow,
    pub last_run_status: Option<String>,
    pub last_run_at: Option<String>,
    pub node_count: usize,
}

fn summaries(db: &Connection) -> Result<Vec<WorkflowSummary>, String> {
    let mut out = Vec::new();
    for workflow in store::list_workflows(db)? {
        let last: Option<(String, String)> = db
            .query_row(
                "SELECT status, created_at FROM workflow_runs WHERE workflow_id=?1 AND parent_node_run_id IS NULL
                 ORDER BY created_at DESC LIMIT 1",
                [&workflow.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        let node_count = store::draft(db, &workflow.id).map(|def| def.nodes.len()).unwrap_or(0);
        out.push(WorkflowSummary {
            last_run_status: last.as_ref().map(|item| item.0.clone()),
            last_run_at: last.map(|item| item.1),
            node_count,
            workflow,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn list_workflows(state: State<'_, AppState>) -> Result<Vec<WorkflowSummary>, String> {
    let db = state.lock_db()?;
    summaries(&db)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDetail {
    pub workflow: store::Workflow,
    pub draft: WorkflowDef,
    /// 最新发布版本的定义（子流程、自动化按它对齐端口）
    pub published: Option<WorkflowDef>,
    pub versions: Vec<store::VersionMeta>,
    pub diagnostics: Vec<Diagnostic>,
}

fn detail(db: &Connection, id: &str) -> Result<WorkflowDetail, String> {
    let draft = store::draft(db, id)?;
    let diagnostics = validate::validate(&draft, &DbLookup(db));
    let workflow = store::workflow(db, id)?;
    let published = match workflow.published_version {
        Some(version) => Some(store::definition(db, id, version)?),
        None => None,
    };
    Ok(WorkflowDetail {
        workflow,
        published,
        draft,
        versions: store::versions(db, id)?,
        diagnostics,
    })
}

#[tauri::command]
pub fn get_workflow(workflow_id: String, state: State<'_, AppState>) -> Result<WorkflowDetail, String> {
    let db = state.lock_db()?;
    detail(&db, &workflow_id)
}

#[tauri::command]
pub fn create_workflow(
    name: String,
    description: Option<String>,
    template_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<store::Workflow, String> {
    let db = state.lock_db()?;
    let def = match template_id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => templates::workflow_template(id, &db)?.definition,
        None => {
            let mut def = WorkflowDef::default();
            def.nodes[0].title = tr!("开始");
            def
        }
    };
    store::create_workflow(&db, &name, description.as_deref().unwrap_or(""), &def, &now())
}

#[tauri::command]
pub fn update_workflow_meta(
    workflow_id: String,
    name: String,
    description: String,
    icon: Option<String>,
    state: State<'_, AppState>,
) -> Result<store::Workflow, String> {
    let db = state.lock_db()?;
    store::update_meta(&db, &workflow_id, &name, &description, icon.as_deref().unwrap_or(""), &now())
}

#[tauri::command]
pub fn set_workflow_enabled(workflow_id: String, enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.lock_db()?;
    store::set_enabled(&db, &workflow_id, enabled, &now())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub updated_at: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[tauri::command]
pub fn save_workflow_draft(
    workflow_id: String,
    draft: Value,
    expected_updated_at: Option<String>,
    state: State<'_, AppState>,
) -> Result<SaveResult, String> {
    let def = parse_def(draft)?;
    let db = state.lock_db()?;
    let updated_at = store::save_draft(&db, &workflow_id, &def, expected_updated_at.as_deref(), &now())?;
    Ok(SaveResult { updated_at, diagnostics: validate::validate(&def, &DbLookup(&db)) })
}

#[tauri::command]
pub fn validate_workflow(draft: Value, state: State<'_, AppState>) -> Result<Vec<Diagnostic>, String> {
    let def = parse_def(draft)?;
    let db = state.lock_db()?;
    Ok(validate::validate(&def, &DbLookup(&db)))
}

#[tauri::command]
pub fn publish_workflow(workflow_id: String, note: Option<String>, state: State<'_, AppState>) -> Result<u32, String> {
    let db = state.lock_db()?;
    let def = store::draft(&db, &workflow_id)?;
    let diagnostics = validate::validate(&def, &DbLookup(&db));
    if validate::has_errors(&diagnostics) {
        let first = diagnostics.iter().find(|item| item.severity == "error").map(|item| item.message.clone()).unwrap_or_default();
        return Err(tr!("还有错误没有解决，不能发布：{first}", first = first));
    }
    store::publish(&db, &workflow_id, note.as_deref().unwrap_or(""), &now())
}

#[tauri::command]
pub fn delete_workflow(workflow_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.lock_db()?;
    let active: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE workflow_id=?1 AND status IN ('queued','running','waiting'))",
            [&workflow_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active {
        return Err(tr!("工作流还有运行没结束，请先取消"));
    }
    store::delete_workflow(&db, &workflow_id)
}

#[tauri::command]
pub fn duplicate_workflow(workflow_id: String, state: State<'_, AppState>) -> Result<store::Workflow, String> {
    let db = state.lock_db()?;
    let source = store::workflow(&db, &workflow_id)?;
    let def = store::draft(&db, &workflow_id)?;
    store::create_workflow(&db, &tr!("{name} 副本", name = source.name), &source.description, &def, &now())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportFile {
    format: String,
    name: String,
    description: String,
    definition: WorkflowDef,
}

#[tauri::command]
pub fn export_workflow(workflow_id: String, path: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.lock_db()?;
    let workflow = store::workflow(&db, &workflow_id)?;
    let file = ExportFile {
        format: "multara-flow/1".into(),
        name: workflow.name,
        description: workflow.description,
        definition: store::draft(&db, &workflow_id)?,
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|error| error.to_string())?;
    std::fs::write(&path, raw).map_err(|error| tr!("写入文件失败：{error}", error = error))
}

#[tauri::command]
pub fn import_workflow(path: String, state: State<'_, AppState>) -> Result<store::Workflow, String> {
    let raw = std::fs::read_to_string(&path).map_err(|error| tr!("读取文件失败：{error}", error = error))?;
    let file: ExportFile = serde_json::from_str(&raw).map_err(|error| tr!("不是有效的工作流文件：{error}", error = error))?;
    if !file.format.starts_with("multara-flow/") {
        return Err(tr!("不是有效的工作流文件：{error}", error = file.format));
    }
    let db = state.lock_db()?;
    store::create_workflow(&db, &file.name, &file.description, &file.definition, &now())
}

// ---------- 模板 ----------

#[tauri::command]
pub fn list_node_templates(state: State<'_, AppState>) -> Result<Vec<templates::NodeTemplate>, String> {
    let db = state.lock_db()?;
    templates::node_templates(&db)
}

#[tauri::command]
pub fn save_node_template(name: String, description: String, template: Value, state: State<'_, AppState>) -> Result<Vec<templates::NodeTemplate>, String> {
    let db = state.lock_db()?;
    templates::save_user_template(&db, &name, &description, &template, &now())?;
    templates::node_templates(&db)
}

#[tauri::command]
pub fn delete_node_template(template_id: String, state: State<'_, AppState>) -> Result<Vec<templates::NodeTemplate>, String> {
    let db = state.lock_db()?;
    db.execute("DELETE FROM workflow_node_templates WHERE id=?1", [&template_id]).map_err(|error| error.to_string())?;
    templates::node_templates(&db)
}

#[tauri::command]
pub fn list_workflow_templates(state: State<'_, AppState>) -> Result<Vec<templates::WorkflowTemplateMeta>, String> {
    let db = state.lock_db()?;
    Ok(templates::workflow_templates(&db))
}

/// 开发模式：把三套示例工作流恢复成内置定义。
#[tauri::command]
pub fn reset_workflow_examples(state: State<'_, AppState>) -> Result<Vec<WorkflowSummary>, String> {
    if !cfg!(debug_assertions) {
        return Err(tr!("仅开发模式可用"));
    }
    let db = state.lock_db()?;
    templates::reset_example_workflows(&db, &now())?;
    summaries(&db)
}

// ---------- 模板预览 ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderPreview {
    pub rendered: Option<String>,
    pub error: Option<String>,
    pub variables: Value,
}

/// 用示例数据渲染一个模板字段，并给出可用变量树（编辑器自动补全）
#[tauri::command]
pub fn workflow_render_preview(
    _draft: Value,
    _node_id: String,
    source: String,
    _is_expression: Option<bool>,
    _sample_inputs: Option<Value>,
) -> Result<RenderPreview, String> {
    Ok(RenderPreview { rendered: Some(source), error: None, variables: json!({}) })
}

// ---------- 运行 ----------

#[tauri::command]
pub fn start_workflow_run(
    workflow_id: String,
    inputs: Value,
    draft: Option<bool>,
    app: AppHandle,
) -> Result<store::Run, String> {
    with_engine(&app, |db, host| {
        let version = if draft.unwrap_or(false) {
            let wf = store::workflow(db, &workflow_id)?;
            let def = store::draft(db, &workflow_id)?;
            let diagnostics = validate::validate(&def, &DbLookup(db));
            if validate::has_errors(&diagnostics) {
                let first = diagnostics.iter().find(|item| item.severity == "error").map(|item| item.message.clone()).unwrap_or_default();
                return Err(tr!("还有错误没有解决，不能运行：{first}", first = first));
            }
            store::snapshot_draft_version(db, &wf.id, &now())?;
            Some(0)
        } else {
            None
        };
        let (run_id, acts) = engine::start_run(db, host, StartRun {
            workflow_id: &workflow_id,
            version,
            inputs,
            source: "manual",
            trigger_ref: None,
            parent_node_run_id: None,
            map_index: None,
            extra: json!({}),
            untrusted: false,
        })?;
        Ok((store::run(db, &run_id)?, acts))
    })
}

#[tauri::command]
pub fn list_workflow_runs(
    workflow_id: Option<String>,
    status: Option<String>,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<store::Run>, String> {
    let db = state.lock_db()?;
    store::list_runs(&db, workflow_id.as_deref(), status.as_deref(), limit.unwrap_or(100))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueMeta {
    pub id: String,
    pub node_run_id: String,
    pub node_id: String,
    pub port: String,
    pub port_type: String,
    pub size: i64,
    pub source: String,
    /// ≤4KB 的值直接带上
    pub preview: Option<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    pub run: store::Run,
    pub workflow: store::Workflow,
    pub definition: WorkflowDef,
    pub node_runs: Vec<store::NodeRun>,
    pub values: Vec<ValueMeta>,
    pub workspaces: Vec<store::RunWorkspace>,
    pub gates: Vec<store::GateDecisionRow>,
    pub loop_counters: std::collections::HashMap<String, u32>,
    pub children: Vec<store::Run>,
    pub usage: Value,
}

fn value_metas(db: &Connection, run_id: &str) -> Result<Vec<ValueMeta>, String> {
    let mut statement = db
        .prepare(
            "SELECT id, node_run_id, node_id, port, type, size, source, CASE WHEN size <= 4096 THEN value_json END
             FROM workflow_values WHERE run_id=?1 ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([run_id], |row| {
            let preview: Option<String> = row.get(7)?;
            Ok(ValueMeta {
                id: row.get(0)?,
                node_run_id: row.get(1)?,
                node_id: row.get(2)?,
                port: row.get(3)?,
                port_type: row.get(4)?,
                size: row.get(5)?,
                source: row.get(6)?,
                preview: preview.and_then(|raw| serde_json::from_str(&raw).ok()),
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn run_usage(db: &Connection, run_id: &str) -> Value {
    // 按节点会话汇总 agent_runs 数量与耗时；token 与费用沿用统计页的数据源时再细分
    let mut statement = match db.prepare(
        "SELECT n.node_id, COUNT(r.id),
                SUM(CASE WHEN r.completed_at IS NOT NULL AND r.started_at IS NOT NULL
                    THEN (julianday(r.completed_at) - julianday(r.started_at)) * 86400000 ELSE 0 END)
         FROM workflow_node_runs n JOIN agent_runs r ON r.session_id = n.session_id
         WHERE n.run_id=?1 GROUP BY n.node_id",
    ) {
        Ok(statement) => statement,
        Err(_) => return json!({}),
    };
    let rows = statement
        .query_map([run_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<f64>>(2)?))
        })
        .map(|rows| rows.filter_map(Result::ok).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut map = Map::new();
    for (node, runs, ms) in rows {
        map.insert(node, json!({ "agentRuns": runs, "elapsedMs": ms.unwrap_or(0.0) as i64 }));
    }
    Value::Object(map)
}

#[tauri::command]
pub fn get_workflow_run(run_id: String, state: State<'_, AppState>) -> Result<RunDetail, String> {
    let db = state.lock_db()?;
    let run = store::run(&db, &run_id)?;
    let children = store::node_runs(&db, &run_id)?
        .iter()
        .filter(|item| matches!(item.kind.as_str(), "subflow" | "map"))
        .map(|item| store::child_runs(&db, &item.id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(RunDetail {
        workflow: store::workflow(&db, &run.workflow_id)?,
        definition: store::definition(&db, &run.workflow_id, run.version)?,
        node_runs: store::node_runs(&db, &run_id)?,
        values: value_metas(&db, &run_id)?,
        workspaces: store::run_workspaces(&db, &run_id)?,
        gates: store::gate_decisions(&db, &run_id)?,
        loop_counters: store::loop_counts(&db, &run_id)?,
        usage: run_usage(&db, &run_id),
        children,
        run,
    })
}

#[tauri::command]
pub fn get_workflow_node_values(node_run_id: String, state: State<'_, AppState>) -> Result<Value, String> {
    let db = state.lock_db()?;
    let node_run = store::node_run(&db, &node_run_id)?;
    Ok(json!({
        "inputs": node_run.inputs,
        "content": node_run.content,
        "metadata": node_run.metadata,
        "nextNodeId": node_run.next_node_id,
    }))
}

#[tauri::command]
pub fn get_workflow_node_log(node_run_id: String, offset: Option<u64>, app: AppHandle) -> Result<Value, String> {
    use std::io::{Read, Seek, SeekFrom};
    let run_id: String = {
        let state = tauri::Manager::state::<AppState>(&app);
        let db = state.lock_db()?;
        store::node_run(&db, &node_run_id)?.run_id
    };
    let root = tauri::Manager::path(&app).app_data_dir().map_err(|error| error.to_string())?;
    let path = root.join("workflow-runs").join(&run_id).join(&node_run_id).join("script.log");
    let Ok(mut file) = std::fs::File::open(&path) else {
        return Ok(json!({ "text": "", "nextOffset": 0, "eof": true }));
    };
    let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let start = offset.unwrap_or(0).min(len);
    file.seek(SeekFrom::Start(start)).map_err(|error| error.to_string())?;
    let mut buffer = Vec::new();
    file.take(512 * 1024).read_to_end(&mut buffer).map_err(|error| error.to_string())?;
    let next = start + buffer.len() as u64;
    Ok(json!({ "text": String::from_utf8_lossy(&buffer), "nextOffset": next, "eof": next >= len }))
}

#[tauri::command]
pub fn list_workflow_events(run_id: String, after_seq: Option<i64>, state: State<'_, AppState>) -> Result<Vec<store::Event>, String> {
    let db = state.lock_db()?;
    store::events(&db, &run_id, after_seq.unwrap_or(0))
}

#[tauri::command]
pub fn cancel_workflow_run(run_id: String, app: AppHandle) -> Result<(), String> {
    runtime::run_engine(&app, |db, host| engine::cancel_run(db, host, &run_id, "user_cancelled"))
}

#[tauri::command]
pub fn retry_workflow_node(run_id: String, node_id: String, reset_workspace: Option<bool>, app: AppHandle) -> Result<(), String> {
    runtime::run_engine(&app, |db, host| engine::retry_node(db, host, &run_id, &node_id, reset_workspace.unwrap_or(false)))
}

#[tauri::command]
pub fn skip_workflow_node(run_id: String, node_id: String, outputs: Value, app: AppHandle) -> Result<(), String> {
    runtime::run_engine(&app, |db, host| engine::skip_with_outputs(db, host, &run_id, &node_id, outputs))
}

#[tauri::command]
pub fn confirm_workflow_node(node_run_id: String, target: String, app: AppHandle) -> Result<(), String> {
    runtime::run_engine(&app, |db, host| engine::choose_route(db, host, &node_run_id, &target))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutcome {
    pub task_ids: Vec<String>,
    pub artifact_dir: Option<String>,
    pub run_id: Option<String>,
}

fn artifact_of(run: &store::Run) -> Option<String> {
    run.extra.get("artifactDir").and_then(Value::as_str).filter(|path| !path.is_empty()).map(str::to_string)
}

fn latest_run_id(db: &Connection, workflow_id: &str) -> Option<String> {
    db.query_row(
        "SELECT id FROM workflow_runs WHERE workflow_id=?1 AND parent_node_run_id IS NULL ORDER BY created_at DESC LIMIT 1",
        [workflow_id],
        |row| row.get(0),
    )
    .ok()
}

#[tauri::command]
pub fn workflow_outcome(workflow_id: String, run_id: Option<String>, state: State<'_, AppState>) -> Result<WorkflowOutcome, String> {
    let db = state.lock_db()?;
    let task_ids = store::linked_task_ids(&db, &workflow_id)?;
    let resolved = run_id.filter(|id| !id.is_empty()).or_else(|| latest_run_id(&db, &workflow_id));
    let run = resolved.as_deref().and_then(|id| store::run(&db, id).ok());
    Ok(WorkflowOutcome {
        task_ids,
        artifact_dir: run.as_ref().and_then(artifact_of),
        run_id: run.map(|item| item.id),
    })
}

#[tauri::command]
pub fn set_workflow_tasks(workflow_id: String, task_ids: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.lock_db()?;
    store::set_linked_task_ids(&db, &workflow_id, &task_ids)
}

#[tauri::command]
pub fn list_workflow_directory(run_id: String, path: Option<String>, state: State<'_, AppState>) -> Result<Vec<crate::models::FileEntry>, String> {
    let db = state.lock_db()?;
    let run = store::run(&db, &run_id)?;
    let Some(root) = artifact_of(&run) else { return Ok(Vec::new()) };
    if !std::path::Path::new(&root).is_dir() {
        return Ok(Vec::new());
    }
    crate::files::list_directory(&run_id, &root, path.as_deref())
}

#[tauri::command]
pub fn read_workflow_file(run_id: String, path: String, state: State<'_, AppState>) -> Result<crate::models::FileContent, String> {
    let db = state.lock_db()?;
    let run = store::run(&db, &run_id)?;
    let Some(root) = artifact_of(&run) else {
        return Err(tr!("这次运行还没有文件目录"));
    };
    crate::files::read(&run_id, &root, &path)
}

#[tauri::command]
pub fn delete_workflow_run(run_id: String, remove_worktrees: Option<bool>, app: AppHandle) -> Result<(), String> {
    let (workspaces, git) = {
        let state = tauri::Manager::state::<AppState>(&app);
        let db = state.lock_db()?;
        let run = store::run(&db, &run_id)?;
        if !run.is_terminal() {
            return Err(tr!("运行还没结束，请先取消"));
        }
        let mut owned = Vec::new();
        if remove_worktrees.unwrap_or(false) {
            for slot in store::run_workspaces(&db, &run_id)?.into_iter().filter(|slot| slot.owned) {
                if let Ok(workspace) = load_workspace(&db, &slot.workspace_id) {
                    owned.push(workspace);
                }
            }
        }
        let git = db
            .query_row("SELECT value FROM app_settings WHERE key='gitExecutable'", [], |row| row.get::<_, String>(0))
            .unwrap_or_else(|_| "git".into());
        for workspace in &owned {
            let _ = db.execute("DELETE FROM workspaces WHERE id=?1", [&workspace.id]);
        }
        let _ = db.execute("DELETE FROM chat_sessions WHERE id=?1", [&run.overview_session_id]);
        let _ = db.execute("DELETE FROM chat_sessions WHERE id IN (SELECT session_id FROM workflow_node_runs WHERE run_id=?1)", [&run_id]);
        let _ = db.execute("DELETE FROM inbox_items WHERE session_id=?1", [&run.overview_session_id]);
        if let Some(task_id) = &run.task_id {
            let _ = db.execute("DELETE FROM tasks WHERE id=?1", [task_id]);
        }
        store::delete_run(&db, &run_id)?;
        (owned, if git.trim().is_empty() { "git".to_string() } else { git })
    };
    for workspace in workspaces {
        crate::workspace::remove(&workspace, &git);
    }
    if let Ok(root) = tauri::Manager::path(&app).app_data_dir() {
        let _ = std::fs::remove_dir_all(root.join("workflow-runs").join(&run_id));
    }
    Ok(())
}

fn load_workspace(db: &Connection, id: &str) -> Result<crate::models::Workspace, String> {
    db.query_row(
        "SELECT id, project_id, session_id, source_workspace_id, name, kind, source_path, work_path, branch, isolation_mode, created_at
         FROM workspaces WHERE id=?1",
        [id],
        crate::session::workspace_from_row,
    )
    .map_err(|error| error.to_string())
}

// ---------- 密钥 ----------

#[tauri::command]
pub fn list_workflow_secrets(state: State<'_, AppState>) -> Result<Vec<store::SecretName>, String> {
    let db = state.lock_db()?;
    store::secret_names(&db)
}

#[tauri::command]
pub fn set_workflow_secret(name: String, value: String, description: Option<String>, app: AppHandle) -> Result<Vec<store::SecretName>, String> {
    let name = name.trim().to_string();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(tr!("密钥名只能用字母、数字和下划线"));
    }
    if value.is_empty() {
        return Err(tr!("密钥值不能为空"));
    }
    runtime::set_secret(&app, &name, &value)?;
    let state = tauri::Manager::state::<AppState>(&app);
    let db = state.lock_db()?;
    store::upsert_secret_name(&db, &name, description.as_deref().unwrap_or(""), &now())?;
    store::secret_names(&db)
}

#[tauri::command]
pub fn delete_workflow_secret(name: String, app: AppHandle) -> Result<Vec<store::SecretName>, String> {
    runtime::delete_secret(&app, &name);
    let state = tauri::Manager::state::<AppState>(&app);
    let db = state.lock_db()?;
    store::delete_secret_name(&db, &name)?;
    store::secret_names(&db)
}

/// 自动化触发工作流（`commands::start_automation_run` 调用）
pub fn start_from_automation(
    db: &Connection,
    host: &dyn engine::Host,
    workflow_id: &str,
    inputs_template: &str,
    prompt: &str,
    payload: Option<&str>,
    automation_run_id: &str,
) -> Result<(String, Vec<engine::Action>), String> {
    let _ = (payload, prompt);
    let inputs = if inputs_template.trim().is_empty() {
        let wf_version = store::latest_version(db, workflow_id)?.ok_or_else(|| tr!("工作流还没有发布，请先发布"))?;
        let def = store::definition(db, workflow_id, wf_version)?;
        let mut map = Map::new();
        if let Some(field) = def.inputs.iter().find(|field| field.primary).or_else(|| def.inputs.first()) {
            map.insert(field.name.clone(), json!(prompt));
        }
        Value::Object(map)
    } else {
        serde_json::from_str(inputs_template).map_err(|error| tr!("入参映射不是合法的 JSON：{error}", error = error))?
    };
    let _ = params![];
    engine::start_run(db, host, StartRun {
        workflow_id,
        version: None,
        inputs,
        source: "automation",
        trigger_ref: Some(automation_run_id),
        parent_node_run_id: None,
        map_index: None,
        extra: json!({}),
        untrusted: payload.is_some(),
    })
}
