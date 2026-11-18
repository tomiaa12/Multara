//! 内置节点模板与工作流模板（技术方案第 12 节），以及用户保存的节点模板。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::{EdgeDef, NodeDef, NodeKind, WorkflowDef};

const NODES: &str = include_str!("templates/nodes.json");
const FLOWS: &str = include_str!("templates/flows.json");

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

fn localize_node(node: &mut NodeDef) {
    node.title = localize(&node.title);
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

pub fn workflow_templates(_db: &Connection) -> Vec<WorkflowTemplateMeta> {
    flows()
        .into_iter()
        .map(|flow| WorkflowTemplateMeta {
            id: flow.id,
            name: localize(&flow.name),
            description: localize(&flow.description),
            node_count: flow.definition.nodes.len(),
        })
        .collect()
}

pub fn workflow_template(id: &str, db: &Connection) -> Result<WorkflowTemplate, String> {
    let flow = flows()
        .into_iter()
        .find(|flow| flow.id == id)
        .ok_or_else(|| tr!("模板不存在"))?;
    let mut definition = flow.definition;
    for node in &mut definition.nodes {
        localize_node(node);
    }
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
            node.config["target"] = serde_json::json!({ "type": "agent", "id": agent });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::validate::{tests::FakeLookup, validate};

    #[test]
    fn builtin_templates_parse() {
        let nodes = builtin_node_templates();
        assert!(nodes.len() >= 10);
        assert!(nodes.iter().all(|item| !item.nodes.is_empty()));
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
}
