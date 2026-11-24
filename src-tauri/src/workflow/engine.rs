//! 执行引擎（技术方案第 3 节）。所有入口都是「拿着 DB 连接做完状态变更，返回需要在锁外执行的动作」，
//! 这样同一套逻辑既能在应用里跑（`runtime.rs` 执行动作），也能在单测里用假 Host 驱动。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::graph::Graph;
use super::model::*;
use super::store::{self, NodeRun, Run};

pub const WF_RUN_PREFIX: &str = "wf-run:";
pub const WF_NODE_PREFIX: &str = "wf-node:";
const MAX_SUBFLOW_DEPTH: usize = 5;

// ---------- 宿主与动作 ----------

pub struct SlotRequest<'a> {
    pub run_id: &'a str,
    pub overview_session_id: &'a str,
    pub slot: &'a WorkspaceSlotDef,
    pub project_id: &'a str,
    pub branch: Option<String>,
}

pub struct ForkRequest<'a> {
    pub run_id: &'a str,
    pub overview_session_id: &'a str,
    pub source_workspace_id: &'a str,
    pub branch: String,
}

pub struct PreparedSlot {
    pub workspace_id: String,
    pub base_commit: Option<String>,
}

/// 引擎依赖的外部能力：时间、git、工作区、密钥。应用里由 `runtime::AppHost` 实现。
pub trait Host {
    fn now(&self) -> String;
    fn artifacts_root(&self) -> Option<PathBuf>;
    fn secrets(&self) -> BTreeMap<String, String>;
    fn git_head(&self, path: &str) -> Option<String>;
    fn git_commit_all(&self, path: &str, message: &str) -> Result<Option<String>, String>;
    fn git_reset_hard(&self, path: &str, commit: &str) -> Result<(), String>;
    fn prepare_slot(&self, db: &Connection, request: &SlotRequest<'_>) -> Result<PreparedSlot, String>;
    fn create_fork(&self, db: &Connection, request: &ForkRequest<'_>) -> Result<PreparedSlot, String>;
}

/// 后台节点（script / http / git / notify）的执行说明，渲染好了模板。
#[derive(Debug, Clone)]
pub struct Job {
    pub node_run_id: String,
    pub run_id: String,
    pub node_id: String,
    pub kind: NodeKind,
    /// 渲染后的配置（不含密钥）
    pub spec: Value,
    /// 只在内存里的环境变量 / 请求头（可能含密钥）
    pub secret_env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub input_dir: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub timeout_minutes: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct JobResult {
    pub outputs: Map<String, Value>,
    pub error: Option<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum Action {
    DispatchAgents,
    Job(Job),
    StopSession(String),
    KillJob(String),
    RunChanged {
        run_id: String,
        workflow_id: String,
        status: String,
    },
    NodeChanged {
        run_id: String,
        node_run_id: String,
        node_id: String,
        status: String,
    },
    SessionChanged(String),
    FeishuNotify {
        session_id: String,
        text: String,
    },
    RunFinished {
        run_id: String,
        status: String,
        trigger_source: String,
        trigger_ref: Option<String>,
        error: Option<String>,
    },
}

type Acts = Vec<Action>;

fn node_changed(nr: &NodeRun, status: &str) -> Action {
    Action::NodeChanged {
        run_id: nr.run_id.clone(),
        node_run_id: nr.id.clone(),
        node_id: nr.node_id.clone(),
        status: status.to_string(),
    }
}

fn plus_seconds(now: &str, seconds: i64) -> String {
    chrono::DateTime::parse_from_rfc3339(now)
        .map(|time| (time + chrono::Duration::seconds(seconds)).to_rfc3339())
        .unwrap_or_else(|_| now.to_string())
}

fn before(a: &str, b: &str) -> bool {
    match (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(a), Ok(b)) => a <= b,
        _ => a <= b,
    }
}

// ---------- 启动运行 ----------

pub struct StartRun<'a> {
    pub workflow_id: &'a str,
    /// None = 最新发布版本；Some(0) = 草稿试运行
    pub version: Option<u32>,
    pub inputs: Value,
    pub source: &'a str,
    pub trigger_ref: Option<&'a str>,
    pub parent_node_run_id: Option<&'a str>,
    pub map_index: Option<i64>,
    pub extra: Value,
    /// 外部来源（webhook / 渠道）的入参不可信
    pub untrusted: bool,
}

fn blank_input(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        _ => false,
    }
}

/// 调用方没填时用入参上的默认值。必填且没有默认值才拒绝。
fn resolve_input(field: &InputField, given: Option<&Value>) -> Result<Value, String> {
    if let Some(value) = given.filter(|value| !blank_input(value)) {
        return Ok(value.clone());
    }
    if let Some(default) = field.default.clone().filter(|value| !blank_input(value)) {
        return Ok(default);
    }
    if field.required {
        return Err(tr!("缺少必填入参 {name}", name = field.label.clone().unwrap_or_else(|| field.name.clone())));
    }
    Ok(Value::Null)
}

pub fn start_run(db: &Connection, host: &dyn Host, request: StartRun<'_>) -> Result<(String, Acts), String> {
    let now = host.now();
    let workflow = store::workflow(db, request.workflow_id)?;
    if !workflow.enabled && request.parent_node_run_id.is_none() && request.source != "manual" {
        return Err(tr!("工作流已停用"));
    }
    let version = match request.version {
        Some(version) => version,
        None => workflow
            .published_version
            .ok_or_else(|| tr!("工作流还没有发布，请先发布"))?,
    };
    let def = store::definition(db, request.workflow_id, version)?;
    if request.parent_node_run_id.is_some() {
        let depth = parent_depth(db, request.parent_node_run_id)?;
        if depth >= MAX_SUBFLOW_DEPTH {
            return Err(tr!("子工作流嵌套超过 5 层"));
        }
    }

    // 入参
    let mut inputs = Map::new();
    let given = request.inputs.as_object().cloned().unwrap_or_default();
    for field in &def.inputs {
        inputs.insert(field.name.clone(), resolve_input(field, given.get(&field.name))?);
    }
    let mut extra = request.extra.as_object().cloned().unwrap_or_default();
    if request.untrusted {
        extra.insert("untrusted".into(), Value::Bool(true));
    }
    let extra = Value::Object(extra);
    let inputs = Value::Object(inputs);

    // 等人确认不算占用。只有真有智能体在跑，并且策略是「已在运行则跳过」时，才不开始新的一次。
    let mut status = "running";
    if request.parent_node_run_id.is_none() && def.settings.concurrency == Concurrency::SkipIfRunning {
        let active: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE workflow_id=?1 AND parent_node_run_id IS NULL AND status='running')",
                [request.workflow_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if active {
            status = "skipped";
        }
    }

    let run_id = uuid::Uuid::new_v4().to_string();
    let overview = format!("{WF_RUN_PREFIX}{run_id}");
    store::insert_run(
        db,
        store::NewRun {
            id: &run_id,
            workflow_id: request.workflow_id,
            version,
            status,
            inputs: &inputs,
            trigger_source: request.source,
            trigger_ref: request.trigger_ref,
            parent_node_run_id: request.parent_node_run_id,
            map_index: request.map_index,
            extra: &extra,
            overview_session_id: &overview,
            now: &now,
        },
    )?;
    if status == "skipped" {
        store::set_run_status(db, &run_id, "skipped", Some("already_running"), Some(&tr!("同一工作流已有运行在进行")), &now)?;
        return Ok((run_id, Vec::new()));
    }
    let acts = begin_run(db, host, &run_id)?;
    Ok((run_id, acts))
}

/// CLI 等没有应用句柄的地方：只写一条排队的运行，由应用的定时器开始
pub fn enqueue_run(db: &Connection, workflow_id: &str, inputs: Value, source: &str, now: &str) -> Result<String, String> {
    let workflow = store::workflow(db, workflow_id)?;
    let version = workflow.published_version.ok_or_else(|| tr!("工作流还没有发布，请先发布"))?;
    let def = store::definition(db, workflow_id, version)?;
    let given = inputs.as_object().cloned().unwrap_or_default();
    let mut checked = Map::new();
    for field in &def.inputs {
        checked.insert(field.name.clone(), resolve_input(field, given.get(&field.name))?);
    }
    let run_id = uuid::Uuid::new_v4().to_string();
    store::insert_run(db, store::NewRun {
        id: &run_id,
        workflow_id,
        version,
        status: "queued",
        inputs: &Value::Object(checked),
        trigger_source: source,
        trigger_ref: None,
        parent_node_run_id: None,
        map_index: None,
        extra: &json!({}),
        overview_session_id: &format!("{WF_RUN_PREFIX}{run_id}"),
        now,
    })?;
    Ok(run_id)
}

/// 定时器接手 CLI 写下的排队运行。等人确认不算进行中；只有「已在运行则跳过」才会留下不开始。
fn start_queued(db: &Connection, host: &dyn Host) -> Result<Acts, String> {
    let mut statement = db
        .prepare("SELECT id, workflow_id, version FROM workflow_runs WHERE status='queued' AND parent_node_run_id IS NULL ORDER BY created_at")
        .map_err(|error| error.to_string())?;
    let queued = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, u32>(2)?)))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut acts = Vec::new();
    let mut started: Vec<String> = Vec::new();
    for (run_id, workflow_id, version) in queued {
        let skip = store::definition(db, &workflow_id, version)
            .map(|def| def.settings.concurrency == Concurrency::SkipIfRunning)
            .unwrap_or(false);
        let active: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE workflow_id=?1 AND parent_node_run_id IS NULL AND status='running')",
                [&workflow_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if skip && (active || started.contains(&workflow_id)) {
            continue;
        }
        started.push(workflow_id);
        acts.extend(begin_run(db, host, &run_id)?);
    }
    Ok(acts)
}

fn parent_depth(db: &Connection, parent: Option<&str>) -> Result<usize, String> {
    let mut depth = 0;
    let mut cursor: Option<String> = parent.map(str::to_string);
    while let Some(node_run_id) = cursor {
        depth += 1;
        if depth > MAX_SUBFLOW_DEPTH + 1 {
            break;
        }
        cursor = db
            .query_row(
                "SELECT r.parent_node_run_id FROM workflow_node_runs n JOIN workflow_runs r ON r.id=n.run_id WHERE n.id=?1",
                [&node_run_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .flatten();
    }
    Ok(depth)
}

/// 真正开始：建总览会话、任务卡、准备工作区、建节点运行
fn begin_run(db: &Connection, host: &dyn Host, run_id: &str) -> Result<Acts, String> {
    let now = host.now();
    let run = store::run(db, run_id)?;
    let def = store::definition(db, &run.workflow_id, run.version)?;
    let workflow = store::workflow(db, &run.workflow_id)?;
    store::set_run_status(db, run_id, "running", None, None, &now)?;
    if let Some(root) = host.artifacts_root() {
        let dir = root.join(run_id);
        if std::fs::create_dir_all(&dir).is_ok() {
            let path = dir.to_string_lossy().to_string();
            let _ = db.execute(
                "UPDATE workflow_runs SET extra_json = json_set(COALESCE(NULLIF(extra_json, ''), '{}'), '$.artifactDir', ?1) WHERE id=?2",
                params![path, run_id],
            );
        }
    }

    // 工作区槽位
    let base_context = run_context(db, &run, &def, &workflow.name, &workflow.slug)?;
    let mut project_ids: Vec<String> = Vec::new();
    for slot in &def.workspaces {
        let result = prepare_slot(db, host, &run, slot, &base_context);
        match result {
            Ok(project_id) => {
                if !project_ids.contains(&project_id) {
                    project_ids.push(project_id);
                }
            }
            Err(error) => {
                ensure_overview(db, &run, &def, &workflow.name, project_ids.first().map(String::as_str), &now)?;
                store::set_run_status(db, run_id, "failed", Some("workspace_prepare_failed"), Some(&error), &now)?;
                let mut acts = vec![Action::RunChanged {
                    run_id: run.id.clone(),
                    workflow_id: run.workflow_id.clone(),
                    status: "failed".into(),
                }];
                acts.extend(after_terminal(db, host, run_id)?);
                return Ok(acts);
            }
        }
    }
    ensure_overview(db, &run, &def, &workflow.name, project_ids.first().map(String::as_str), &now)?;
    append_overview(
        db,
        &run.overview_session_id,
        &tr!("工作流「{name}」v{version} 开始运行。", name = workflow.name, version = run.version),
        &[],
        &now,
    )?;

    for node in &def.nodes {
        store::insert_node_run(
            db,
            store::NewNodeRun {
                run_id,
                node_id: &node.id,
                kind: node.kind.as_str(),
                iteration: 0,
                attempt: 1,
                carried: None,
                retry_at: None,
                now: &now,
            },
        )?;
    }
    store::event(db, run_id, None, "info", &tr!("运行开始"), json!({ "version": run.version }), &now)?;
    let mut acts = vec![Action::RunChanged {
        run_id: run.id.clone(),
        workflow_id: run.workflow_id.clone(),
        status: "running".into(),
    }];
    acts.extend(advance(db, host, run_id)?);
    Ok(acts)
}

fn prepare_slot(db: &Connection, host: &dyn Host, run: &Run, slot: &WorkspaceSlotDef, context: &Value) -> Result<String, String> {
    let project_id = slot.project.trim();
    if project_id.is_empty() {
        return Err(tr!("工作区槽位 {slot} 没有指定需求", slot = slot.key));
    }
    let branch = match &slot.branch {
        Some(branch) if !branch.trim().is_empty() => Some(branch.clone()),
        _ if slot.isolation == Isolation::WorktreeNew => {
            let slug = context["workflow"]["slug"].as_str().unwrap_or("flow");
            Some(format!("wf/{slug}-{}", run.short_id()))
        }
        _ => None,
    };
    let prepared = host.prepare_slot(
        db,
        &SlotRequest {
            run_id: &run.id,
            overview_session_id: &run.overview_session_id,
            slot,
            project_id,
            branch,
        },
    )?;
    store::insert_run_workspace(db, &run.id, &slot.key, &prepared.workspace_id, prepared.base_commit.as_deref(), true)?;
    Ok(project_id.to_string())
}

fn ensure_overview(db: &Connection, run: &Run, def: &WorkflowDef, name: &str, project_id: Option<&str>, now: &str) -> Result<(), String> {
    let exists: bool = db
        .query_row("SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id=?1)", [&run.overview_session_id], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if exists {
        return Ok(());
    }
    let owner = owner_agent(db, def)?;
    let title = format!("{} · {}", tr!("工作流"), run_title(name, def, &run.inputs));
    db.execute(
        "INSERT INTO chat_sessions (id, project_id, title, target_type, target_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'agent', ?4, ?5, ?5)",
        params![run.overview_session_id, project_id, title, owner, now],
    )
    .map_err(|error| error.to_string())?;
    if let Some(project_id) = project_id {
        crate::session::replace_linked_projects(db, &run.overview_session_id, &[project_id.to_string()])?;
    }
    Ok(())
}

fn owner_agent(db: &Connection, def: &WorkflowDef) -> Result<String, String> {
    let wanted = def.settings.owner_agent_id.trim();
    let exists: bool = db
        .query_row("SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1)", [wanted], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if exists {
        return Ok(wanted.to_string());
    }
    db.query_row("SELECT id FROM agents ORDER BY rowid LIMIT 1", [], |row| row.get(0))
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| tr!("请先创建一个智能体"))
}

fn run_title(name: &str, def: &WorkflowDef, inputs: &Value) -> String {
    let primary = def
        .inputs
        .iter()
        .find(|port| port.primary)
        .or_else(|| def.inputs.first())
        .and_then(|port| inputs.get(&port.name))
        .map(|value| match value {
            Value::String(text) => text.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    let primary: String = primary.chars().take(60).collect();
    if primary.trim().is_empty() {
        name.to_string()
    } else {
        format!("{name} · {primary}")
    }
}

pub fn append_overview(db: &Connection, session_id: &str, content: &str, quick_actions: &[Value], now: &str) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, quick_actions_json, created_at)
         VALUES (?1, ?2, 'assistant', ?3, ?4, ?5)",
        params![id, session_id, content, Value::Array(quick_actions.to_vec()).to_string(), now],
    )
    .map_err(|error| error.to_string())?;
    db.execute("UPDATE chat_sessions SET updated_at=?1 WHERE id=?2", params![now, session_id])
        .map_err(|error| error.to_string())?;
    Ok(id)
}

// ---------- 上下文 ----------

fn run_context(db: &Connection, run: &Run, def: &WorkflowDef, name: &str, slug: &str) -> Result<Value, String> {
    let mut ws = Map::new();
    for slot in store::run_workspaces(db, &run.id)? {
        ws.insert(
            slot.slot_key.clone(),
            json!({
                "path": slot.path,
                "branch": slot.branch,
                "head": slot.head_commit,
                "base": slot.base_commit,
                "workspace_id": slot.workspace_id,
            }),
        );
    }
    let mut context = json!({
        "run": {
            "id": run.id,
            "short_id": run.short_id(),
            "inputs": run.inputs,
            "started_at": run.started_at,
            "source": run.trigger_source,
        },
        "workflow": { "id": run.workflow_id, "name": name, "slug": slug, "version": run.version },
        "ws": ws,
        "iteration": 0,
        "attempt": 1,
        "inputs": {},
        "nodes": {},
    });
    if let Some(extra) = run.extra.as_object() {
        for key in ["item", "index", "parent"] {
            if let Some(value) = extra.get(key) {
                context[key] = value.clone();
            }
        }
    }
    let _ = def;
    Ok(context)
}

fn node_context(db: &Connection, run: &Run, def: &WorkflowDef, _graph: &Graph<'_>, node: &NodeDef, nr: &NodeRun, _inputs: &Value) -> Result<Value, String> {
    let workflow = store::workflow(db, &run.workflow_id)?;
    let mut context = run_context(db, run, def, &workflow.name, &workflow.slug)?;
    if let Some(fork) = &nr.fork_workspace_id {
        if let Some((path, branch, _)) = store::workspace_path(db, fork)? {
            for usage in node.workspaces.iter().filter(|usage| usage.fork) {
                context["ws"][&usage.slot]["path"] = json!(path);
                context["ws"][&usage.slot]["branch"] = json!(branch);
            }
        }
    }
    context["iteration"] = json!(nr.iteration);
    context["attempt"] = json!(nr.attempt);
    context["node"] = json!({ "id": node.id, "title": node.title });
    Ok(context)
}

enum Readiness {
    Ready,
    Wait,
    Skip(String),
}

fn next_targets(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.map(str::trim).filter(|text| !text.is_empty()) else {
        return Vec::new();
    };
    if let Some(text) = raw.strip_prefix('[') {
        if text.ends_with(']') {
            if let Ok(Value::Array(items)) = serde_json::from_str(raw) {
                return items.into_iter().filter_map(|item| item.as_str().map(str::to_string)).collect();
            }
        }
    }
    vec![raw.to_string()]
}

fn chose(nr: &NodeRun, target: &str) -> bool {
    next_targets(nr.next_node_id.as_deref()).iter().any(|id| id == target)
}

fn encode_next(ids: &[String]) -> Option<String> {
    match ids {
        [] => None,
        [one] => Some(one.clone()),
        many => serde_json::to_string(many).ok(),
    }
}

fn readiness(_def: &WorkflowDef, graph: &Graph<'_>, node: &NodeDef, _nr: &NodeRun, current: &HashMap<String, NodeRun>, _now: &str) -> Readiness {
    let incoming = graph.forward_in.get(node.id.as_str()).cloned().unwrap_or_default();
    if incoming.is_empty() {
        return if node.kind == NodeKind::Trigger {
            Readiness::Ready
        } else {
            Readiness::Skip("unreachable".into())
        };
    }
    let mut live = false;
    let mut undecided = false;
    for edge in incoming {
        let Some(source) = current.get(&edge.source) else {
            undecided = true;
            continue;
        };
        if !source.is_terminal() {
            undecided = true;
            continue;
        }
        if source.status == "succeeded" && chose(source, &node.id) {
            live = true;
        }
    }
    // 有多条入线时，要等每一条都结束，避免只跑完其中一条就往下走。
    if undecided {
        Readiness::Wait
    } else if live {
        Readiness::Ready
    } else {
        Readiness::Skip("not_chosen".into())
    }
}

fn chosen_predecessors<'a>(def: &WorkflowDef, node: &NodeDef, current: &'a HashMap<String, NodeRun>) -> Vec<&'a NodeRun> {
    let mut seen = HashSet::new();
    def.edges.iter().filter_map(|edge| {
        if edge.target != node.id || !seen.insert(edge.source.clone()) {
            return None;
        }
        let source = current.get(&edge.source)?;
        chose(source, &node.id).then_some(source)
    }).collect()
}

fn chosen_predecessor<'a>(def: &WorkflowDef, node: &NodeDef, current: &'a HashMap<String, NodeRun>) -> Option<&'a NodeRun> {
    chosen_predecessors(def, node, current).into_iter().next()
}


fn step_title(def: &WorkflowDef, id: &str) -> String {
    def.node(id).map(display_name).unwrap_or_else(|| id.to_string())
}

fn context_steps(def: &WorkflowDef, graph: &Graph<'_>, node: &NodeDef, current: &HashMap<String, NodeRun>, scope: &str) -> Vec<super::brief::StepText> {
    let ids: Vec<String> = if scope == "all" {
        let mut ids: Vec<String> = graph.ancestors(&node.id).into_iter().collect();
        ids.sort_by_key(|id| graph.index.get(id.as_str()).copied().unwrap_or(0));
        ids
    } else {
        chosen_predecessors(def, node, current).into_iter().map(|item| item.node_id.clone()).collect()
    };
    ids.into_iter()
        .filter_map(|id| {
            let item = current.get(&id)?;
            if item.content.trim().is_empty() {
                return None;
            }
            Some(super::brief::StepText { title: step_title(def, &id), content: item.content.clone() })
        })
        .collect()
}

fn single_target(def: &WorkflowDef, node_id: &str) -> Result<Option<String>, String> {
    let edges = def.outgoing(node_id);
    match edges.len() {
        0 => Ok(None),
        1 => Ok(Some(edges[0].target.clone())),
        _ => Err(tr!("这一步有多条出路，不能自动继续")),
    }
}


// ---------- 推进 ----------

struct Loaded {
    run: Run,
    def: WorkflowDef,
}

fn load(db: &Connection, run_id: &str) -> Result<Loaded, String> {
    let run = store::run(db, run_id)?;
    let def = store::definition(db, &run.workflow_id, run.version)?;
    Ok(Loaded { run, def })
}

pub fn advance(db: &Connection, host: &dyn Host, run_id: &str) -> Result<Acts, String> {
    let mut acts = Vec::new();
    let Loaded { run, def } = load(db, run_id)?;
    if run.is_terminal() || run.status == "queued" {
        return Ok(acts);
    }
    let graph = Graph::new(&def);
    let order = graph
        .topo_order()
        .map_err(|cyclic| tr!("存在环：{nodes}", nodes = cyclic.join(", ")))?;
    for _guard in 0..10_000 {
        let now = host.now();
        let mut progressed = false;
        let current = store::current_node_runs(db, run_id)?;
        for node_id in &order {
            let Some(nr) = current.get(*node_id) else { continue };
            if nr.status != "pending" {
                continue;
            }
            let node = def.node(node_id).expect("node in order");
            match readiness(&def, &graph, node, nr, &current, &now) {
                Readiness::Wait => {}
                Readiness::Skip(reason) => {
                    store::set_node_field(db, &nr.id, "skip_reason", Some(&reason))?;
                    store::set_node_status(db, &nr.id, "skipped", &now)?;
                    acts.push(node_changed(nr, "skipped"));
                    progressed = true;
                }
                Readiness::Ready => {
                    store::set_node_status(db, &nr.id, "ready", &now)?;
                    acts.push(node_changed(nr, "ready"));
                    progressed = true;
                }
            }
        }
        if store::run(db, run_id)?.is_terminal() {
            return Ok(acts);
        }
        // 启动 ready 节点
        let current = store::current_node_runs(db, run_id)?;
        let mut running = current.values().filter(|item| item.status == "running").count() as u32;
        for node_id in &order {
            let Some(nr) = current.get(*node_id) else { continue };
            if nr.status != "ready" {
                continue;
            }
            if running >= def.settings.max_parallel_nodes.max(1) {
                break;
            }
            let node = def.node(node_id).expect("node in order");
            if !locks_available(db, &run, node, nr)? {
                continue;
            }
            let started = start_node(db, host, &run, &def, &graph, node, nr)?;
            acts.extend(started);
            running += 1;
            progressed = true;
            if store::run(db, run_id)?.is_terminal() {
                return Ok(acts);
            }
        }
        if !progressed {
            break;
        }
    }
    acts.extend(update_run_status(db, host, run_id)?);
    Ok(acts)
}

fn write_slots<'a>(node: &'a NodeDef) -> impl Iterator<Item = &'a WorkspaceUse> {
    node.workspaces
        .iter()
        .filter(|usage| usage.access == WorkspaceAccess::Write && !usage.fork)
}

fn slot_workspace(db: &Connection, run_id: &str, slot: &str) -> Result<Option<store::RunWorkspace>, String> {
    Ok(store::run_workspaces(db, run_id)?.into_iter().find(|item| item.slot_key == slot))
}

fn locks_available(db: &Connection, run: &Run, node: &NodeDef, nr: &NodeRun) -> Result<bool, String> {
    for usage in write_slots(node) {
        if let Some(slot) = slot_workspace(db, &run.id, &usage.slot)? {
            if let Some(holder) = store::lock_holder(db, &slot.workspace_id)? {
                if holder != nr.id {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

fn take_locks_and_snapshot(db: &Connection, host: &dyn Host, run: &Run, node: &NodeDef, nr: &NodeRun) -> Result<(), String> {
    let mut before = Map::new();
    for usage in &node.workspaces {
        if usage.access != WorkspaceAccess::Write {
            continue;
        }
        let Some(slot) = slot_workspace(db, &run.id, &usage.slot)? else { continue };
        if !usage.fork {
            store::take_lock(db, &slot.workspace_id, &nr.id)?;
        }
        before.insert(usage.slot.clone(), json!(host.git_head(&slot.path)));
    }
    if !before.is_empty() {
        store::set_node_field(db, &nr.id, "workspace_before_json", Some(&Value::Object(before).to_string()))?;
    }
    Ok(())
}

fn start_node(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, graph: &Graph<'_>, node: &NodeDef, nr: &NodeRun) -> Result<Acts, String> {
    let inputs = nr.inputs.clone();
    // fork：先建临时 worktree
    let mut nr = nr.clone();
    if let Some(usage) = node.workspaces.iter().find(|usage| usage.fork && usage.access == WorkspaceAccess::Write) {
        if nr.fork_workspace_id.is_none() {
            let slot = slot_workspace(db, &run.id, &usage.slot)?
                .ok_or_else(|| tr!("引用了未声明的工作区槽位 {slot}", slot = usage.slot))?;
            let branch = format!(
                "{}-{}-{}",
                slot.branch.clone().unwrap_or_else(|| format!("wf/{}", run.short_id())),
                node.id.replace('_', "-"),
                nr.iteration
            );
            match host.create_fork(db, &ForkRequest { run_id: &run.id, overview_session_id: &run.overview_session_id, source_workspace_id: &slot.workspace_id, branch }) {
                Ok(prepared) => {
                    store::set_node_field(db, &nr.id, "fork_workspace_id", Some(&prepared.workspace_id))?;
                    nr.fork_workspace_id = Some(prepared.workspace_id);
                }
                Err(error) => return fail_node(db, host, run, def, &nr, "workspace_prepare_failed", &error, false),
            }
        }
    }
    let context = node_context(db, run, def, graph, node, &nr, &inputs)?;
    take_locks_and_snapshot(db, host, run, node, &nr)?;
    match node.kind {
        NodeKind::Trigger => {
            let content = run.inputs.to_string();
            let next = match single_target(def, &node.id) {
                Ok(next) => next,
                Err(error) => return fail_node(db, host, run, def, &nr, "route", &error, false),
            };
            let next = next.into_iter().collect::<Vec<_>>();
            succeed_node(db, host, run, def, node, &nr, &content, json!({}), &next)
        }
        NodeKind::Output => {
            let current = store::current_node_runs(db, &run.id)?;
            let content = chosen_predecessors(def, node, &current)
                .into_iter()
                .filter(|item| !item.content.trim().is_empty())
                .map(|item| format!("## {}\n{}", step_title(def, &item.node_id), item.content.trim()))
                .collect::<Vec<_>>()
                .join("\n\n");
            store::set_run_outputs(db, &run.id, &json!({ "content": content }))?;
            succeed_node(db, host, run, def, node, &nr, &content, json!({}), &[])
        }
        NodeKind::Agent => start_agent(db, host, run, def, node, &nr, &context),
    }
}

// ---------- agent ----------

fn start_agent(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, node: &NodeDef, nr: &NodeRun, context: &Value) -> Result<Acts, String> {
    let now = host.now();
    let cfg: AgentConfig = config(node)?;
    let prompt = cfg.prompt.clone();
    let session_id = format!("{WF_NODE_PREFIX}{}", nr.id);
    let target_type = if cfg.target.target_type == "squad" { "squad" } else { "agent" };
    let workflow = store::workflow(db, &run.workflow_id)?;
    let title = format!("{} · {}", workflow.name, if node.title.is_empty() { &node.id } else { &node.title });

    // 工作区：只挂节点声明的槽位
    let mut workspace_rows: Vec<(String, String)> = Vec::new();
    let mut project_ids: Vec<String> = Vec::new();
    for usage in &node.workspaces {
        let Some(slot) = slot_workspace(db, &run.id, &usage.slot)? else { continue };
        let workspace_id = if usage.fork {
            nr.fork_workspace_id.clone().unwrap_or(slot.workspace_id.clone())
        } else {
            slot.workspace_id.clone()
        };
        if !project_ids.contains(&slot.project_id) {
            project_ids.push(slot.project_id.clone());
        }
        workspace_rows.push((workspace_id, slot.workspace_id.clone()));
    }
    db.execute(
        "INSERT INTO chat_sessions (id, project_id, title, target_type, target_id, created_at, updated_at, default_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET title=excluded.title, target_type=excluded.target_type,
           target_id=excluded.target_id, updated_at=excluded.updated_at",
        params![session_id, project_ids.first(), title, target_type, cfg.target.id, now, cfg.mode],
    )
    .map_err(|error| error.to_string())?;
    crate::session::replace_linked_projects(db, &session_id, &project_ids)?;
    db.execute("DELETE FROM session_workspaces WHERE session_id=?1", [&session_id])
        .map_err(|error| error.to_string())?;
    for (workspace_id, source_id) in &workspace_rows {
        db.execute(
            "INSERT OR IGNORE INTO session_workspaces (session_id, workspace_id, source_workspace_id) VALUES (?1, ?2, ?3)",
            params![session_id, workspace_id, source_id],
        )
        .map_err(|error| error.to_string())?;
    }

    let current = store::current_node_runs(db, &run.id)?;
    let graph = Graph::new(def);
    let steps = context_steps(def, &graph, node, &current, &cfg.context);
    let predecessor = chosen_predecessor(def, node, &current);
    let loop_note = if nr.iteration > 0 {
        predecessor.map(|item| {
            let reason = item.metadata.get("reason").or_else(|| item.metadata.get("comment")).and_then(Value::as_str).unwrap_or(item.content.as_str());
            tr!("第 {round} 轮。{reason}", round = nr.iteration + 1, reason = reason)
        })
    } else {
        None
    };
    let allowed: Vec<(String, String)> = def.outgoing(&node.id).into_iter().map(|edge| (edge.target.clone(), step_title(def, &edge.target))).collect();
    let allowed_ref: Vec<(&str, &str)> = allowed.iter().map(|(id, title)| (id.as_str(), title.as_str())).collect();
    let mut workspace_lines = Vec::new();
    for usage in &node.workspaces {
        let ws = &context["ws"][&usage.slot];
        workspace_lines.push(super::brief::workspace_line(
            &usage.slot,
            usage.access,
            ws["path"].as_str().unwrap_or(""),
            ws["branch"].as_str().unwrap_or(""),
        ));
    }
    let brief = super::brief::agent_brief(&super::brief::BriefInput {
        workflow_name: &workflow.name,
        node,
        iteration: nr.iteration,
        attempt: nr.attempt,
        task: &prompt,
        run_inputs: &run.inputs,
        steps: &steps,
        allowed_next: &allowed_ref,
        route: &cfg.route,
        loop_note,
        allow_tasks: cfg.allow_task_commands,
        workspace_lines: &workspace_lines,
        human_confirm: cfg.human_confirm,
    });
    let mut brief = brief;
    if let Some(dir) = run.extra.get("artifactDir").and_then(Value::as_str) {
        brief.push_str("\n## ");
        brief.push_str(&tr!("这次运行的文件目录"));
        brief.push('\n');
        brief.push_str(dir);
        brief.push('\n');
        brief.push_str(&tr!("要留下的文件可以写在这个目录。就算绑定了需求，也可以写在这里，由你决定。"));
        brief.push('\n');
    }
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at, requested_mode) VALUES (?1, ?2, 'user', ?3, ?4, ?5)",
        params![uuid::Uuid::new_v4().to_string(), session_id, brief, now, cfg.mode],
    )
    .map_err(|error| error.to_string())?;
    let mode = if cfg.mode == "plan" { crate::models::RunMode::Plan } else { crate::models::RunMode::Auto };
    if let Err(error) = crate::commands::dispatch(db, &session_id, None, &brief, &now, &[], mode) {
        return fail_node(db, host, run, def, nr, "agent_failed", &error, true);
    }
    store::set_node_field(db, &nr.id, "session_id", Some(&session_id))?;
    store::set_node_status(db, &nr.id, "running", &now)?;
    store::event(db, &run.id, Some(&nr.id), "node_status", &tr!("{node} 开始运行", node = display_name(node)), json!({ "sessionId": session_id }), &now)?;
    Ok(vec![node_changed(nr, "running"), Action::DispatchAgents])
}

fn display_name(node: &NodeDef) -> String {
    if node.title.trim().is_empty() {
        node.id.clone()
    } else {
        node.title.clone()
    }
}

fn asks_confirmation(text: &str) -> bool {
    if text.contains("```multara-questions") || text.contains("~~~multara-questions") {
        return true;
    }
    let ui = text.contains("```multara-ui") || text.contains("~~~multara-ui");
    ui && (text.contains("\"type\":\"form\"") || text.contains("\"type\": \"form\"") || text.contains("\"type\":\"question\"") || text.contains("\"type\": \"question\""))
}

/// 助手提过确认，而之后没有一次界面提交。普通追问不算确认。
fn confirmation_outstanding(db: &Connection, session_id: &str, started: &str) -> Result<bool, String> {
    let mut statement = db
        .prepare(
            "SELECT role, content, source_type FROM messages
             WHERE session_id=?1 AND created_at >= ?2 AND role IN ('user','assistant')
             ORDER BY created_at, rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![session_id, started], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut asked = false;
    let mut answered = false;
    for (role, content, source) in rows {
        if role == "assistant" && asks_confirmation(&content) {
            asked = true;
            answered = false;
        } else if asked && role == "user" && source.as_deref() == Some("genui") {
            answered = true;
        }
    }
    Ok(asked && !answered)
}

fn record_artifact_files(db: &Connection, run: &Run, nr: &NodeRun) {
    let Some(dir) = run.extra.get("artifactDir").and_then(Value::as_str) else { return };
    let files = collect_files(std::path::Path::new(dir), 200);
    let mut meta = nr.metadata.as_object().cloned().unwrap_or_default();
    meta.insert("files".into(), json!(files));
    let _ = store::set_node_field(db, &nr.id, "metadata_json", Some(&Value::Object(meta).to_string()));
}

fn collect_files(root: &std::path::Path, limit: usize) -> Vec<String> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>, limit: usize) {
        if out.len() >= limit {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            if out.len() >= limit {
                return;
            }
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out, limit);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out, limit);
    out
}

fn active_session(db: &Connection, session_id: &str) -> Result<bool, String> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE session_id=?1 AND status IN ('queued','running'))",
        [session_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

/// agent 会话里没有在跑的 run 了：判断节点是完成、追问还是失败
pub fn on_session_idle(db: &Connection, host: &dyn Host, session_id: &str) -> Result<Acts, String> {
    let Some(nr) = store::node_run_by_session(db, session_id)? else {
        return Ok(Vec::new());
    };
    if nr.status == "succeeded" {
        return Ok(Vec::new());
    }
    if !matches!(nr.status.as_str(), "running" | "waiting_human") {
        return Ok(Vec::new());
    }
    let active = active_session(db, session_id)?;
    if active {
        if nr.status == "waiting_human" {
            store::set_node_status(db, &nr.id, "running", &host.now())?;
            return Ok(vec![node_changed(&nr, "running")]);
        }
        return Ok(Vec::new());
    }
    let Loaded { run, def } = load(db, &nr.run_id)?;
    if run.is_terminal() {
        return Ok(Vec::new());
    }
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    let now = host.now();
    if crate::plan::latest_awaiting_plan(db, session_id)?.is_some() {
        if nr.status != "waiting_human" {
            store::set_node_status(db, &nr.id, "waiting_human", &now)?;
            let mut acts = vec![node_changed(&nr, "waiting_human")];
            acts.extend(update_run_status(db, host, &run.id)?);
            return Ok(acts);
        }
        return Ok(Vec::new());
    }
    if let Some(reason) = nr.state.get("declaredFailure").and_then(Value::as_str) {
        return fail_node(db, host, &run, &def, &nr, "agent_declared_failure", reason, false);
    }
    let cfg: AgentConfig = config(&node)?;
    let started = nr.started_at.clone().unwrap_or_default();
    let final_text: Option<String> = db
        .query_row(
            "SELECT content FROM messages WHERE session_id=?1 AND role='assistant' AND created_at >= ?2
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            params![session_id, started],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let text = final_text.unwrap_or_default();
    record_artifact_files(db, &run, &nr);
    // 人还没确认时，追问只是继续这段对话，不进入下一步。
    if cfg.human_confirm || confirmation_outstanding(db, session_id, &started)? {
        store::set_node_field(db, &nr.id, "content", Some(&text))?;
        if nr.status != "waiting_human" {
            store::set_node_status(db, &nr.id, "waiting_human", &now)?;
        }
        let mut acts = vec![node_changed(&nr, "waiting_human")];
        acts.extend(update_run_status(db, host, &run.id)?);
        return Ok(acts);
    }
    let edges = def.outgoing(&node.id);
    if edges.len() <= 1 || cfg.route == "all" {
        let next: Vec<String> = edges.iter().map(|edge| edge.target.clone()).collect();
        let mut acts = succeed_node(db, host, &run, &def, &node, &nr, &text, json!({ "next": next }), &next)?;
        acts.extend(advance(db, host, &run.id)?);
        return Ok(acts);
    }
    if cfg.route == "human" {
        let choices: Vec<Value> = edges
            .iter()
            .map(|edge| json!({ "nodeId": edge.target, "title": step_title(&def, &edge.target) }))
            .collect();
        store::set_node_field(db, &nr.id, "content", Some(&text))?;
        store::set_node_field(db, &nr.id, "metadata_json", Some(&json!({ "choices": choices }).to_string()))?;
        store::set_node_status(db, &nr.id, "waiting_human", &now)?;
        let mut acts = vec![node_changed(&nr, "waiting_human")];
        acts.extend(update_run_status(db, host, &run.id)?);
        return Ok(acts);
    }
    let next = nr.state.get("next").and_then(Value::as_str).map(str::to_string);
    let Some(next) = next.filter(|id| edges.iter().any(|edge| &edge.target == id)) else {
        return fail_node(db, host, &run, &def, &nr, "route", &tr!("这一步有多条出路，但没有提交合法的下一步"), false);
    };
    let reason = nr.state.get("reason").cloned().unwrap_or(Value::Null);
    let next_ids = vec![next.clone()];
    let mut acts = succeed_node(db, host, &run, &def, &node, &nr, &text, json!({ "next": next, "reason": reason }), &next_ids)?;
    acts.extend(advance(db, host, &run.id)?);
    Ok(acts)
}

/// 智能体在多条出路里提交下一步：`multara node next --id <节点> --reason "..."`
pub fn submit_next(db: &Connection, node_run_id: &str, next: &str, reason: &str) -> Result<(), String> {
    let nr = store::node_run(db, node_run_id)?;
    let Loaded { def, .. } = load(db, &nr.run_id)?;
    if let Some(node) = def.node(&nr.node_id) {
        if config::<AgentConfig>(node).is_ok_and(|cfg| cfg.human_confirm) {
            return Err(tr!("这一步需要人工确认，不能自己进入下一步"));
        }
    }
    if let (Some(session), Some(started)) = (&nr.session_id, nr.started_at.as_deref()) {
        if confirmation_outstanding(db, session, started)? {
            return Err(tr!("这一步需要人工确认，不能自己进入下一步"));
        }
    }
    if nr.status != "running" {
        return Err(tr!("节点已经结束，不能再选择下一步"));
    }
    let allowed = def.outgoing(&nr.node_id);
    if !allowed.iter().any(|edge| edge.target == next) {
        let names: Vec<&str> = allowed.iter().map(|edge| edge.target.as_str()).collect();
        return Err(tr!("下一步只能是：{names}", names = names.join(", ")));
    }
    let mut state = nr.state.as_object().cloned().unwrap_or_default();
    state.insert("next".into(), json!(next));
    state.insert("reason".into(), json!(reason));
    store::set_node_field(db, node_run_id, "state_json", Some(&Value::Object(state).to_string()))
}

/// 多条出路暂停后，由人选定下一步。
pub fn choose_route(db: &Connection, host: &dyn Host, node_run_id: &str, target: &str) -> Result<Acts, String> {
    let nr = store::node_run(db, node_run_id)?;
    if nr.status != "waiting_human" || nr.kind != "agent" {
        return Err(tr!("这一步现在不能选下一步"));
    }
    let Loaded { run, def } = load(db, &nr.run_id)?;
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    if !def.outgoing(&node.id).iter().any(|edge| edge.target == target) {
        return Err(tr!("这一步没有连到 {target}", target = target));
    }
    let next_ids = vec![target.to_string()];
    let mut acts = succeed_node(db, host, &run, &def, &node, &nr, &nr.content, json!({ "next": target, "by": "human" }), &next_ids)?;
    acts.extend(advance(db, host, &run.id)?);
    Ok(acts)
}

pub fn declare_failure(db: &Connection, node_run_id: &str, reason: &str) -> Result<(), String> {
    let nr = store::node_run(db, node_run_id)?;
    let mut state = nr.state.as_object().cloned().unwrap_or_default();
    state.insert("declaredFailure".into(), json!(reason));
    store::set_node_field(db, node_run_id, "state_json", Some(&Value::Object(state).to_string()))
}

pub fn handle_overview_message(_db: &Connection, _host: &dyn Host, _session_id: &str, _text: &str) -> Result<Option<Acts>, String> {
    Ok(None)
}

pub fn complete_job(db: &Connection, host: &dyn Host, node_run_id: &str, _result: JobResult) -> Result<Acts, String> {
    let nr = store::node_run(db, node_run_id)?;
    let Loaded { run, def } = load(db, &nr.run_id)?;
    fail_node(db, host, &run, &def, &nr, "unsupported", &tr!("这个节点类型已经删除"), false)
}

// ---------- 结束节点 ----------

#[allow(clippy::too_many_arguments)]
fn succeed_node(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, node: &NodeDef, nr: &NodeRun, content: &str, metadata: Value, next_ids: &[String]) -> Result<Acts, String> {
    let now = host.now();
    let encoded = encode_next(next_ids);
    store::set_node_field(db, &nr.id, "content", Some(content))?;
    store::set_node_field(db, &nr.id, "metadata_json", Some(&metadata.to_string()))?;
    store::set_node_field(db, &nr.id, "next_node_id", encoded.as_deref())?;
    let mut after = Map::new();
    for usage in node.workspaces.iter().filter(|usage| usage.access == WorkspaceAccess::Write) {
        let Some(slot) = slot_workspace(db, &run.id, &usage.slot)? else { continue };
        let slot_def = def.slot(&usage.slot);
        let path = if usage.fork {
            nr.fork_workspace_id.as_deref().and_then(|id| store::workspace_path(db, id).ok().flatten()).map(|(path, _, _)| path).unwrap_or(slot.path.clone())
        } else {
            slot.path.clone()
        };
        if slot_def.map(|slot| slot.auto_commit).unwrap_or(true) {
            let message = format!(
                "wf({}#{}): {}",
                store::workflow(db, &run.workflow_id).map(|wf| wf.slug).unwrap_or_default(),
                run.short_id(),
                display_name(node)
            );
            match host.git_commit_all(&path, &message) {
                Ok(Some(commit)) => {
                    store::event(db, &run.id, Some(&nr.id), "commit", &tr!("自动提交 {commit}", commit = commit.chars().take(8).collect::<String>()), json!({ "slot": usage.slot, "commit": commit }), &now)?;
                }
                Ok(None) => {}
                Err(error) => {
                    store::event(db, &run.id, Some(&nr.id), "error", &tr!("自动提交失败：{error}", error = error), json!({ "slot": usage.slot }), &now)?;
                }
            }
        }
        after.insert(usage.slot.clone(), json!(host.git_head(&path)));
        if !usage.fork {
            store::set_slot_head(db, &run.id, &usage.slot, host.git_head(&path).as_deref())?;
        }
    }
    if !after.is_empty() {
        store::set_node_field(db, &nr.id, "workspace_after_json", Some(&Value::Object(after).to_string()))?;
    }
    store::release_locks(db, &nr.id)?;

    let mut acts = Vec::new();
    for target in next_ids {
        if let Some(edge) = def.outgoing(&node.id).into_iter().find(|edge| &edge.target == target && edge.is_back()) {
            let count = store::loop_count(db, &run.id, &edge.id)?;
            let limit = edge.max_iterations.unwrap_or(1);
            if count >= limit {
                store::event(db, &run.id, Some(&nr.id), "loop", &tr!("循环已达上限 {limit} 轮", limit = limit), json!({ "edgeId": edge.id }), &now)?;
                return fail_node(db, host, run, def, nr, "loop_exhausted", &tr!("循环已达上限 {limit} 轮", limit = limit), false);
            }
            store::bump_loop(db, &run.id, &edge.id)?;
            acts.extend(loop_reset(db, host, run, def, edge)?);
        }
    }
    store::set_node_status(db, &nr.id, "succeeded", &now)?;
    store::event(db, &run.id, Some(&nr.id), "node_status", &tr!("{node} 完成", node = display_name(node)), json!({ "next": next_ids }), &now)?;
    acts.push(node_changed(nr, "succeeded"));
    Ok(acts)
}

fn loop_reset(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, edge: &EdgeDef) -> Result<Acts, String> {
    let now = host.now();
    let graph = Graph::new(def);
    let current = store::current_node_runs(db, &run.id)?;
    let mut targets = graph.descendants(&edge.target);
    targets.insert(edge.target.clone());
    let mut acts = Vec::new();
    for node_id in &targets {
        let Some(old) = current.get(node_id) else { continue };
        if old.is_active() && old.node_id != edge.source {
            acts.extend(stop_node(db, host, old, "cancelled")?);
        }
        let kind = def.node(node_id).map(|node| node.kind.as_str()).unwrap_or("agent");
        let id = store::insert_node_run(db, store::NewNodeRun {
            run_id: &run.id, node_id, kind, iteration: old.iteration + 1, attempt: 1, carried: None, retry_at: None, now: &now,
        })?;
        acts.push(Action::NodeChanged { run_id: run.id.clone(), node_run_id: id, node_id: node_id.clone(), status: "pending".into() });
    }
    store::event(db, &run.id, None, "loop", &tr!("回到 {node} 开始新一轮", node = edge.target), json!({ "edgeId": edge.id }), &now)?;
    Ok(acts)
}

fn stop_node(db: &Connection, host: &dyn Host, nr: &NodeRun, status: &str) -> Result<Acts, String> {
    let now = host.now();
    let mut acts = Vec::new();
    match nr.kind.as_str() {
        "agent" => {
            if let Some(session) = &nr.session_id {
                acts.push(Action::StopSession(session.clone()));
            }
        }
        "script" | "http" | "git" => acts.push(Action::KillJob(nr.id.clone())),
        "subflow" | "map" => {
            for child in store::child_runs(db, &nr.id)? {
                if !child.is_terminal() {
                    acts.extend(cancel_run(db, host, &child.id, "parent_cancelled")?);
                }
            }
        }
        _ => {}
    }
    store::release_locks(db, &nr.id)?;
    store::set_node_status(db, &nr.id, status, &now)?;
    acts.push(node_changed(nr, status));
    Ok(acts)
}

#[allow(clippy::too_many_arguments)]
fn fail_node(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, nr: &NodeRun, code: &str, message: &str, _retryable: bool) -> Result<Acts, String> {
    let now = host.now();
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    let message = super::runtime_redact(message, &host.secrets());
    store::set_node_field(db, &nr.id, "error_code", Some(code))?;
    store::set_node_field(db, &nr.id, "error", Some(&message))?;
    store::release_locks(db, &nr.id)?;
    store::set_node_status(db, &nr.id, "failed", &now)?;
    store::event(db, &run.id, Some(&nr.id), "error", &format!("{}：{message}", display_name(&node)), json!({ "code": code }), &now)?;
    let mut acts = vec![node_changed(nr, "failed")];
    acts.extend(finish_run(db, host, &run.id, "failed", Some(code), Some(&format!("{}：{message}", display_name(&node))))?);
    Ok(acts)
}

// ---------- 运行状态 ----------

fn update_run_status(db: &Connection, host: &dyn Host, run_id: &str) -> Result<Acts, String> {
    let run = store::run(db, run_id)?;
    if run.is_terminal() || run.status == "queued" {
        return Ok(Vec::new());
    }
    let current = store::current_node_runs(db, run_id)?;
    let all_terminal = current.values().all(NodeRun::is_terminal);
    if all_terminal {
        return finish_run(db, host, run_id, "succeeded", None, None);
    }
    let busy = current.values().any(|item| matches!(item.status.as_str(), "running" | "ready" | "waiting_children"));
    let waiting = current.values().any(|item| item.status == "waiting_human");
    let pending_retry = current.values().any(|item| item.status == "pending" && item.retry_at.is_some());
    let status = if waiting && !busy { "waiting" } else { "running" };
    let _ = pending_retry;
    if status != run.status {
        store::set_run_status(db, run_id, status, None, None, &host.now())?;
        return Ok(vec![Action::RunChanged { run_id: run.id.clone(), workflow_id: run.workflow_id.clone(), status: status.into() }]);
    }
    Ok(Vec::new())
}

fn finish_run(db: &Connection, host: &dyn Host, run_id: &str, status: &str, code: Option<&str>, error: Option<&str>) -> Result<Acts, String> {
    let run = store::run(db, run_id)?;
    if run.is_terminal() {
        return Ok(Vec::new());
    }
    let now = host.now();
    let mut acts = Vec::new();
    // 收尾：停掉还在进行的节点
    for item in store::current_node_runs(db, run_id)?.into_values() {
        if item.is_active() || item.status == "ready" || item.status == "pending" {
            let target = if item.is_active() || item.status == "ready" { "cancelled" } else { "skipped" };
            acts.extend(stop_node(db, host, &item, target)?);
        }
    }
    store::set_run_status(db, run_id, status, code, error, &now)?;
    store::event(db, run_id, None, "info", &match status {
        "succeeded" => tr!("运行成功"),
        "cancelled" => tr!("运行已取消"),
        _ => tr!("运行失败"),
    }, json!({ "code": code }), &now)?;
    acts.push(Action::RunChanged { run_id: run.id.clone(), workflow_id: run.workflow_id.clone(), status: status.into() });
    acts.extend(after_terminal(db, host, run_id)?);
    Ok(acts)
}

/// 运行进入终态后：总览会话、任务卡、收件箱、父节点、排队中的下一次运行
fn after_terminal(db: &Connection, host: &dyn Host, run_id: &str) -> Result<Acts, String> {
    let run = store::run(db, run_id)?;
    let now = host.now();
    let mut acts = Vec::new();
    if run.parent_node_run_id.is_none() {
        let workflow = store::workflow(db, &run.workflow_id)?;
        let (text, task_status, inbox_status) = match run.status.as_str() {
            "succeeded" => (tr!("工作流「{name}」运行成功。", name = workflow.name), "done", "completed"),
            "cancelled" => (tr!("工作流「{name}」已取消。", name = workflow.name), "cancelled", "stopped"),
            _ => (
                tr!("工作流「{name}」运行失败：{error}", name = workflow.name, error = run.error.clone().unwrap_or_default()),
                "blocked",
                "failed",
            ),
        };
        let exists: bool = db
            .query_row("SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id=?1)", [&run.overview_session_id], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if exists {
            let mut text = text;
            if run.status == "succeeded" {
                if let Some(map) = run.outputs.as_object().filter(|map| !map.is_empty()) {
                    text.push_str("\n\n");
                    for (key, value) in map {
                        let shown = match value {
                            Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                        text.push_str(&format!("- **{key}**：{shown}\n"));
                    }
                }
            }
            append_overview(db, &run.overview_session_id, &text, &[], &now)?;
            let _ = crate::ops::inbox::upsert(db, crate::models::InboxInput {
                session_id: run.overview_session_id.clone(),
                project_id: None,
                task_id: run.task_id.clone(),
                title: format!("{} · {}", tr!("工作流"), workflow.name),
                preview: text.chars().take(200).collect(),
                status: inbox_status.into(),
            });
            // 「开始运行」已经在过程里出现，未读只留刚写入的结束结果。
            let _ = db.execute(
                "UPDATE inbox_items
                 SET read_message_id = (
                   SELECT id FROM messages
                   WHERE session_id = ?1
                   ORDER BY created_at DESC, rowid DESC
                   LIMIT 1 OFFSET 1
                 )
                 WHERE session_id = ?1 AND read_message_id IS NULL",
                [&run.overview_session_id],
            );
            acts.push(Action::SessionChanged(run.overview_session_id.clone()));
        }
        if let Some(task_id) = &run.task_id {
            let _ = db.execute("UPDATE tasks SET status=?1, updated_at=?2 WHERE id=?3", params![task_status, now, task_id]);
        }
        acts.push(Action::RunFinished {
            run_id: run.id.clone(),
            status: run.status.clone(),
            trigger_source: run.trigger_source.clone(),
            trigger_ref: run.trigger_ref.clone(),
            error: run.error.clone(),
        });
        // 排队的下一次
        let next: Option<String> = db
            .query_row(
                "SELECT id FROM workflow_runs WHERE workflow_id=?1 AND status='queued' AND parent_node_run_id IS NULL
                 ORDER BY created_at LIMIT 1",
                [&run.workflow_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(next) = next {
            acts.extend(begin_run(db, host, &next)?);
        }
    }
    Ok(acts)
}

// ---------- 用户操作 ----------

pub fn cancel_run(db: &Connection, host: &dyn Host, run_id: &str, reason: &str) -> Result<Acts, String> {
    let run = store::run(db, run_id)?;
    if run.is_terminal() {
        return Ok(Vec::new());
    }
    if run.status == "queued" {
        store::set_run_status(db, run_id, "cancelled", Some(reason), None, &host.now())?;
        return Ok(vec![Action::RunChanged { run_id: run.id, workflow_id: run.workflow_id, status: "cancelled".into() }]);
    }
    finish_run(db, host, run_id, "cancelled", Some(reason), None)
}

/// 从某节点重试：该节点与全部后代建新的尝试
pub fn retry_node(db: &Connection, host: &dyn Host, run_id: &str, node_id: &str, reset_workspace: bool) -> Result<Acts, String> {
    let Loaded { run, def } = load(db, run_id)?;
    if run.parent_node_run_id.is_some() {
        return Err(tr!("请在最外层运行里重试"));
    }
    let node = def.node(node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    let graph = Graph::new(&def);
    let current = store::current_node_runs(db, run_id)?;
    let target = current.get(node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    if target.is_active() {
        return Err(tr!("节点还在运行，不能重试"));
    }
    let now = host.now();
    let mut acts = Vec::new();
    if reset_workspace {
        if let Some(before) = target.workspace_before.as_object() {
            for (slot_key, commit) in before {
                let (Some(commit), Some(slot)) = (commit.as_str(), slot_workspace(db, run_id, slot_key)?) else { continue };
                host.git_reset_hard(&slot.path, commit)?;
                store::set_slot_head(db, run_id, slot_key, Some(commit))?;
                store::event(db, run_id, Some(&target.id), "commit", &tr!("工作区 {slot} 回到 {commit}", slot = slot_key, commit = commit.chars().take(8).collect::<String>()), json!({}), &now)?;
            }
        }
    }
    let mut nodes = graph.descendants(node_id);
    nodes.insert(node_id.to_string());
    for id in &nodes {
        let Some(old) = current.get(id) else { continue };
        if old.is_active() {
            acts.extend(stop_node(db, host, old, "cancelled")?);
        }
        let kind = def.node(id).map(|node| node.kind.as_str()).unwrap_or("transform");
        let new_id = store::insert_node_run(db, store::NewNodeRun {
            run_id, node_id: id, kind, iteration: old.iteration, attempt: old.attempt + 1,
            carried: if old.carried.is_null() { None } else { Some(&old.carried) }, retry_at: None, now: &now,
        })?;
        acts.push(Action::NodeChanged { run_id: run_id.to_string(), node_run_id: new_id, node_id: id.clone(), status: "pending".into() });
    }
    store::event(db, run_id, None, "retry", &tr!("从 {node} 重试", node = display_name(&node)), json!({}), &now)?;
    if run.is_terminal() {
        db.execute(
            "UPDATE workflow_runs SET status='running', error=NULL, error_code=NULL, completed_at=NULL WHERE id=?1",
            [run_id],
        )
        .map_err(|error| error.to_string())?;
        if let Some(task_id) = &run.task_id {
            let _ = db.execute("UPDATE tasks SET status='in_progress', updated_at=?1 WHERE id=?2", params![now, task_id]);
        }
        acts.push(Action::RunChanged { run_id: run.id.clone(), workflow_id: run.workflow_id.clone(), status: "running".into() });
    }
    acts.extend(advance(db, host, run_id)?);
    Ok(acts)
}

/// 失败节点手动标记成功并填输出
pub fn skip_with_outputs(db: &Connection, host: &dyn Host, run_id: &str, node_id: &str, outputs: Value) -> Result<Acts, String> {
    let Loaded { run, def } = load(db, run_id)?;
    let node = def.node(node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    let current = store::current_node_runs(db, run_id)?;
    let old = current.get(node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    if old.status != "failed" {
        return Err(tr!("只有失败的节点可以手动跳过"));
    }
    let now = host.now();
    let id = store::insert_node_run(db, store::NewNodeRun {
        run_id, node_id, kind: &old.kind, iteration: old.iteration, attempt: old.attempt + 1, carried: None, retry_at: None, now: &now,
    })?;
    let nr = store::node_run(db, &id)?;
    if run.is_terminal() {
        db.execute("UPDATE workflow_runs SET status='running', error=NULL, error_code=NULL, completed_at=NULL WHERE id=?1", [run_id])
            .map_err(|error| error.to_string())?;
    }
    let run = store::run(db, run_id)?;
    let content = outputs.as_str().unwrap_or("").to_string();
    let next = single_target(&def, &node.id).unwrap_or(None).into_iter().collect::<Vec<_>>();
    let mut acts = succeed_node(db, host, &run, &def, &node, &nr, &content, json!({ "by": "user" }), &next)?;
    acts.extend(advance(db, host, run_id)?);
    Ok(acts)
}

// ---------- 定时与恢复 ----------

/// 每 30 秒：超时、到期重试、运行超时
pub fn tick(db: &Connection, host: &dyn Host) -> Result<Acts, String> {
    let now = host.now();
    let mut acts = Vec::new();
    let mut statement = db
        .prepare(
            "SELECT n.id FROM workflow_node_runs n JOIN workflow_runs r ON r.id=n.run_id
             WHERE r.status IN ('running','waiting') AND n.status IN ('running','waiting_human')
               AND n.deadline_at IS NOT NULL",
        )
        .map_err(|error| error.to_string())?;
    let candidates = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    for id in candidates {
        let nr = store::node_run(db, &id)?;
        let Some(deadline) = &nr.deadline_at else { continue };
        if !before(deadline, &now) {
            continue;
        }
        let Loaded { run, def } = load(db, &nr.run_id)?;
        let mut stop = stop_node(db, host, &nr, "running")?;
        stop.retain(|action| !matches!(action, Action::NodeChanged { .. }));
        acts.extend(stop);
        acts.extend(fail_node(db, host, &run, &def, &nr, "timeout", &tr!("节点运行超时"), true)?);
        acts.extend(advance(db, host, &nr.run_id)?);
    }
    acts.extend(start_queued(db, host)?);
    // 到期重试 / 运行超时
    for run_id in store::active_run_ids(db)? {
        let Loaded { run, def } = load(db, &run_id)?;
        if let (Some(minutes), Some(started)) = (def.settings.run_timeout_minutes, &run.started_at) {
            if before(&plus_seconds(started, minutes as i64 * 60), &now) {
                acts.extend(finish_run(db, host, &run_id, "failed", Some("timeout"), Some(&tr!("运行超时")))?);
                continue;
            }
        }
        let due = store::current_node_runs(db, &run_id)?
            .values()
            .any(|item| item.status == "pending" && item.retry_at.as_deref().is_some_and(|at| before(at, &now)));
        if due {
            acts.extend(advance(db, host, &run_id)?);
        }
    }
    Ok(acts)
}

/// 应用启动：后台节点在上次退出时中断了，标失败再按重试策略处理；agent 节点按会话现状判定
pub fn recover(db: &Connection, host: &dyn Host) -> Result<Acts, String> {
    let mut acts = Vec::new();
    for run_id in store::active_run_ids(db)? {
        for nr in store::current_node_runs(db, &run_id)?.into_values() {
            match (nr.kind.as_str(), nr.status.as_str()) {
                ("agent", "running" | "waiting_human") => {
                    if let Some(session) = &nr.session_id {
                        acts.extend(on_session_idle(db, host, session)?);
                    }
                }
                _ => {}
            }
        }
        if !store::run(db, &run_id)?.is_terminal() {
            acts.extend(advance(db, host, &run_id)?);
        }
    }
    Ok(acts)
}

pub fn is_workflow_session(session_id: &str) -> bool {
    session_id.starts_with(WF_NODE_PREFIX) || session_id.starts_with(WF_RUN_PREFIX)
}


/// 节点会话且目标是单个智能体：不解析 @ 派活
pub fn is_plain_agent_node_session(db: &Connection, session_id: &str) -> bool {
    if !session_id.starts_with(WF_NODE_PREFIX) {
        return false;
    }
    db.query_row("SELECT target_type FROM chat_sessions WHERE id=?1", [session_id], |row| row.get::<_, String>(0))
        .map(|target| target == "agent")
        .unwrap_or(true)
}

