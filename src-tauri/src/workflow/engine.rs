//! 执行引擎（技术方案第 3 节）。所有入口都是「拿着 DB 连接做完状态变更，返回需要在锁外执行的动作」，
//! 这样同一套逻辑既能在应用里跑（`runtime.rs` 执行动作），也能在单测里用假 Host 驱动。

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::graph::Graph;
use super::model::*;
use super::store::{self, NodeRun, Run};
use super::template::{self, Place, ShellKind};
use super::values;

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
    for port in &def.inputs {
        let value = given
            .get(&port.name)
            .cloned()
            .filter(|value| !value.is_null())
            .or_else(|| port.default.clone());
        match value {
            Some(value) => {
                let value = values::coerce(value, port)?;
                inputs.insert(port.name.clone(), value);
            }
            None if port.required => {
                return Err(tr!("缺少必填入参 {name}", name = port.label.clone().unwrap_or_else(|| port.name.clone())));
            }
            None => {
                inputs.insert(port.name.clone(), Value::Null);
            }
        }
    }
    let mut extra = request.extra.as_object().cloned().unwrap_or_default();
    if request.untrusted {
        extra.insert("untrusted".into(), Value::Bool(true));
    }
    let extra = Value::Object(extra);
    let inputs = Value::Object(inputs);

    // 并发策略（子运行不受限）
    let mut status = "running";
    if request.parent_node_run_id.is_none() && def.settings.concurrency != Concurrency::Allow {
        let active: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE workflow_id=?1 AND parent_node_run_id IS NULL
                   AND status IN ('queued','running','waiting'))",
                [request.workflow_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if active {
            status = match def.settings.concurrency {
                Concurrency::SkipIfRunning => "skipped",
                _ => "queued",
            };
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
    let mut acts = Vec::new();
    if status == "running" {
        acts.extend(begin_run(db, host, &run_id)?);
    } else {
        store::event(db, &run_id, None, "info", &tr!("排队等待同一工作流的上一次运行结束"), json!({}), &now)?;
        acts.push(Action::RunChanged {
            run_id: run_id.clone(),
            workflow_id: request.workflow_id.to_string(),
            status: status.to_string(),
        });
    }
    Ok((run_id, acts))
}

/// CLI 等没有应用句柄的地方：只写一条排队的运行，由应用的定时器开始
pub fn enqueue_run(db: &Connection, workflow_id: &str, inputs: Value, source: &str, now: &str) -> Result<String, String> {
    let workflow = store::workflow(db, workflow_id)?;
    let version = workflow.published_version.ok_or_else(|| tr!("工作流还没有发布，请先发布"))?;
    let def = store::definition(db, workflow_id, version)?;
    let given = inputs.as_object().cloned().unwrap_or_default();
    let mut checked = Map::new();
    for port in &def.inputs {
        let value = given.get(&port.name).cloned().filter(|value| !value.is_null()).or_else(|| port.default.clone());
        match value {
            Some(value) => {
                checked.insert(port.name.clone(), values::coerce(value, port)?);
            }
            None if port.required => return Err(tr!("缺少必填入参 {name}", name = port.name)),
            None => {
                checked.insert(port.name.clone(), Value::Null);
            }
        }
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

/// 排队的运行：同一工作流没有进行中的运行（或允许并发）就开始
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
        let allow = store::definition(db, &workflow_id, version)
            .map(|def| def.settings.concurrency == Concurrency::Allow)
            .unwrap_or(false);
        let active: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE workflow_id=?1 AND parent_node_run_id IS NULL AND status IN ('running','waiting'))",
                [&workflow_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if (active || started.contains(&workflow_id)) && !allow {
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
        if node.kind == NodeKind::Note {
            continue;
        }
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
    // 子流程继承父运行的槽位
    if let (Some(inherit), Some(parent_node_run)) = (&slot.inherit, &run.parent_node_run_id) {
        let parent = store::node_run(db, parent_node_run)?;
        let parent_slots = store::run_workspaces(db, &parent.run_id)?;
        let found = parent_slots
            .into_iter()
            .find(|item| &item.slot_key == inherit)
            .ok_or_else(|| tr!("父运行没有工作区槽位 {slot}", slot = inherit))?;
        store::insert_run_workspace(db, &run.id, &slot.key, &found.workspace_id, found.head_commit.as_deref(), false)?;
        return Ok(found.project_id);
    }
    let project_id = template::render(&slot.project, context, Place::Text, None)?;
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err(tr!("工作区槽位 {slot} 没有指定需求", slot = slot.key));
    }
    let branch = match &slot.branch {
        Some(branch) if !branch.trim().is_empty() => Some(template::render(branch, context, Place::Text, None)?),
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
    if run.parent_node_run_id.is_none() {
        if let Ok(task) = crate::ops::task::attach_existing_session(
            db,
            &run.overview_session_id,
            &title,
            &tr!("工作流运行 {id}", id = run.id),
            &owner,
            "in_progress",
            now,
        ) {
            store::set_run_task(db, &run.id, &task.id)?;
        }
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

fn node_context(db: &Connection, run: &Run, def: &WorkflowDef, graph: &Graph<'_>, node: &NodeDef, nr: &NodeRun, inputs: &Value) -> Result<Value, String> {
    let workflow = store::workflow(db, &run.workflow_id)?;
    let mut context = run_context(db, run, def, &workflow.name, &workflow.slug)?;
    let current = store::current_node_runs(db, &run.id)?;
    let mut nodes = Map::new();
    for ancestor in graph.ancestors(&node.id) {
        if let Some(item) = current.get(&ancestor) {
            let outputs = values::outputs(db, &item.id)?;
            nodes.insert(
                ancestor.clone(),
                json!({ "outputs": outputs, "status": item.status, "iteration": item.iteration }),
            );
        }
    }
    // fork 节点的 ws 指向临时 worktree
    if let Some(fork) = &nr.fork_workspace_id {
        if let Some((path, branch, _)) = store::workspace_path(db, fork)? {
            for usage in node.workspaces.iter().filter(|usage| usage.fork) {
                context["ws"][&usage.slot]["path"] = json!(path);
                context["ws"][&usage.slot]["branch"] = json!(branch);
            }
        }
    }
    context["nodes"] = Value::Object(nodes);
    context["inputs"] = inputs.clone();
    context["iteration"] = json!(nr.iteration);
    context["attempt"] = json!(nr.attempt);
    context["node"] = json!({ "id": node.id, "title": node.title });
    Ok(context)
}

// ---------- 连线与就绪 ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EdgeState {
    Undecided,
    Live,
    Dead,
}

fn edge_state(def: &WorkflowDef, edge: &EdgeDef, current: &HashMap<String, NodeRun>) -> EdgeState {
    let Some(source_nr) = current.get(&edge.source) else {
        return EdgeState::Undecided;
    };
    let Some(source) = def.node(&edge.source) else {
        return EdgeState::Dead;
    };
    match source_nr.status.as_str() {
        "skipped" | "cancelled" => EdgeState::Dead,
        "failed" => {
            if source.on_error == OnError::Continue && edge.source_port == ERROR_PORT {
                EdgeState::Live
            } else if source.on_error == OnError::Continue {
                EdgeState::Dead
            } else {
                EdgeState::Undecided
            }
        }
        "succeeded" | "reused" => {
            if edge.source_port == ERROR_PORT {
                return EdgeState::Dead;
            }
            if source.kind.is_branching() && branch_ports(source).contains(&edge.source_port) {
                if source_nr.fired_ports.contains(&edge.source_port) {
                    EdgeState::Live
                } else {
                    EdgeState::Dead
                }
            } else {
                EdgeState::Live
            }
        }
        _ => EdgeState::Undecided,
    }
}

enum Readiness {
    Wait,
    Skip(String),
    Ready,
}

fn readiness(def: &WorkflowDef, graph: &Graph<'_>, node: &NodeDef, nr: &NodeRun, current: &HashMap<String, NodeRun>, now: &str) -> Readiness {
    if let Some(retry_at) = &nr.retry_at {
        if !before(retry_at, now) {
            return Readiness::Wait;
        }
    }
    let incoming = graph.incoming(&node.id);
    if incoming.is_empty() {
        return Readiness::Ready;
    }
    let states: Vec<(&EdgeDef, EdgeState)> = incoming
        .iter()
        .map(|edge| (*edge, edge_state(def, edge, current)))
        .collect();
    let live: Vec<&EdgeDef> = states.iter().filter(|(_, s)| *s == EdgeState::Live).map(|(e, _)| *e).collect();
    let undecided = states.iter().any(|(_, s)| *s == EdgeState::Undecided);
    if node.join == JoinMode::Any {
        if !live.is_empty() {
            return Readiness::Ready;
        }
        if undecided {
            return Readiness::Wait;
        }
        return Readiness::Skip("all_inputs_dead".into());
    }
    if undecided {
        return Readiness::Wait;
    }
    if live.is_empty() {
        return Readiness::Skip("all_inputs_dead".into());
    }
    let carried = nr.carried.as_object();
    for port in node.inputs.iter().filter(|port| port.required) {
        let has_edge = live.iter().any(|edge| edge.target_port == port.name);
        let has_carried = carried.is_some_and(|map| map.contains_key(&port.name));
        if !has_edge && !has_carried && port.default.is_none() {
            return Readiness::Skip(format!("required_input_dead:{}", port.name));
        }
    }
    Readiness::Ready
}

fn resolve_inputs(db: &Connection, def: &WorkflowDef, graph: &Graph<'_>, node: &NodeDef, nr: &NodeRun, current: &HashMap<String, NodeRun>) -> Result<Value, (String, String)> {
    let mut result = Map::new();
    let carried = nr.carried.as_object().cloned().unwrap_or_default();
    let fail = |code: &str, message: String| (code.to_string(), message);
    for port in &node.inputs {
        let mut collected: Vec<Value> = Vec::new();
        for edge in graph.incoming(&node.id).iter().filter(|edge| edge.target_port == port.name) {
            if edge_state(def, edge, current) != EdgeState::Live {
                continue;
            }
            let source_nr = &current[&edge.source];
            let value = if edge.source_port == OUT_PORT {
                Value::Null
            } else {
                values::output(db, &source_nr.id, &edge.source_port)
                    .map_err(|error| fail("internal", error))?
                    .unwrap_or(Value::Null)
            };
            collected.push(value);
        }
        if let Some(value) = carried.get(&port.name) {
            collected.push(value.clone());
        }
        let value = if port.multi {
            if collected.is_empty() {
                port.default.clone().unwrap_or(Value::Null)
            } else {
                Value::Array(collected)
            }
        } else {
            collected
                .into_iter()
                .rev()
                .find(|value| !value.is_null())
                .or_else(|| port.default.clone())
                .unwrap_or(Value::Null)
        };
        if value.is_null() && port.required {
            return Err(fail("missing_input", tr!("必填输入 {port} 没有值", port = port.name)));
        }
        let value = if port.multi {
            match value {
                Value::Array(items) => {
                    let mut out = Vec::new();
                    for item in items {
                        let mut single = port.clone();
                        single.multi = false;
                        out.push(values::coerce(item, &single).map_err(|error| fail("type_mismatch", error))?);
                    }
                    Value::Array(out)
                }
                other => other,
            }
        } else {
            values::coerce(value, port).map_err(|error| fail("type_mismatch", error))?
        };
        result.insert(port.name.clone(), value);
    }
    let _ = def;
    Ok(Value::Object(result))
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
                    match resolve_inputs(db, &def, &graph, node, nr, &current) {
                        Ok(inputs) => {
                            store::set_node_field(db, &nr.id, "inputs_json", Some(&inputs.to_string()))?;
                            store::set_node_status(db, &nr.id, "ready", &now)?;
                            acts.push(node_changed(nr, "ready"));
                        }
                        Err((code, message)) => {
                            acts.extend(fail_node(db, host, &run, &def, nr, &code, &message, false)?);
                        }
                    }
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

fn set_deadline(db: &Connection, node: &NodeDef, nr: &NodeRun, now: &str) -> Result<(), String> {
    let minutes = node.timeout_minutes.or_else(|| node.kind.default_timeout_minutes());
    if let Some(minutes) = minutes.filter(|minutes| *minutes > 0) {
        store::set_node_field(db, &nr.id, "deadline_at", Some(&plus_seconds(now, minutes as i64 * 60)))?;
    }
    Ok(())
}

fn start_node(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, graph: &Graph<'_>, node: &NodeDef, nr: &NodeRun) -> Result<Acts, String> {
    let now = host.now();
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
            let mut outputs = run.inputs.as_object().cloned().unwrap_or_default();
            outputs.retain(|_, value| !value.is_null());
            succeed_node(db, host, run, def, node, &nr, outputs, Vec::new(), "engine")
        }
        NodeKind::Output => {
            let mut merged = run.outputs.as_object().cloned().unwrap_or_default();
            if let Some(map) = inputs.as_object() {
                for (key, value) in map {
                    merged.insert(key.clone(), value.clone());
                }
            }
            store::set_run_outputs(db, &run.id, &Value::Object(merged))?;
            succeed_node(db, host, run, def, node, &nr, Map::new(), Vec::new(), "engine")
        }
        NodeKind::Transform => {
            let cfg: TransformConfig = config(node)?;
            let mut outputs = Map::new();
            for item in &cfg.assignments {
                match template::eval(&item.expr, &context) {
                    Ok(value) => {
                        outputs.insert(item.port.clone(), value);
                    }
                    Err(error) => return fail_node(db, host, run, def, &nr, "expression_failed", &error, true),
                }
            }
            succeed_node(db, host, run, def, node, &nr, outputs, Vec::new(), "engine")
        }
        NodeKind::Switch => {
            let cfg: SwitchConfig = config(node)?;
            let value = match cfg.value.as_deref().filter(|value| !value.trim().is_empty()) {
                Some(expr) => match template::eval(expr, &context) {
                    Ok(value) => value,
                    Err(error) => return fail_node(db, host, run, def, &nr, "expression_failed", &error, true),
                },
                None => inputs.get("value").cloned().unwrap_or(inputs.clone()),
            };
            let mut fired = Vec::new();
            for case in &cfg.cases {
                match template::eval(&case.when, &context) {
                    Ok(result) if template::truthy(&result) => {
                        fired.push(case.port.clone());
                        if cfg.mode != "all" {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => return fail_node(db, host, run, def, &nr, "expression_failed", &error, true),
                }
            }
            if fired.is_empty() {
                match &cfg.default_port {
                    Some(port) => fired.push(port.clone()),
                    None => return fail_node(db, host, run, def, &nr, "no_case_matched", &tr!("没有命中任何分支，也没有默认分支"), false),
                }
            }
            let mut outputs = Map::new();
            for port in branch_ports(node) {
                outputs.insert(port, value.clone());
            }
            succeed_node(db, host, run, def, node, &nr, outputs, fired, "engine")
        }
        NodeKind::Agent => start_agent(db, host, run, def, node, &nr, &context),
        NodeKind::Gate => start_gate(db, host, run, def, node, &nr, &context),
        NodeKind::Script | NodeKind::Http | NodeKind::Git => start_job(db, host, run, def, node, &nr, &context),
        NodeKind::Notify => {
            let cfg: NotifyConfig = config(node)?;
            let text = match template::render(&cfg.text, &context, Place::Text, None) {
                Ok(text) => text,
                Err(error) => return fail_node(db, host, run, def, &nr, "template_failed", &error, false),
            };
            let session = root_overview(db, run)?;
            append_overview(db, &session, &text, &[], &now)?;
            let mut acts = vec![Action::SessionChanged(session.clone())];
            if cfg.channel == "feishu" {
                acts.push(Action::FeishuNotify { session_id: session, text });
            }
            acts.extend(succeed_node(db, host, run, def, node, &nr, Map::new(), Vec::new(), "engine")?);
            Ok(acts)
        }
        NodeKind::Subflow => start_subflow(db, host, run, def, node, &nr, &context),
        NodeKind::Map => start_map(db, host, run, def, node, &nr, &context),
        NodeKind::Note => Ok(Vec::new()),
    }
}

/// 子运行的审批、通知都发到最外层运行的总览会话
fn root_overview(db: &Connection, run: &Run) -> Result<String, String> {
    let mut current = run.clone();
    for _ in 0..=MAX_SUBFLOW_DEPTH {
        let Some(parent) = current.parent_node_run_id.clone() else { break };
        let parent_nr = store::node_run(db, &parent)?;
        current = store::run(db, &parent_nr.run_id)?;
    }
    Ok(current.overview_session_id)
}

fn root_run(db: &Connection, run: &Run) -> Result<Run, String> {
    let mut current = run.clone();
    for _ in 0..=MAX_SUBFLOW_DEPTH {
        let Some(parent) = current.parent_node_run_id.clone() else { break };
        let parent_nr = store::node_run(db, &parent)?;
        current = store::run(db, &parent_nr.run_id)?;
    }
    Ok(current)
}

// ---------- agent ----------

fn start_agent(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, node: &NodeDef, nr: &NodeRun, context: &Value) -> Result<Acts, String> {
    let now = host.now();
    let cfg: AgentConfig = config(node)?;
    let prompt = match template::render(&cfg.prompt, context, Place::Text, None) {
        Ok(prompt) => prompt,
        Err(error) => return fail_node(db, host, run, def, nr, "template_failed", &error, false),
    };
    // 会话：同节点上一轮的会话可以续上
    let previous: Option<String> = if cfg.continue_session == "same_node" {
        db.query_row(
            "SELECT session_id FROM workflow_node_runs WHERE run_id=?1 AND node_id=?2 AND session_id IS NOT NULL
             AND id != ?3 ORDER BY created_at DESC LIMIT 1",
            params![run.id, node.id, nr.id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else {
        None
    };
    let session_id = previous.unwrap_or_else(|| format!("{WF_NODE_PREFIX}{}", nr.id));
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

    let untrusted = run.extra.get("untrusted").and_then(Value::as_bool).unwrap_or(false)
        && node_has_trigger_input(def, node);
    let brief = super::brief::agent_brief(&super::brief::BriefInput {
        workflow_name: &workflow.name,
        node,
        def,
        iteration: nr.iteration,
        attempt: nr.attempt,
        prompt: &prompt,
        inputs: &nr.inputs,
        context,
        untrusted,
        artifacts: host.artifacts_root().map(|root| values::artifact_dir(&root, &run.id, &nr.id, "inputs")),
        allow_tasks: cfg.allow_task_commands,
    });
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
    set_deadline(db, node, nr, &now)?;
    store::event(db, &run.id, Some(&nr.id), "node_status", &tr!("{node} 开始运行", node = display_name(node)), json!({ "sessionId": session_id }), &now)?;
    Ok(vec![node_changed(nr, "running"), Action::DispatchAgents])
}

fn node_has_trigger_input(def: &WorkflowDef, node: &NodeDef) -> bool {
    let Some(trigger) = def.trigger() else { return false };
    def.edges.iter().any(|edge| edge.target == node.id && edge.source == trigger.id)
}

fn display_name(node: &NodeDef) -> String {
    if node.title.trim().is_empty() {
        node.id.clone()
    } else {
        node.title.clone()
    }
}

/// agent 会话里没有在跑的 run 了：判断节点是完成、追问还是失败
pub fn on_session_idle(db: &Connection, host: &dyn Host, session_id: &str) -> Result<Acts, String> {
    let Some(nr) = store::node_run_by_session(db, session_id)? else {
        return Ok(Vec::new());
    };
    if !matches!(nr.status.as_str(), "running" | "waiting_human") {
        return Ok(Vec::new());
    }
    let active: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE session_id=?1 AND status IN ('queued','running'))",
            [session_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
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
    let last_status: Option<String> = db
        .query_row(
            "SELECT status FROM agent_runs WHERE session_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
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
    let mut outputs = values::outputs(db, &nr.id)?;
    let text = final_text.unwrap_or_default();
    outputs.insert("text".into(), json!(text));
    outputs.insert("session_id".into(), json!(session_id));
    if let Some(port) = &cfg.fallback_text_port {
        if !outputs.contains_key(port) && !text.trim().is_empty() {
            outputs.insert(port.clone(), json!(text));
        }
    }
    let mut problems = Vec::new();
    for port in &node.outputs {
        match outputs.get(&port.name) {
            Some(value) if !value.is_null() => problems.extend(values::check_output(value, port)),
            _ if port.required => problems.push(tr!("{port}：必填，还没有提交", port = port.name)),
            _ => {}
        }
    }
    if problems.is_empty() {
        return succeed_node(db, host, &run, &def, &node, &nr, outputs, Vec::new(), "agent_text");
    }
    if last_status.as_deref() != Some("completed") {
        return fail_node(db, host, &run, &def, &nr, "agent_failed", &problems.join("\n"), true);
    }
    if (nr.repair_count as u32) < cfg.max_repair_turns {
        store::bump_repair(db, &nr.id)?;
        let message = super::brief::repair_message(&node, &problems);
        db.execute(
            "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'user', ?3, ?4)",
            params![uuid::Uuid::new_v4().to_string(), session_id, message, now],
        )
        .map_err(|error| error.to_string())?;
        crate::commands::dispatch(db, session_id, None, &message, &now, &[], crate::models::RunMode::Auto)?;
        store::event(db, &run.id, Some(&nr.id), "retry", &tr!("输出不合格，追问补交"), json!({ "problems": problems }), &now)?;
        return Ok(vec![Action::DispatchAgents, Action::SessionChanged(session_id.to_string())]);
    }
    fail_node(db, host, &run, &def, &nr, "output_invalid", &problems.join("\n"), true)
}

/// `multara node output` 写入：校验后落库
pub fn submit_output(db: &Connection, node_run_id: &str, port_name: &str, value: Value, now: &str) -> Result<(), String> {
    let nr = store::node_run(db, node_run_id)?;
    if !matches!(nr.status.as_str(), "running" | "waiting_human") {
        return Err(tr!("节点已经结束，不能再提交输出"));
    }
    let Loaded { def, .. } = load(db, &nr.run_id)?;
    let node = def.node(&nr.node_id).ok_or_else(|| tr!("节点不存在"))?;
    let port = node
        .outputs
        .iter()
        .find(|port| port.name == port_name)
        .ok_or_else(|| {
            let names: Vec<&str> = node.outputs.iter().map(|port| port.name.as_str()).collect();
            tr!("没有输出端口 {port}，可用的有：{names}", port = port_name, names = names.join(", "))
        })?;
    let value = values::coerce(value, port)?;
    let problems = values::check_output(&value, port);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    values::put(
        db,
        None,
        values::PutValue {
            run_id: &nr.run_id,
            node_run_id: &nr.id,
            node_id: &nr.node_id,
            port: &port.name,
            port_type: port.port_type,
            value: &value,
            source: "agent_cli",
            now,
        },
    )
}

pub fn declare_failure(db: &Connection, node_run_id: &str, reason: &str) -> Result<(), String> {
    let nr = store::node_run(db, node_run_id)?;
    let mut state = nr.state.as_object().cloned().unwrap_or_default();
    state.insert("declaredFailure".into(), json!(reason));
    store::set_node_field(db, node_run_id, "state_json", Some(&Value::Object(state).to_string()))
}

// ---------- gate ----------

fn start_gate(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, node: &NodeDef, nr: &NodeRun, context: &Value) -> Result<Acts, String> {
    let now = host.now();
    let cfg: GateConfig = config(node)?;
    let title = template::render(&cfg.title, context, Place::Text, None).unwrap_or_default();
    let title = if title.trim().is_empty() { display_name(node) } else { title };
    let body = match template::render(&cfg.body, context, Place::Text, None) {
        Ok(body) => body,
        Err(error) => return fail_node(db, host, run, def, nr, "template_failed", &error, false),
    };
    let overview = root_overview(db, run)?;
    let actions: Vec<Value> = cfg
        .effective_decisions()
        .iter()
        .map(|decision| {
            json!({
                "label": decision.label,
                "prompt": decision.label,
                "primary": decision.primary,
                "action": {
                    "type": "workflow.gate",
                    "nodeRunId": nr.id,
                    "decision": decision.port,
                    "requireComment": decision.require_comment,
                    "form": cfg.form,
                }
            })
        })
        .collect();
    let content = format!("### {} {title}\n\n{body}", tr!("待审批："));
    append_overview(db, &overview, &content, &actions, &now)?;
    let root = root_run(db, run)?;
    let _ = crate::ops::inbox::upsert(
        db,
        crate::models::InboxInput {
            session_id: overview.clone(),
            project_id: None,
            task_id: root.task_id.clone(),
            title: tr!("待审批：{title}", title = title),
            preview: body.chars().take(200).collect(),
            status: "needs_input".into(),
        },
    );
    if let Some(task_id) = &root.task_id {
        let _ = db.execute("UPDATE tasks SET status='in_review', updated_at=?1 WHERE id=?2", params![now, task_id]);
    }
    let mut state = Map::new();
    state.insert("title".into(), json!(title));
    store::set_node_field(db, &nr.id, "state_json", Some(&Value::Object(state).to_string()))?;
    store::set_node_status(db, &nr.id, "waiting_human", &now)?;
    if node.timeout_minutes.is_some() {
        set_deadline(db, node, nr, &now)?;
    }
    store::event(db, &run.id, Some(&nr.id), "gate", &tr!("等待审批：{title}", title = title), json!({}), &now)?;
    let mut acts = vec![node_changed(nr, "waiting_human"), Action::SessionChanged(overview.clone())];
    if cfg.notify.get("feishu").and_then(Value::as_bool).unwrap_or(false) {
        acts.push(Action::FeishuNotify { session_id: overview, text: content });
    }
    Ok(acts)
}

pub fn decide_gate(db: &Connection, host: &dyn Host, node_run_id: &str, decision: &str, comment: &str, form: Value, source: &str) -> Result<Acts, String> {
    let nr = store::node_run(db, node_run_id)?;
    if nr.status != "waiting_human" || nr.kind != "gate" {
        return Err(tr!("这个关卡已经不需要审批了"));
    }
    let Loaded { run, def } = load(db, &nr.run_id)?;
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    let cfg: GateConfig = config(&node)?;
    let chosen = cfg
        .effective_decisions()
        .into_iter()
        .find(|item| item.port == decision)
        .ok_or_else(|| tr!("无效的审批选项"))?;
    if chosen.require_comment && comment.trim().is_empty() {
        return Err(tr!("「{label}」需要填写说明", label = chosen.label));
    }
    if let Some(schema) = &cfg.form {
        if !form.is_null() {
            super::schema::validate(schema, &form).map_err(|errors| errors.join("; "))?;
        }
    }
    let now = host.now();
    store::insert_gate_decision(
        db,
        &store::GateDecisionRow {
            node_run_id: nr.id.clone(),
            decision: decision.to_string(),
            comment: comment.trim().to_string(),
            form: form.clone(),
            source: source.to_string(),
            decided_at: now.clone(),
        },
    )?;
    let payload = json!({ "decision": decision, "comment": comment.trim(), "form": form });
    let mut outputs = Map::new();
    for port in branch_ports(&node) {
        outputs.insert(port, payload.clone());
    }
    outputs.insert("comment".into(), json!(comment.trim()));
    outputs.insert("form".into(), form);
    outputs.insert("decision".into(), json!(decision));
    let overview = root_overview(db, &run)?;
    let mut message = tr!("已选择「{label}」。", label = chosen.label);
    if !comment.trim().is_empty() {
        message.push_str(&format!("\n\n> {}", comment.trim()));
    }
    append_overview(db, &overview, &message, &[], &now)?;
    let mut acts = vec![Action::SessionChanged(overview)];
    acts.extend(succeed_node(db, host, &run, &def, &node, &nr, outputs, vec![decision.to_string()], "user")?);
    Ok(acts)
}

/// 总览会话里打字审批：「通过」「打回：原因」「拒绝」或选项名开头
pub fn handle_overview_message(db: &Connection, host: &dyn Host, session_id: &str, text: &str) -> Result<Option<Acts>, String> {
    let Some(run) = store::run_by_overview_session(db, session_id)? else {
        return Ok(None);
    };
    let waiting = waiting_gates(db, &run.id)?;
    let Some(nr) = waiting.into_iter().next() else {
        return Ok(Some(Vec::new()));
    };
    let Loaded { def, .. } = load(db, &nr.run_id)?;
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    let cfg: GateConfig = config(&node)?;
    let trimmed = text.trim();
    let aliases = |port: &str| -> Vec<String> {
        match port {
            "approve" => vec!["通过".into(), "同意".into(), "approve".into(), "ok".into(), "lgtm".into()],
            "changes" => vec!["打回".into(), "修改".into(), "changes".into()],
            "reject" => vec!["拒绝".into(), "reject".into()],
            _ => Vec::new(),
        }
    };
    for decision in cfg.effective_decisions() {
        let mut names = aliases(&decision.port);
        names.push(decision.label.clone());
        names.push(decision.port.clone());
        for name in names {
            if trimmed.to_lowercase().starts_with(&name.to_lowercase()) {
                let comment = trimmed[name.len().min(trimmed.len())..]
                    .trim_start_matches(|c: char| c == ':' || c == '：' || c.is_whitespace())
                    .to_string();
                return decide_gate(db, host, &nr.id, &decision.port, &comment, Value::Null, "chat").map(Some);
            }
        }
    }
    let labels: Vec<String> = cfg.effective_decisions().into_iter().map(|item| item.label).collect();
    let hint = tr!("没听懂这条审批。请回复以下之一开头：{labels}（需要说明时用「打回：原因」）", labels = labels.join(" / "));
    append_overview(db, session_id, &hint, &[], &host.now())?;
    Ok(Some(vec![Action::SessionChanged(session_id.to_string())]))
}

fn waiting_gates(db: &Connection, run_id: &str) -> Result<Vec<NodeRun>, String> {
    let mut found: Vec<NodeRun> = store::current_node_runs(db, run_id)?
        .into_values()
        .filter(|item| item.kind == "gate" && item.status == "waiting_human")
        .collect();
    // 子运行里的关卡
    for item in store::current_node_runs(db, run_id)?.into_values() {
        if item.status == "waiting_children" {
            for child in store::child_runs(db, &item.id)? {
                if !child.is_terminal() {
                    found.extend(waiting_gates(db, &child.id)?);
                }
            }
        }
    }
    found.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    Ok(found)
}

// ---------- 后台节点 ----------

fn start_job(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, node: &NodeDef, nr: &NodeRun, context: &Value) -> Result<Acts, String> {
    let now = host.now();
    let secrets = host.secrets();
    let artifacts = host.artifacts_root();
    let input_dir = artifacts.as_ref().map(|root| values::artifact_dir(root, &run.id, &nr.id, "inputs"));
    let output_dir = artifacts.as_ref().map(|root| values::artifact_dir(root, &run.id, &nr.id, "outputs"));
    let slot_path = |slot: &str| -> Result<Option<String>, String> {
        if let Some(fork) = &nr.fork_workspace_id {
            if node.workspaces.iter().any(|usage| usage.slot == slot && usage.fork) {
                return Ok(store::workspace_path(db, fork)?.map(|(path, _, _)| path));
            }
        }
        Ok(slot_workspace(db, &run.id, slot)?.map(|slot| slot.path))
    };
    let rendered = (|| -> Result<(Value, BTreeMap<String, String>, Option<String>), String> {
        match node.kind {
            NodeKind::Script => {
                let cfg: ScriptConfig = config(node)?;
                let shell = ShellKind::from_config(&cfg.shell);
                let command = template::render(&cfg.command, context, Place::Command(shell), None)?;
                let mut env = BTreeMap::new();
                for (key, value) in &cfg.env {
                    env.insert(key.clone(), template::render(value, context, Place::Secret, Some(&secrets))?);
                }
                let cwd = match &cfg.cwd {
                    Some(cwd) => {
                        let base = slot_path(&cwd.slot)?.ok_or_else(|| tr!("引用了未声明的工作区槽位 {slot}", slot = cwd.slot))?;
                        Some(match &cwd.subdir {
                            Some(sub) if !sub.trim().is_empty() => PathBuf::from(base).join(sub).to_string_lossy().to_string(),
                            _ => base,
                        })
                    }
                    None => None,
                };
                let mut spec = serde_json::to_value(&cfg).map_err(|error| error.to_string())?;
                spec["command"] = json!(command);
                spec["env"] = json!({});
                Ok((spec, env, cwd))
            }
            NodeKind::Http => {
                let cfg: HttpConfig = config(node)?;
                let url = template::render(&cfg.url, context, Place::Secret, Some(&secrets))?;
                let mut headers = BTreeMap::new();
                for (key, value) in &cfg.headers {
                    headers.insert(key.clone(), template::render(value, context, Place::Secret, Some(&secrets))?);
                }
                let body = match &cfg.body {
                    Some(body) => Some(template::render(body, context, Place::Secret, Some(&secrets))?),
                    None => None,
                };
                let mut spec = serde_json::to_value(&cfg).map_err(|error| error.to_string())?;
                spec["url"] = json!("");
                spec["headers"] = json!({});
                spec["body"] = Value::Null;
                let mut secret_env = headers.into_iter().map(|(k, v)| (format!("header:{k}"), v)).collect::<BTreeMap<_, _>>();
                secret_env.insert("url".into(), url);
                if let Some(body) = body {
                    secret_env.insert("body".into(), body);
                }
                Ok((spec, secret_env, None))
            }
            NodeKind::Git => {
                let cfg: GitConfig = config(node)?;
                let mut spec = serde_json::to_value(&cfg).map_err(|error| error.to_string())?;
                if let Some(message) = &cfg.message {
                    spec["message"] = json!(template::render(message, context, Place::Text, None)?);
                }
                if let Some(branch) = &cfg.branch {
                    spec["branch"] = json!(template::render(branch, context, Place::Text, None)?);
                }
                let slot = slot_workspace(db, &run.id, &cfg.slot)?.ok_or_else(|| tr!("引用了未声明的工作区槽位 {slot}", slot = cfg.slot))?;
                spec["base"] = json!(slot.base_commit);
                spec["slotBranch"] = json!(slot.branch);
                Ok((spec, BTreeMap::new(), Some(slot.path)))
            }
            _ => Err("not a job".into()),
        }
    })();
    let (spec, secret_env, cwd) = match rendered {
        Ok(value) => value,
        Err(error) => return fail_node(db, host, run, def, nr, "template_failed", &error, false),
    };
    store::set_node_status(db, &nr.id, "running", &now)?;
    set_deadline(db, node, nr, &now)?;
    store::event(db, &run.id, Some(&nr.id), "node_status", &tr!("{node} 开始运行", node = display_name(node)), json!({}), &now)?;
    Ok(vec![
        node_changed(nr, "running"),
        Action::Job(Job {
            node_run_id: nr.id.clone(),
            run_id: run.id.clone(),
            node_id: node.id.clone(),
            kind: node.kind,
            spec,
            secret_env,
            cwd,
            input_dir,
            output_dir,
            timeout_minutes: node.timeout_minutes.or_else(|| node.kind.default_timeout_minutes()),
        }),
    ])
}

pub fn complete_job(db: &Connection, host: &dyn Host, node_run_id: &str, result: JobResult) -> Result<Acts, String> {
    let nr = store::node_run(db, node_run_id)?;
    if nr.status != "running" {
        return Ok(Vec::new());
    }
    let Loaded { run, def } = load(db, &nr.run_id)?;
    if run.is_terminal() {
        return Ok(Vec::new());
    }
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    // 脚本写在 $WF_OUTPUT_DIR 的值已经由执行方放进 outputs；即使失败也保存隐式输出，方便排查
    if let Some((code, message)) = result.error {
        for (port, value) in &result.outputs {
            let port_type = find_port(&effective_outputs(&node, &def), port).map(|p| p.port_type).unwrap_or(PortType::Any);
            let _ = values::put(db, host.artifacts_root().as_deref(), values::PutValue {
                run_id: &run.id, node_run_id: &nr.id, node_id: &nr.node_id, port, port_type, value, source: "script", now: &host.now(),
            });
        }
        let retryable = code != "template_failed";
        return fail_node(db, host, &run, &def, &nr, &code, &message, retryable);
    }
    let mut problems = Vec::new();
    for port in &node.outputs {
        match result.outputs.get(&port.name) {
            Some(value) if !value.is_null() => problems.extend(values::check_output(value, port)),
            _ if port.required => problems.push(tr!("{port}：必填，还没有提交", port = port.name)),
            _ => {}
        }
    }
    if !problems.is_empty() {
        return fail_node(db, host, &run, &def, &nr, "output_invalid", &problems.join("\n"), false);
    }
    succeed_node(db, host, &run, &def, &node, &nr, result.outputs, Vec::new(), "script")
}

// ---------- 子流程与 Map ----------

fn child_inputs(mapping: &BTreeMap<String, String>, context: &Value) -> Result<Map<String, Value>, String> {
    let mut inputs = Map::new();
    for (key, source) in mapping {
        inputs.insert(key.clone(), template::render_value(source, context)?);
    }
    Ok(inputs)
}

fn start_subflow(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, node: &NodeDef, nr: &NodeRun, context: &Value) -> Result<Acts, String> {
    let cfg: SubflowConfig = config(node)?;
    let mut inputs = match child_inputs(&cfg.inputs, context) {
        Ok(inputs) => inputs,
        Err(error) => return fail_node(db, host, run, def, nr, "template_failed", &error, false),
    };
    // 未映射的入参：同名输入端口直接透传
    if let Some(map) = nr.inputs.as_object() {
        for (key, value) in map {
            inputs.entry(key.clone()).or_insert(value.clone());
        }
    }
    let now = host.now();
    store::set_node_status(db, &nr.id, "waiting_children", &now)?;
    let started = start_run(db, host, StartRun {
        workflow_id: &cfg.workflow_id,
        version: cfg.version,
        inputs: Value::Object(inputs),
        source: "subflow",
        trigger_ref: Some(&nr.id),
        parent_node_run_id: Some(&nr.id),
        map_index: None,
        extra: json!({ "parent": { "run_id": run.id, "node_id": node.id } }),
        untrusted: run.extra.get("untrusted").and_then(Value::as_bool).unwrap_or(false),
    });
    match started {
        Ok((_child, mut acts)) => {
            acts.insert(0, node_changed(nr, "waiting_children"));
            let current = store::node_run(db, &nr.id)?;
            if current.status == "waiting_children" {
                // 子运行可能已经同步结束（全是同步节点），由 after_terminal 回调处理
            }
            Ok(acts)
        }
        Err(error) => {
            store::set_node_status(db, &nr.id, "running", &now)?;
            fail_node(db, host, run, def, nr, "subflow_failed", &error, false)
        }
    }
}

fn start_map(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, node: &NodeDef, nr: &NodeRun, context: &Value) -> Result<Acts, String> {
    let cfg: MapConfig = config(node)?;
    let items = match template::eval(&cfg.items, context) {
        Ok(Value::Array(items)) => items,
        Ok(_) => return fail_node(db, host, run, def, nr, "type_mismatch", &tr!("Map 的 items 必须是数组"), false),
        Err(error) => return fail_node(db, host, run, def, nr, "expression_failed", &error, false),
    };
    let extra = match child_inputs(&cfg.extra_inputs, context) {
        Ok(inputs) => inputs,
        Err(error) => return fail_node(db, host, run, def, nr, "template_failed", &error, false),
    };
    let now = host.now();
    let state = json!({ "items": items, "extra": extra, "next": 0 });
    store::set_node_field(db, &nr.id, "state_json", Some(&state.to_string()))?;
    store::set_node_status(db, &nr.id, "waiting_children", &now)?;
    let mut acts = vec![node_changed(nr, "waiting_children")];
    if items.is_empty() {
        let mut outputs = Map::new();
        outputs.insert("results".into(), json!([]));
        outputs.insert("failed_indexes".into(), json!([]));
        acts.extend(succeed_node(db, host, run, def, node, nr, outputs, Vec::new(), "engine")?);
        return Ok(acts);
    }
    acts.extend(map_fill(db, host, &nr.id)?);
    Ok(acts)
}

/// 按并发上限补齐 Map 的子运行
fn map_fill(db: &Connection, host: &dyn Host, node_run_id: &str) -> Result<Acts, String> {
    let nr = store::node_run(db, node_run_id)?;
    if nr.status != "waiting_children" {
        return Ok(Vec::new());
    }
    let Loaded { run, def } = load(db, &nr.run_id)?;
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    let cfg: MapConfig = config(&node)?;
    let items = nr.state["items"].as_array().cloned().unwrap_or_default();
    let extra = nr.state["extra"].as_object().cloned().unwrap_or_default();
    let mut next = nr.state["next"].as_u64().unwrap_or(0) as usize;
    let children = store::child_runs(db, &nr.id)?;
    let active = children.iter().filter(|child| !child.is_terminal()).count();
    let mut acts = Vec::new();
    let mut slots = (cfg.concurrency.max(1) as usize).saturating_sub(active);
    while slots > 0 && next < items.len() {
        let mut inputs = extra.clone();
        inputs.insert(cfg.item_input.clone(), items[next].clone());
        let index = next as i64;
        next += 1;
        let mut state = nr.state.clone();
        state["next"] = json!(next);
        store::set_node_field(db, &nr.id, "state_json", Some(&state.to_string()))?;
        match start_run(db, host, StartRun {
            workflow_id: &cfg.workflow_id,
            version: cfg.version,
            inputs: Value::Object(inputs),
            source: "map",
            trigger_ref: Some(&nr.id),
            parent_node_run_id: Some(&nr.id),
            map_index: Some(index),
            extra: json!({ "item": items[index as usize], "index": index, "parent": { "run_id": run.id, "node_id": node.id } }),
            untrusted: run.extra.get("untrusted").and_then(Value::as_bool).unwrap_or(false),
        }) {
            Ok((_, more)) => acts.extend(more),
            Err(error) => {
                store::set_node_status(db, &nr.id, "running", &host.now())?;
                acts.extend(fail_node(db, host, &run, &def, &nr, "subflow_failed", &error, false)?);
                return Ok(acts);
            }
        }
        slots -= 1;
    }
    Ok(acts)
}

/// 子运行结束：推进父节点
fn on_child_finished(db: &Connection, host: &dyn Host, child: &Run) -> Result<Acts, String> {
    let Some(parent_id) = &child.parent_node_run_id else { return Ok(Vec::new()) };
    let nr = store::node_run(db, parent_id)?;
    if nr.status != "waiting_children" {
        return Ok(Vec::new());
    }
    let Loaded { run, def } = load(db, &nr.run_id)?;
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    if node.kind == NodeKind::Subflow {
        if child.status == "succeeded" {
            let mut outputs = child.outputs.as_object().cloned().unwrap_or_default();
            outputs.insert("outputs".into(), child.outputs.clone());
            return succeed_node(db, host, &run, &def, &node, &nr, outputs, Vec::new(), "engine");
        }
        store::set_node_status(db, &nr.id, "running", &host.now())?;
        let message = child.error.clone().unwrap_or_else(|| tr!("子工作流运行失败"));
        return fail_node(db, host, &run, &def, &nr, "subflow_failed", &message, true);
    }
    // Map
    let cfg: MapConfig = config(&node)?;
    let children = store::child_runs(db, &nr.id)?;
    let total = nr.state["items"].as_array().map(Vec::len).unwrap_or(0);
    if cfg.fail_fast && child.status == "failed" {
        let mut acts = Vec::new();
        for other in children.iter().filter(|other| !other.is_terminal()) {
            acts.extend(cancel_run(db, host, &other.id, "fail_fast")?);
        }
        store::set_node_status(db, &nr.id, "running", &host.now())?;
        acts.extend(fail_node(db, host, &run, &def, &nr, "subflow_failed", &tr!("第 {index} 项失败", index = child.map_index.unwrap_or(0) + 1), false)?);
        return Ok(acts);
    }
    let finished = children.iter().filter(|other| other.is_terminal()).count();
    if finished < total {
        return map_fill(db, host, &nr.id);
    }
    let mut results = vec![Value::Null; total];
    let mut failed = Vec::new();
    for item in &children {
        let index = item.map_index.unwrap_or(0) as usize;
        if index < total {
            results[index] = item.outputs.clone();
        }
        if item.status != "succeeded" {
            failed.push(json!(index));
        }
    }
    let mut outputs = Map::new();
    outputs.insert("results".into(), Value::Array(results));
    outputs.insert("failed_indexes".into(), Value::Array(failed));
    succeed_node(db, host, &run, &def, &node, &nr, outputs, Vec::new(), "engine")
}

// ---------- 结束节点 ----------

#[allow(clippy::too_many_arguments)]
fn succeed_node(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, node: &NodeDef, nr: &NodeRun, outputs: Map<String, Value>, fired: Vec<String>, source: &str) -> Result<Acts, String> {
    let now = host.now();
    let ports = effective_outputs(node, def);
    let artifacts = host.artifacts_root();
    for (name, value) in &outputs {
        let port_type = find_port(&ports, name).map(|port| port.port_type).unwrap_or(PortType::Any);
        let source = if node.kind == NodeKind::Agent {
            if source == "agent_text" && values::output(db, &nr.id, name)?.is_some() {
                continue; // agent 用 CLI 提交过的值保持不变
            }
            "agent_text"
        } else {
            source
        };
        values::put(db, artifacts.as_deref(), values::PutValue {
            run_id: &run.id, node_run_id: &nr.id, node_id: &nr.node_id, port: name, port_type, value, source, now: &now,
        })?;
    }
    let mut acts = Vec::new();
    // 自动提交
    let mut after = Map::new();
    for usage in node.workspaces.iter().filter(|usage| usage.access == WorkspaceAccess::Write) {
        let Some(slot) = slot_workspace(db, &run.id, &usage.slot)? else { continue };
        let slot_def = def.slot(&usage.slot);
        let path = if usage.fork {
            nr.fork_workspace_id.as_deref().and_then(|id| store::workspace_path(db, id).ok().flatten()).map(|(path, _, _)| path).unwrap_or(slot.path.clone())
        } else {
            slot.path.clone()
        };
        if node.kind != NodeKind::Git && slot_def.map(|slot| slot.auto_commit).unwrap_or(true) {
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
        let head = host.git_head(&path);
        after.insert(usage.slot.clone(), json!(head));
        if !usage.fork {
            store::set_slot_head(db, &run.id, &usage.slot, head.as_deref())?;
        } else if let Some(fork) = &nr.fork_workspace_id {
            if let Some((_, branch, _)) = store::workspace_path(db, fork)? {
                values::put(db, None, values::PutValue {
                    run_id: &run.id, node_run_id: &nr.id, node_id: &nr.node_id, port: "fork_branch", port_type: PortType::Text, value: &json!(branch), source: "engine", now: &now,
                })?;
            }
        }
    }
    if node.kind == NodeKind::Git {
        if let Ok(cfg) = config::<GitConfig>(node) {
            if let Some(head) = outputs.get("head").and_then(Value::as_str) {
                store::set_slot_head(db, &run.id, &cfg.slot, Some(head))?;
            }
        }
    }
    if !after.is_empty() {
        store::set_node_field(db, &nr.id, "workspace_after_json", Some(&Value::Object(after).to_string()))?;
    }
    store::release_locks(db, &nr.id)?;

    // 回边
    let mut fired = fired;
    if node.kind.is_branching() {
        let graph = Graph::new(def);
        let mut extra_fired = Vec::new();
        for port in fired.clone() {
            for edge in graph.back_edges_from(&node.id).into_iter().filter(|edge| edge.source_port == port) {
                let count = store::loop_count(db, &run.id, &edge.id)?;
                let limit = edge.max_iterations.unwrap_or(1);
                if count >= limit {
                    fired.retain(|item| item != &port);
                    if !extra_fired.contains(&EXHAUSTED_PORT.to_string()) {
                        extra_fired.push(EXHAUSTED_PORT.to_string());
                    }
                    store::event(db, &run.id, Some(&nr.id), "loop", &tr!("循环已达上限 {limit} 轮", limit = limit), json!({ "edgeId": edge.id }), &now)?;
                    continue;
                }
                store::bump_loop(db, &run.id, &edge.id)?;
                let value = outputs.get(&port).cloned().unwrap_or(Value::Null);
                acts.extend(loop_reset(db, host, run, def, &graph, edge, value)?);
            }
        }
        fired.extend(extra_fired);
        if fired.iter().any(|port| port == EXHAUSTED_PORT) {
            let connected = def.edges.iter().any(|edge| edge.source == node.id && edge.source_port == EXHAUSTED_PORT);
            if !connected {
                store::set_node_field(db, &nr.id, "fired_ports_json", Some(&json!(fired).to_string()))?;
                return fail_node(db, host, run, def, nr, "loop_exhausted", &tr!("循环已达上限，且没有连接「已用尽」分支"), false).map(|more| {
                    acts.extend(more);
                    acts
                });
            }
            let value = outputs.values().next().cloned().unwrap_or(Value::Null);
            values::put(db, None, values::PutValue {
                run_id: &run.id, node_run_id: &nr.id, node_id: &nr.node_id, port: EXHAUSTED_PORT, port_type: PortType::Any, value: &value, source: "engine", now: &now,
            })?;
        }
    }
    store::set_node_field(db, &nr.id, "fired_ports_json", Some(&json!(fired).to_string()))?;
    // loop_reset 可能已经给本节点建了新一轮，这里只结束本条节点运行
    store::set_node_status(db, &nr.id, "succeeded", &now)?;
    store::event(db, &run.id, Some(&nr.id), "node_status", &tr!("{node} 完成", node = display_name(node)), json!({ "fired": fired }), &now)?;
    acts.push(node_changed(nr, "succeeded"));
    Ok(acts)
}

/// 回边触发：T 及其全部前向后代建新一轮
fn loop_reset(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, graph: &Graph<'_>, edge: &EdgeDef, value: Value) -> Result<Acts, String> {
    let now = host.now();
    let current = store::current_node_runs(db, &run.id)?;
    let mut targets = graph.descendants(&edge.target);
    targets.insert(edge.target.clone());
    let mut acts = Vec::new();
    for node_id in &targets {
        let Some(old) = current.get(node_id) else { continue };
        if old.is_active() && old.node_id != edge.source {
            acts.extend(stop_node(db, host, old, "cancelled")?);
        }
        let carried = if node_id == &edge.target {
            let mut map = Map::new();
            map.insert(edge.target_port.clone(), value.clone());
            Some(Value::Object(map))
        } else {
            None
        };
        let kind = def.node(node_id).map(|node| node.kind.as_str()).unwrap_or("transform");
        let id = store::insert_node_run(db, store::NewNodeRun {
            run_id: &run.id, node_id, kind, iteration: old.iteration + 1, attempt: 1, carried: carried.as_ref(), retry_at: None, now: &now,
        })?;
        acts.push(Action::NodeChanged { run_id: run.id.clone(), node_run_id: id, node_id: node_id.clone(), status: "pending".into() });
    }
    store::event(db, &run.id, None, "loop", &tr!("回到 {node} 开始新一轮", node = edge.target), json!({ "edgeId": edge.id }), &now)?;
    Ok(acts)
}

/// 停止一个进行中的节点运行（不触发重试）
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
fn fail_node(db: &Connection, host: &dyn Host, run: &Run, def: &WorkflowDef, nr: &NodeRun, code: &str, message: &str, retryable: bool) -> Result<Acts, String> {
    let now = host.now();
    let node = def.node(&nr.node_id).cloned().ok_or_else(|| tr!("节点不存在"))?;
    let message = super::runtime_redact(message, &host.secrets());
    store::set_node_field(db, &nr.id, "error_code", Some(code))?;
    store::set_node_field(db, &nr.id, "error", Some(&message))?;
    store::release_locks(db, &nr.id)?;
    store::set_node_status(db, &nr.id, "failed", &now)?;
    store::event(db, &run.id, Some(&nr.id), "error", &format!("{}：{message}", display_name(&node)), json!({ "code": code }), &now)?;
    let mut acts = vec![node_changed(nr, "failed")];
    let policy = node.retry.clone();
    let reason = if code == "timeout" { "timeout" } else { "failed" };
    if let Some(policy) = policy.filter(|_| retryable) {
        if (nr.attempt as u32) < policy.max_attempts && policy.on.iter().any(|item| item == reason) {
            let delay = policy.backoff_seconds as i64 * 2_i64.pow((nr.attempt - 1).clamp(0, 10) as u32);
            let retry_at = plus_seconds(&now, delay);
            let id = store::insert_node_run(db, store::NewNodeRun {
                run_id: &run.id, node_id: &nr.node_id, kind: &nr.kind, iteration: nr.iteration, attempt: nr.attempt + 1,
                carried: if nr.carried.is_null() { None } else { Some(&nr.carried) }, retry_at: Some(&retry_at), now: &now,
            })?;
            store::event(db, &run.id, Some(&id), "retry", &tr!("{delay} 秒后第 {attempt} 次尝试", delay = delay, attempt = nr.attempt + 1), json!({}), &now)?;
            acts.push(Action::NodeChanged { run_id: run.id.clone(), node_run_id: id, node_id: nr.node_id.clone(), status: "pending".into() });
            return Ok(acts);
        }
    }
    if node.on_error == OnError::Continue {
        values::put(db, None, values::PutValue {
            run_id: &run.id, node_run_id: &nr.id, node_id: &nr.node_id, port: ERROR_PORT, port_type: PortType::Json,
            value: &json!({ "code": code, "message": message }), source: "engine", now: &now,
        })?;
        return Ok(acts);
    }
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
    } else {
        acts.extend(on_child_finished(db, host, &run)?);
        let parent = store::node_run(db, run.parent_node_run_id.as_deref().unwrap_or_default())?;
        acts.extend(advance(db, host, &parent.run_id)?);
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
    let map = outputs.as_object().cloned().unwrap_or_default();
    let fired = if node.kind.is_branching() {
        map.get("$fired").and_then(Value::as_str).map(|port| vec![port.to_string()]).unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut acts = succeed_node(db, host, &run, &def, &node, &nr, map, fired, "user")?;
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
        let Some(node) = def.node(&nr.node_id).cloned() else { continue };
        if node.kind == NodeKind::Gate {
            let cfg: GateConfig = config(&node)?;
            match cfg.on_timeout.as_deref() {
                Some("approve") | Some("reject") => {
                    let decision = cfg.on_timeout.clone().unwrap_or_default();
                    let port = if decision == "approve" { cfg.effective_decisions().into_iter().find(|d| d.primary).map(|d| d.port).unwrap_or("approve".into()) } else { "reject".into() };
                    acts.extend(decide_gate(db, host, &nr.id, &port, &tr!("审批超时，自动处理"), Value::Null, "timeout")?);
                }
                _ => {
                    acts.extend(stop_node(db, host, &nr, "running")?);
                    acts.extend(fail_node(db, host, &run, &def, &nr, "gate_rejected_timeout", &tr!("审批超时"), false)?);
                }
            }
        } else {
            let mut stop = stop_node(db, host, &nr, "running")?;
            stop.retain(|action| !matches!(action, Action::NodeChanged { .. }));
            acts.extend(stop);
            acts.extend(fail_node(db, host, &run, &def, &nr, "timeout", &tr!("节点运行超时"), true)?);
        }
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
        let Loaded { run, def } = load(db, &run_id)?;
        for nr in store::current_node_runs(db, &run_id)?.into_values() {
            match (nr.kind.as_str(), nr.status.as_str()) {
                ("script" | "http" | "git", "running") => {
                    acts.extend(fail_node(db, host, &run, &def, &nr, "app_restarted", &tr!("Multara 在节点完成前退出"), true)?);
                }
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

