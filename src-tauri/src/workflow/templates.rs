//! 内置节点模板与工作流模板（技术方案第 12 节），以及用户保存的节点模板。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::model::{EdgeDef, NodeDef, NodeKind, WorkflowDef};
use super::store;

const NODES: &str = include_str!("templates/nodes.json");
const FLOWS: &str = include_str!("templates/flows.json");
const EXAMPLES: &str = include_str!("templates/examples.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTemplate {
    pub id: String,
    #[serde(default)]
    pub category: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub recommends: Value,
    pub nodes: Vec<NodeDef>,
    #[serde(default)]
    pub edges: Vec<EdgeDef>,
    #[serde(default)]
    pub builtin: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowTemplate {
    id: String,
    name: String,
    description: String,
    definition: WorkflowDef,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTemplateMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub node_count: usize,
}

pub struct WorkflowTemplate {
    pub definition: WorkflowDef,
}

fn localize(text: &str) -> String {
    crate::i18n::tr(text)
}

fn localize_opt(text: &Option<String>) -> Option<String> {
    text.as_ref().map(|value| localize(value))
}

fn localize_config_string(config: &mut Value, key: &str) {
    let Some(Value::String(text)) = config.get(key).cloned() else { return };
    if text.trim().is_empty() {
        return;
    }
    config[key] = Value::String(localize(&text));
}

fn localize_node(node: &mut NodeDef) {
    node.title = localize(&node.title);
    node.description = localize_opt(&node.description);
    // 关卡决策文案、通知正文、关卡标题等
    localize_config_string(&mut node.config, "title");
    localize_config_string(&mut node.config, "body");
    localize_config_string(&mut node.config, "text");
    if let Some(Value::Array(decisions)) = node.config.get_mut("decisions") {
        for decision in decisions {
            localize_config_string(decision, "label");
        }
    }
}

fn localize_definition(definition: &mut WorkflowDef) {
    for field in &mut definition.inputs {
        if let Some(label) = field.label.take() {
            field.label = Some(localize(&label));
        }
    }
    for slot in &mut definition.workspaces {
        slot.label = localize(&slot.label);
    }
    for node in &mut definition.nodes {
        localize_node(node);
    }
}

pub fn builtin_node_templates() -> Vec<NodeTemplate> {
    let mut items: Vec<NodeTemplate> = serde_json::from_str(NODES).expect("builtin node templates");
    for item in &mut items {
        item.builtin = true;
        item.name = localize(&item.name);
        item.description = localize(&item.description);
        for node in &mut item.nodes {
            localize_node(node);
        }
    }
    items
}

pub fn node_templates(db: &Connection) -> Result<Vec<NodeTemplate>, String> {
    let mut items = builtin_node_templates();
    let default_agent = default_agent(db);
    for item in &mut items {
        fill_agents(&mut item.nodes, default_agent.as_deref());
    }
    let mut statement = db
        .prepare("SELECT id, name, description, template_json FROM workflow_node_templates ORDER BY created_at DESC")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (id, name, description, raw) = row.map_err(|error| error.to_string())?;
        let Ok(body) = serde_json::from_str::<Value>(&raw) else { continue };
        let nodes: Vec<NodeDef> = serde_json::from_value(body["nodes"].clone()).unwrap_or_default();
        let edges: Vec<EdgeDef> = serde_json::from_value(body["edges"].clone()).unwrap_or_default();
        if nodes.is_empty() {
            continue;
        }
        items.push(NodeTemplate {
            id,
            category: "mine".into(),
            name,
            description,
            recommends: Value::Null,
            nodes,
            edges,
            builtin: false,
        });
    }
    Ok(items)
}

pub fn save_user_template(db: &Connection, name: &str, description: &str, template: &Value, now: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(tr!("模板名称不能为空"));
    }
    let nodes: Vec<NodeDef> = serde_json::from_value(template["nodes"].clone())
        .map_err(|error| tr!("模板内容无效：{error}", error = error))?;
    if nodes.is_empty() || nodes.iter().any(|node| node.kind == NodeKind::Trigger) {
        return Err(tr!("模板至少要有一个节点，且不能包含触发器"));
    }
    db.execute(
        "INSERT INTO workflow_node_templates (id, name, description, template_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![uuid::Uuid::new_v4().to_string(), name.trim(), description.trim(), template.to_string(), now],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn flows() -> Vec<FlowTemplate> {
    serde_json::from_str(FLOWS).expect("builtin workflow templates")
}

fn examples() -> Vec<FlowTemplate> {
    serde_json::from_str(EXAMPLES).expect("example workflow templates")
}

pub fn workflow_templates(_db: &Connection) -> Vec<WorkflowTemplateMeta> {
    flows()
        .into_iter()
        .chain(examples())
        .map(|flow| WorkflowTemplateMeta {
            id: flow.id,
            name: localize(&flow.name),
            description: localize(&flow.description),
            node_count: flow.definition.nodes.len(),
        })
        .collect()
}

fn find_flow(id: &str) -> Option<FlowTemplate> {
    flows().into_iter().chain(examples()).find(|flow| flow.id == id)
}

pub fn workflow_template(id: &str, db: &Connection) -> Result<WorkflowTemplate, String> {
    let flow = find_flow(id).ok_or_else(|| tr!("模板不存在"))?;
    let mut definition = flow.definition;
    localize_definition(&mut definition);
    let default_agent = default_agent(db);
    fill_agents(&mut definition.nodes, default_agent.as_deref());
    if let Some(agent) = &default_agent {
        let exists: bool = db
            .query_row("SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1)", [&definition.settings.owner_agent_id], |row| row.get(0))
            .unwrap_or(false);
        if !exists {
            definition.settings.owner_agent_id = agent.clone();
        }
    }
    Ok(WorkflowTemplate { definition })
}

const INSTALLED_EXAMPLES_KEY: &str = "workflowInstalledExamples";

fn example_workflow_id(example_id: &str) -> String {
    example_id.to_string()
}

fn installed_example_ids(db: &Connection) -> Vec<String> {
    let raw: String = db
        .query_row(
            "SELECT value FROM app_settings WHERE key=?1",
            [INSTALLED_EXAMPLES_KEY],
            |row| row.get(0),
        )
        .unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_installed_example_ids(db: &Connection, ids: &[String]) -> Result<(), String> {
    let value = serde_json::to_string(ids).map_err(|error| error.to_string())?;
    db.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![INSTALLED_EXAMPLES_KEY, value],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// 开发模式：删掉现有示例工作流后按内置 JSON 重新写入（用户改过的示例也会被覆盖）。
pub fn reset_example_workflows(db: &Connection, now: &str) -> Result<(), String> {
    for example in examples() {
        let workflow_id = example_workflow_id(&example.id);
        let active: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE workflow_id=?1 AND status IN ('queued','running','waiting'))",
                [&workflow_id],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if active {
            return Err(tr!("工作流还有运行没结束，请先取消"));
        }
    }
    for example in examples() {
        let workflow_id = example_workflow_id(&example.id);
        store::delete_workflow(db, &workflow_id)?;
    }
    db.execute("DELETE FROM app_settings WHERE key=?1", [INSTALLED_EXAMPLES_KEY])
        .map_err(|error| error.to_string())?;
    let _ = db.execute("DELETE FROM app_settings WHERE key='workflowExamplesSeeded'", []);
    seed_example_workflows(db, now)
}

/// 把简单 / 中等 / 复杂三套示例写入工作流列表，方便学习。
/// 按示例 id 记录「已经送过」；用户删掉后不会再塞回。升级用户若之前被旧逻辑跳过，会在此补种。
pub fn seed_example_workflows(db: &Connection, now: &str) -> Result<(), String> {
    let mut installed = installed_example_ids(db);
    let mut changed = false;
    for example in examples() {
        if installed.iter().any(|id| id == &example.id) {
            continue;
        }
        let workflow_id = example_workflow_id(&example.id);
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workflows WHERE id=?1)",
                [&workflow_id],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !exists {
            let mut definition = example.definition;
            localize_definition(&mut definition);
            fill_agents(&mut definition.nodes, default_agent(db).as_deref());
            if definition.nodes[0].title.trim().is_empty() {
                definition.nodes[0].title = tr!("开始");
            }
            let name = localize(&example.name);
            let description = localize(&example.description);
            let slug = example.id.replace('.', "-");
            let raw = serde_json::to_string(&definition).map_err(|error| error.to_string())?;
            db.execute(
                "INSERT INTO workflows
                 (id, name, slug, description, icon, draft_json, draft_hash, draft_updated_at, enabled, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7, 1, ?7, ?7)",
                params![
                    workflow_id,
                    name,
                    slug,
                    description,
                    raw,
                    store::hash_definition(&definition),
                    now
                ],
            )
            .map_err(|error| error.to_string())?;
        }
        installed.push(example.id.clone());
        changed = true;
    }
    if changed {
        save_installed_example_ids(db, &installed)?;
    }
    Ok(())
}

fn default_agent(db: &Connection) -> Option<String> {
    db.query_row(
        "SELECT id FROM agents ORDER BY CASE WHEN system_key='miku' THEN 0 ELSE 1 END, rowid LIMIT 1",
        [],
        |row| row.get(0),
    )
    .ok()
}

fn fill_agents(nodes: &mut [NodeDef], agent: Option<&str>) {
    let Some(agent) = agent else { return };
    for node in nodes.iter_mut().filter(|node| node.kind == NodeKind::Agent) {
        if node.config["target"]["id"].as_str().unwrap_or("").is_empty() {
            node.config["target"] = json!({ "type": "agent", "id": agent });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::validate::{validate, Lookup};

    struct FakeLookup;
    impl Lookup for FakeLookup {
        fn agent_exists(&self, id: &str) -> bool { id == "coder" || id == "miku" }
        fn squad_exists(&self, _id: &str) -> bool { false }
        fn published(&self, _workflow_id: &str, _version: Option<u32>) -> Option<crate::workflow::model::WorkflowDef> { None }
        fn secret_exists(&self, _name: &str) -> bool { false }
    }

    #[test]
    fn builtin_templates_parse() {
        let nodes = builtin_node_templates();
        assert!(nodes.len() >= 1);
        assert!(nodes.iter().all(|item| !item.nodes.is_empty()));
    }

    #[test]
    fn example_templates_parse_and_validate() {
        let items = examples();
        assert_eq!(items.len(), 2);
        for flow in items {
            let mut def = flow.definition;
            fill_agents(&mut def.nodes, Some("coder"));
            let errors: Vec<_> = validate(&def, &FakeLookup)
                .into_iter()
                .filter(|item| item.severity == "error")
                .collect();
            assert!(errors.is_empty(), "{}: {:?}", flow.id, errors);
        }
    }

    #[test]
    fn builtin_flows_validate_without_errors() {
        for flow in flows() {
            let mut def = flow.definition;
            fill_agents(&mut def.nodes, Some("coder"));
            let errors: Vec<_> = validate(&def, &FakeLookup)
                .into_iter()
                .filter(|item| item.severity == "error")
                .collect();
            assert!(errors.is_empty(), "{}: {:?}", flow.id, errors);
        }
    }

    #[test]
    fn seed_example_workflows_is_idempotent() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        crate::database::seed(&db).unwrap();
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM workflows", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        crate::database::seed(&db).unwrap();
        let again: u32 = db
            .query_row("SELECT COUNT(*) FROM workflows", [], |row| row.get(0))
            .unwrap();
        assert_eq!(again, 2);
        let names: Vec<String> = db
            .prepare("SELECT name FROM workflows ORDER BY created_at")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(names.iter().any(|name| name.contains("简单") || name.contains("Simple")));
    }

    #[test]
    fn seed_examples_even_when_other_workflows_exist() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        crate::database::seed(&db).unwrap();
        // 模拟旧逻辑：只打了标记、库里已有别的工作流、示例没写上
        db.execute("DELETE FROM workflows", []).unwrap();
        db.execute("DELETE FROM app_settings WHERE key=?1", [INSTALLED_EXAMPLES_KEY])
            .unwrap();
        db.execute(
            "INSERT INTO app_settings (key, value) VALUES ('workflowExamplesSeeded', '1')
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [],
        )
        .unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        store::create_workflow(
            &db,
            "我的流程",
            "",
            &WorkflowDef::default(),
            &now,
        )
        .unwrap();
        seed_example_workflows(&db, &now).unwrap();
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM workflows", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
        let exists: bool = db
            .query_row("SELECT EXISTS(SELECT 1 FROM workflows WHERE id='example.simple')", [], |row| row.get(0))
            .unwrap();
        assert!(exists);
        // 用户删掉示例后不再塞回
        db.execute("DELETE FROM workflows WHERE id LIKE 'example.%'", []).unwrap();
        seed_example_workflows(&db, &now).unwrap();
        let examples: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM workflows WHERE id LIKE 'example.%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(examples, 0);
    }

    #[test]
    fn reset_example_workflows_restores_builtin() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        crate::database::seed(&db).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        db.execute(
            "UPDATE workflows SET name='改过的示例' WHERE id='example.simple'",
            [],
        )
        .unwrap();
        reset_example_workflows(&db, &now).unwrap();
        let name: String = db
            .query_row("SELECT name FROM workflows WHERE id='example.simple'", [], |row| row.get(0))
            .unwrap();
        assert!(name.contains("简单") || name.contains("Simple"), "{name}");
        let count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM workflows WHERE id LIKE 'example.%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
}
