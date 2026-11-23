//! `multara node …` / `multara workflow …`（技术方案第 8 节）。CLI 直接打开 SQLite，
//! 通过 MULTARA_RUN_ID → agent_runs.session_id → workflow_node_runs 找到当前节点。

use std::collections::HashMap;
use std::env;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use super::engine;
use super::model::{self, EdgeDef, InputField, NodeDef, NodeKind, WorkflowDef};
use super::store;
use super::templates;
use super::validate;

struct Args {
    positional: Vec<String>,
    flags: HashMap<String, Vec<String>>,
}

impl Args {
    fn parse(args: &[String]) -> Args {
        let mut positional = Vec::new();
        let mut flags: HashMap<String, Vec<String>> = HashMap::new();
        let mut current: Option<String> = None;
        for arg in args {
            if let Some(name) = arg
                .strip_prefix("--")
                .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            {
                flags.entry(name.to_string()).or_default();
                current = Some(name.to_string());
                continue;
            }
            match &current {
                Some(name) => {
                    let list = flags.entry(name.clone()).or_default();
                    list.push(arg.clone());
                    // 只有 --files / --input 接多个值
                    if name != "files" && name != "input" {
                        current = None;
                    }
                }
                None => positional.push(arg.clone()),
            }
        }
        Args { positional, flags }
    }

    fn one(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|list| list.first()).map(String::as_str)
    }

    fn many(&self, name: &str) -> Vec<String> {
        self.flags.get(name).cloned().unwrap_or_default()
    }

    /// `--flag` 或 `--flag true` 为真，`--flag false` 为假。没写这个参数则是 None。
    fn boolean(&self, name: &str) -> Result<Option<bool>, String> {
        let Some(list) = self.flags.get(name) else { return Ok(None) };
        match list.first().map(String::as_str) {
            None | Some("true") | Some("1") => Ok(Some(true)),
            Some("false") | Some("0") => Ok(Some(false)),
            Some(other) => Err(tr!("--{name} 只能是 true 或 false，收到 {other}", name = name, other = other)),
        }
    }
}

fn current_node_run(db: &Connection) -> Result<store::NodeRun, String> {
    if let Ok(id) = env::var("MULTARA_NODE_RUN_ID") {
        if let Ok(item) = store::node_run(db, &id) {
            return Ok(item);
        }
    }
    let run_id = env::var("MULTARA_RUN_ID").map_err(|_| tr!("当前运行不属于工作流节点"))?;
    let session: Option<String> = db
        .query_row("SELECT session_id FROM agent_runs WHERE id=?1", [&run_id], |row| row.get(0))
        .optional()
        .map_err(|error| error.to_string())?;
    let session = session.ok_or_else(|| tr!("当前运行不属于工作流节点"))?;
    store::node_run_by_session(db, &session)?
        .filter(|item| matches!(item.status.as_str(), "running" | "waiting_human"))
        .ok_or_else(|| tr!("当前运行不属于工作流节点"))
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn workflow_id(parsed: &Args) -> Result<&str, String> {
    parsed.positional.get(2).map(String::as_str).ok_or_else(|| tr!("缺少工作流 id"))
}

fn edit_tasks(db: &Connection, parsed: &Args) -> Result<String, String> {
    let head = parsed.positional.get(2).map(String::as_str).unwrap_or("list");
    let (action, explicit) = match head {
        "add" | "remove" | "list" => (head, parsed.one("workflow")),
        other => ("list", Some(other)),
    };
    let workflow = if let Some(id) = explicit {
        id.to_string()
    } else if let Ok(node_run) = current_node_run(db) {
        store::run(db, &node_run.run_id)?.workflow_id
    } else {
        return Err(tr!("缺少工作流 id"));
    };
    let mut ids = store::linked_task_ids(db, &workflow)?;
    match action {
        "add" => {
            let id = parsed.one("id").ok_or_else(|| tr!("缺少 --id"))?;
            if !ids.iter().any(|item| item == id) {
                ids.push(id.to_string());
            }
            store::set_linked_task_ids(db, &workflow, &ids)?;
            Ok(tr!("已关联需求 {id}", id = id))
        }
        "remove" => {
            let id = parsed.one("id").ok_or_else(|| tr!("缺少 --id"))?;
            ids.retain(|item| item != id);
            store::set_linked_task_ids(db, &workflow, &ids)?;
            Ok(tr!("已取消关联需求 {id}", id = id))
        }
        _ => Ok(pretty(&json!(ids))),
    }
}

fn save(db: &Connection, id: &str, def: &WorkflowDef, now: &str) -> Result<(), String> {
    store::save_draft(db, id, def, None, now)?;
    Ok(())
}

fn summary(workflow: &store::Workflow, def: &WorkflowDef) -> Value {
    json!({
        "id": workflow.id,
        "name": workflow.name,
        "slug": workflow.slug,
        "description": workflow.description,
        "enabled": workflow.enabled,
        "publishedVersion": workflow.published_version,
        "nodeCount": def.nodes.len(),
        "edgeCount": def.edges.len(),
    })
}

fn parse_kind(raw: &str) -> Result<NodeKind, String> {
    match raw {
        "agent" => Ok(NodeKind::Agent),
        "output" => Ok(NodeKind::Output),
        "trigger" | "start" => Err(tr!("开始节点每个工作流只有一个，创建时自带")),
        other => Err(tr!("节点类型只能是 agent 或 output，收到 {kind}", kind = other)),
    }
}

fn default_config(kind: NodeKind) -> Value {
    match kind {
        NodeKind::Agent => json!({
            "target": { "type": "agent", "id": "" },
            "prompt": "",
            "mode": "auto",
            "context": "previous",
            "route": "auto",
            "allowTaskCommands": false,
            "humanConfirm": false
        }),
        NodeKind::Output | NodeKind::Trigger => json!({}),
    }
}

fn unique_id(used: &[String], base: &str) -> String {
    let base = if model::is_valid_id(base) { base.to_string() } else { "step".into() };
    if !used.iter().any(|id| id == &base) {
        return base;
    }
    let mut index = 2;
    loop {
        let next = format!("{base}_{index}");
        if !used.iter().any(|id| id == &next) {
            return next;
        }
        index += 1;
    }
}

fn place(def: &WorkflowDef, x: Option<&str>, y: Option<&str>) -> Result<Value, String> {
    let parse = |raw: &str, flag: &str| {
        raw.parse::<f64>().map_err(|_| tr!("--{flag} 要是数字", flag = flag))
    };
    if let (Some(x), Some(y)) = (x, y) {
        return Ok(json!({ "x": parse(x, "x")?, "y": parse(y, "y")? }));
    }
    let right = def.nodes.iter().filter_map(|node| node.position.get("x").and_then(Value::as_f64)).fold(80.0, f64::max);
    let y = y.map(|raw| parse(raw, "y")).transpose()?.unwrap_or(200.0);
    let x = x.map(|raw| parse(raw, "x")).transpose()?.unwrap_or(right + 220.0);
    Ok(json!({ "x": x, "y": y }))
}

fn node_json(node: &NodeDef) -> Value {
    json!({
        "id": node.id,
        "kind": node.kind.as_str(),
        "title": node.title,
        "config": node.config,
        "position": node.position,
        "timeoutMinutes": node.timeout_minutes,
    })
}

pub fn execute(args: &[String], db: &mut Connection) -> Result<String, String> {
    let parsed = Args::parse(args);
    let resource = parsed.positional.first().map(String::as_str).unwrap_or("");
    let verb = parsed.positional.get(1).map(String::as_str).unwrap_or("");
    let now = chrono::Utc::now().to_rfc3339();
    match (resource, verb) {
        ("node", "inputs") => {
            let nr = current_node_run(db)?;
            Ok(pretty(&nr.inputs))
        }
        ("node", "status") => {
            let nr = current_node_run(db)?;
            Ok(pretty(&json!({
                "nodeId": nr.node_id,
                "iteration": nr.iteration,
                "attempt": nr.attempt,
                "status": nr.status,
                "next": nr.state.get("next"),
            })))
        }
        ("node", "next") => {
            let nr = current_node_run(db)?;
            let id = parsed.one("id").ok_or_else(|| tr!("缺少 --id"))?;
            let reason = parsed.one("reason").unwrap_or("");
            engine::submit_next(db, &nr.id, id, reason)?;
            Ok(tr!("已选择下一步 {id}", id = id))
        }
        ("node", "fail") => {
            let reason = parsed.one("reason").ok_or_else(|| tr!("缺少 --reason"))?;
            let nr = current_node_run(db)?;
            engine::declare_failure(db, &nr.id, reason)?;
            Ok(tr!("已标记节点失败，结束本轮回答即可"))
        }
        ("workflow", "list") => {
            let items: Vec<Value> = store::list_workflows(db)?
                .into_iter()
                .map(|wf| json!({ "id": wf.id, "name": wf.name, "publishedVersion": wf.published_version, "enabled": wf.enabled, "description": wf.description }))
                .collect();
            Ok(pretty(&Value::Array(items)))
        }
        ("workflow", "get") | ("workflow", "draft") => {
            let id = workflow_id(&parsed)?;
            let workflow = store::workflow(db, id)?;
            let def = store::draft(db, id)?;
            if verb == "draft" {
                return Ok(pretty(&json!({ "id": workflow.id, "name": workflow.name, "description": workflow.description, "definition": def })));
            }
            Ok(pretty(&json!({
                "id": workflow.id,
                "name": workflow.name,
                "slug": workflow.slug,
                "description": workflow.description,
                "enabled": workflow.enabled,
                "publishedVersion": workflow.published_version,
                "inputs": def.inputs,
                "nodes": def.nodes.iter().map(node_json).collect::<Vec<_>>(),
                "edges": def.edges,
            })))
        }
        ("workflow", "create") => {
            let name = parsed.one("name").ok_or_else(|| tr!("缺少 --name"))?;
            let description = parsed.one("description").unwrap_or("");
            let def = match parsed.one("template").filter(|id| !id.is_empty()) {
                Some(id) => templates::workflow_template(id, db)?.definition,
                None => {
                    let mut def = WorkflowDef::default();
                    def.nodes[0].title = tr!("开始");
                    def
                }
            };
            let workflow = store::create_workflow(db, name, description, &def, &now)?;
            Ok(pretty(&summary(&workflow, &def)))
        }
        ("workflow", "rename") => {
            let id = workflow_id(&parsed)?;
            let current = store::workflow(db, id)?;
            let name = parsed.one("name").unwrap_or(&current.name);
            let description = parsed.one("description").unwrap_or(&current.description);
            let workflow = store::update_meta(db, id, name, description, &current.icon, &now)?;
            let def = store::draft(db, id)?;
            Ok(pretty(&summary(&workflow, &def)))
        }
        ("workflow", "delete") => {
            let id = workflow_id(&parsed)?;
            let active: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE workflow_id=?1 AND status IN ('queued','running','waiting'))",
                    [id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            if active {
                return Err(tr!("工作流还有运行没结束，请先取消"));
            }
            store::delete_workflow(db, id)?;
            Ok(pretty(&json!({ "deleted": id })))
        }
        ("workflow", "enable") | ("workflow", "disable") => {
            let id = workflow_id(&parsed)?;
            store::set_enabled(db, id, verb == "enable", &now)?;
            Ok(pretty(&json!({ "id": id, "enabled": verb == "enable" })))
        }
        ("workflow", "publish") => {
            let id = workflow_id(&parsed)?;
            let def = store::draft(db, id)?;
            let diagnostics = validate::validate(&def, &store::DbLookup(db));
            if validate::has_errors(&diagnostics) {
                let first = diagnostics.iter().find(|item| item.severity == "error").map(|item| item.message.clone()).unwrap_or_default();
                return Err(tr!("还有错误没有解决，不能发布：{first}", first = first));
            }
            let version = store::publish(db, id, parsed.one("note").unwrap_or(""), &now)?;
            Ok(pretty(&json!({ "id": id, "version": version })))
        }
        ("workflow", "duplicate") => {
            let id = workflow_id(&parsed)?;
            let source = store::workflow(db, id)?;
            let def = store::draft(db, id)?;
            let workflow = store::create_workflow(db, &tr!("{name} 副本", name = source.name), &source.description, &def, &now)?;
            Ok(pretty(&summary(&workflow, &def)))
        }
        ("workflow", "node") => edit_node(db, &parsed, &now),
        ("workflow", "edge") => edit_edge(db, &parsed, &now),
        ("workflow", "input") => edit_input(db, &parsed, &now),
        ("workflow", "tasks") => edit_tasks(db, &parsed),
        ("workflow", "runs") => {
            let id = parsed.positional.get(2).map(String::as_str);
            let runs: Vec<Value> = store::list_runs(db, id, parsed.one("status"), 20)?
                .into_iter()
                .map(|run| json!({ "id": run.id, "workflowId": run.workflow_id, "status": run.status, "createdAt": run.created_at, "error": run.error, "outputs": run.outputs }))
                .collect();
            Ok(pretty(&Value::Array(runs)))
        }
        ("workflow", "run") => {
            let id = parsed.positional.get(2).ok_or_else(|| tr!("缺少工作流 id"))?;
            let mut inputs = match parsed.one("input-json") {
                Some(raw) => serde_json::from_str::<Value>(raw).map_err(|error| tr!("--input-json 不是合法的 JSON：{error}", error = error))?,
                None => json!({}),
            };
            for pair in parsed.many("input") {
                let (key, value) = pair.split_once('=').ok_or_else(|| tr!("--input 的格式是 key=value"))?;
                inputs[key.trim()] = serde_json::from_str(value).unwrap_or_else(|_| json!(value));
            }
            let run_id = engine::enqueue_run(db, id, inputs, "cli", &now)?;
            Ok(pretty(&json!({ "runId": run_id, "status": "queued", "note": tr!("Multara 会在 30 秒内开始这次运行") })))
        }
        _ => Err(tr!(
            "用法：multara workflow list|get|draft|create|rename|delete|enable|disable|publish|duplicate|node|edge|input|runs|run；multara node status|next|fail"
        )),
    }
}

fn edit_node(db: &Connection, parsed: &Args, now: &str) -> Result<String, String> {
    let action = parsed.positional.get(2).map(String::as_str).unwrap_or("");
    let workflow = parsed.positional.get(3).ok_or_else(|| tr!("缺少工作流 id"))?;
    let mut def = store::draft(db, workflow)?;
    let result = match action {
        "add" => {
            let kind = parse_kind(parsed.one("kind").ok_or_else(|| tr!("缺少 --kind"))?)?;
            if kind == NodeKind::Output && def.nodes.iter().any(|node| node.kind == NodeKind::Output) {
                return Err(tr!("结束节点只能有一个"));
            }
            let title = parsed.one("title").unwrap_or(match kind {
                NodeKind::Agent => "智能体",
                NodeKind::Output => "结束",
                NodeKind::Trigger => "开始",
            });
            let used: Vec<String> = def.nodes.iter().map(|node| node.id.clone()).collect();
            let id = match parsed.one("id") {
                Some(id) if model::is_valid_id(id) && !used.iter().any(|item| item == id) => id.to_string(),
                Some(id) if !model::is_valid_id(id) => return Err(tr!("节点 id 要以小写字母开头，只能包含小写字母、数字和下划线")),
                Some(id) => return Err(tr!("节点 id {id} 已经存在", id = id)),
                None => unique_id(&used, kind.as_str()),
            };
            let mut config = default_config(kind);
            if let Some(prompt) = parsed.one("prompt") {
                if let Some(object) = config.as_object_mut() {
                    object.insert("prompt".into(), json!(prompt));
                }
            }
            if let Some(raw) = parsed.one("config") {
                let extra: Value = serde_json::from_str(raw).map_err(|error| tr!("--config 不是合法的 JSON：{error}", error = error))?;
                if let (Some(base), Some(extra)) = (config.as_object_mut(), extra.as_object()) {
                    for (key, value) in extra {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
            let node = NodeDef {
                id: id.clone(),
                kind,
                title: title.to_string(),
                description: None,
                config,
                workspaces: Vec::new(),
                timeout_minutes: parsed.one("timeout").map(|raw| raw.parse::<u32>()).transpose().map_err(|_| tr!("--timeout 要是分钟数"))?,
                position: place(&def, parsed.one("x"), parsed.one("y"))?,
            };
            let shown = node_json(&node);
            def.nodes.push(node);
            shown
        }
        "set" => {
            let node_id = parsed.positional.get(4).map(String::as_str).ok_or_else(|| tr!("缺少节点 id"))?;
            let position = if parsed.one("x").is_some() || parsed.one("y").is_some() {
                let current = def.nodes.iter().find(|node| node.id == node_id);
                let current_x = current.and_then(|node| node.position.get("x")).and_then(Value::as_f64).map(|value| value.to_string());
                let current_y = current.and_then(|node| node.position.get("y")).and_then(Value::as_f64).map(|value| value.to_string());
                let x = parsed.one("x").or(current_x.as_deref());
                let y = parsed.one("y").or(current_y.as_deref());
                Some(place(&def, x, y)?)
            } else {
                None
            };
            let node = def.nodes.iter_mut().find(|node| node.id == node_id).ok_or_else(|| tr!("节点不存在"))?;
            if let Some(title) = parsed.one("title") {
                node.title = title.to_string();
            }
            if let Some(prompt) = parsed.one("prompt") {
                if !node.config.is_object() {
                    node.config = json!({});
                }
                node.config["prompt"] = json!(prompt);
            }
            if let Some(raw) = parsed.one("config") {
                let extra: Value = serde_json::from_str(raw).map_err(|error| tr!("--config 不是合法的 JSON：{error}", error = error))?;
                if !node.config.is_object() {
                    node.config = json!({});
                }
                if let Some(extra) = extra.as_object() {
                    for (key, value) in extra {
                        node.config[key] = value.clone();
                    }
                }
            }
            if let Some(raw) = parsed.one("timeout") {
                let minutes = raw.parse::<u32>().map_err(|_| tr!("--timeout 要是分钟数"))?;
                node.timeout_minutes = if minutes == 0 { None } else { Some(minutes) };
            }
            if let Some(position) = position {
                node.position = position;
            }
            node_json(node)
        }
        "remove" => {
            let node_id = parsed.positional.get(4).map(String::as_str).ok_or_else(|| tr!("缺少节点 id"))?;
            let node = def.nodes.iter().find(|node| node.id == node_id).ok_or_else(|| tr!("节点不存在"))?;
            if node.kind == NodeKind::Trigger {
                return Err(tr!("开始节点不能删除"));
            }
            def.nodes.retain(|node| node.id != node_id);
            def.edges.retain(|edge| edge.source != node_id && edge.target != node_id);
            json!({ "removed": node_id })
        }
        _ => return Err(tr!("用法：multara workflow node add|set|remove <工作流 id> [节点 id]")),
    };
    save(db, workflow, &def, now)?;
    Ok(pretty(&result))
}

fn edit_edge(db: &Connection, parsed: &Args, now: &str) -> Result<String, String> {
    let action = parsed.positional.get(2).map(String::as_str).unwrap_or("");
    let workflow = parsed.positional.get(3).ok_or_else(|| tr!("缺少工作流 id"))?;
    let mut def = store::draft(db, workflow)?;
    let result = match action {
        "add" => {
            let source = parsed.one("from").ok_or_else(|| tr!("缺少 --from"))?.to_string();
            let target = parsed.one("to").ok_or_else(|| tr!("缺少 --to"))?.to_string();
            if def.node(&source).is_none() || def.node(&target).is_none() {
                return Err(tr!("连线指向不存在的节点"));
            }
            if source == target {
                return Err(tr!("连线不能指向自己"));
            }
            let when = parsed.one("when").map(str::to_string);
            if def.edges.iter().any(|edge| edge.source == source && edge.target == target && edge.when == when) {
                return Err(tr!("这条连线已经存在"));
            }
            let max_iterations = match parsed.one("max-iterations") {
                Some(raw) => {
                    let rounds = raw.parse::<u32>().map_err(|_| tr!("--max-iterations 要是 1 到 20"))?;
                    if !(1..=20).contains(&rounds) {
                        return Err(tr!("--max-iterations 要是 1 到 20"));
                    }
                    Some(rounds)
                }
                None => None,
            };
            let used: Vec<String> = def.edges.iter().map(|edge| edge.id.clone()).collect();
            let edge = EdgeDef { id: unique_id(&used, "e"), source, target, when, max_iterations };
            let shown = json!({ "id": edge.id, "source": edge.source, "target": edge.target, "when": edge.when, "maxIterations": edge.max_iterations });
            def.edges.push(edge);
            shown
        }
        "remove" => {
            let edge_id = parsed.positional.get(4).map(String::as_str).ok_or_else(|| tr!("缺少连线 id"))?;
            if !def.edges.iter().any(|edge| edge.id == edge_id) {
                return Err(tr!("连线不存在"));
            }
            def.edges.retain(|edge| edge.id != edge_id);
            json!({ "removed": edge_id })
        }
        _ => return Err(tr!("用法：multara workflow edge add <工作流 id> --from <节点> --to <节点>；multara workflow edge remove <工作流 id> <连线 id>")),
    };
    save(db, workflow, &def, now)?;
    Ok(pretty(&result))
}

fn edit_input(db: &Connection, parsed: &Args, now: &str) -> Result<String, String> {
    let action = parsed.positional.get(2).map(String::as_str).unwrap_or("");
    let workflow = parsed.positional.get(3).ok_or_else(|| tr!("缺少工作流 id"))?;
    let mut def = store::draft(db, workflow)?;
    let result = match action {
        "add" => {
            let name = parsed.one("name").ok_or_else(|| tr!("缺少 --name"))?.trim().to_string();
            if !model::is_valid_name(&name) {
                return Err(tr!("入参名称不能为空"));
            }
            if def.inputs.iter().any(|field| field.name == name) {
                return Err(tr!("入参 {name} 已经存在", name = name));
            }
            let primary = parsed.boolean("primary")?.unwrap_or(false);
            if primary {
                for field in &mut def.inputs {
                    field.primary = false;
                }
            }
            let field = InputField {
                name: name.clone(),
                label: parsed.one("label").map(str::to_string),
                required: parsed.boolean("required")?.unwrap_or(true),
                primary,
                default: parsed.one("default").map(|text| json!(text)),
            };
            let shown = serde_json::to_value(&field).unwrap_or(json!({ "name": name }));
            def.inputs.push(field);
            shown
        }
        "set" => {
            let name = parsed.positional.get(4).map(String::as_str).ok_or_else(|| tr!("缺少入参名称"))?;
            let primary = parsed.boolean("primary")?;
            let shown = {
                let field = def.inputs.iter_mut().find(|field| field.name == name).ok_or_else(|| tr!("入参不存在"))?;
                if let Some(label) = parsed.one("label") {
                    field.label = Some(label.to_string());
                }
                if let Some(required) = parsed.boolean("required")? {
                    field.required = required;
                }
                if let Some(primary) = primary {
                    field.primary = primary;
                }
                if let Some(default) = parsed.one("default") {
                    field.default = if default.is_empty() { None } else { Some(json!(default)) };
                }
                serde_json::to_value(&field).unwrap_or(json!({ "name": name }))
            };
            if primary == Some(true) {
                for other in &mut def.inputs {
                    if other.name != name {
                        other.primary = false;
                    }
                }
            }
            shown
        }
        "remove" => {
            let name = parsed.positional.get(4).map(String::as_str).ok_or_else(|| tr!("缺少入参名称"))?;
            if !def.inputs.iter().any(|field| field.name == name) {
                return Err(tr!("入参不存在"));
            }
            def.inputs.retain(|field| field.name != name);
            json!({ "removed": name })
        }
        _ => return Err(tr!("用法：multara workflow input add|set|remove <工作流 id>")),
    };
    save(db, workflow, &def, now)?;
    Ok(pretty(&result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn edits_a_workflow_draft() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        let created: Value = serde_json::from_str(&execute(&words("workflow create --name 演示流程"), &mut db).unwrap()).unwrap();
        let id = created["id"].as_str().unwrap();
        execute(&words(&format!("workflow node add {id} --kind agent --title 回答 --prompt 回答问题")), &mut db).unwrap();
        execute(&words(&format!("workflow edge add {id} --from start --to agent")), &mut db).unwrap();
        execute(&words(&format!("workflow input add {id} --name topic --label 主题 --required --default 你好")), &mut db).unwrap();
        let draft: Value = serde_json::from_str(&execute(&words(&format!("workflow draft {id}")), &mut db).unwrap()).unwrap();
        assert_eq!(draft["definition"]["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(draft["definition"]["edges"].as_array().unwrap().len(), 1);
        assert_eq!(draft["definition"]["inputs"][0]["default"], "你好");
        execute(&words(&format!("workflow node remove {id} agent")), &mut db).unwrap();
        let draft: Value = serde_json::from_str(&execute(&words(&format!("workflow draft {id}")), &mut db).unwrap()).unwrap();
        assert_eq!(draft["definition"]["nodes"].as_array().unwrap().len(), 1);
        assert!(draft["definition"]["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn parses_multi_value_flags() {
        let args: Vec<String> = ["node", "output", "shots", "--files", "a.png", "b.png"].iter().map(|s| s.to_string()).collect();
        let parsed = Args::parse(&args);
        assert_eq!(parsed.positional, vec!["node", "output", "shots"]);
        assert_eq!(parsed.many("files"), vec!["a.png", "b.png"]);
        let args: Vec<String> = ["node", "output", "summary", "--text", "-- 列表", "x"].iter().map(|s| s.to_string()).collect();
        let parsed = Args::parse(&args);
        assert_eq!(parsed.one("text"), Some("-- 列表"));
        assert_eq!(parsed.positional.last().map(String::as_str), Some("x"));
    }
}
