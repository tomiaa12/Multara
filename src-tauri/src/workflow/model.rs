//! 工作流定义。一条流程是开始、若干智能体、可选的人工审批，然后结束。
//! 步骤之间不声明端口。上一步的正文由引擎交给下一步。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Trigger,
    Output,
    Agent,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Trigger => "trigger",
            NodeKind::Output => "output",
            NodeKind::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAccess {
    Read,
    Write,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceUse {
    pub slot: String,
    pub access: WorkspaceAccess,
    #[serde(default)]
    pub fork: bool,
}

/// 开始节点收集的一项。名字只出现在开始表单和触发入参里。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputField {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// 集成消息触发时，消息正文填到这一项
    #[serde(default)]
    pub primary: bool,
    /// 没填写时使用。必填项有默认值也可以直接开跑。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDef {
    pub id: String,
    pub kind: NodeKind,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceUse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_minutes: Option<u32>,
    #[serde(default)]
    pub position: Value,
}

/// 连线只表示「接下来去哪」。`when` 是人工审批的决定。`maxIterations` 表示这是一条回边。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeDef {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
}

impl EdgeDef {
    pub fn is_back(&self) -> bool {
        self.max_iterations.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    WorktreeNew,
    WorktreeExisting,
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSlotDef {
    pub key: String,
    #[serde(default)]
    pub label: String,
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    pub isolation: Isolation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default = "default_true")]
    pub auto_commit: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Concurrency {
    #[default]
    Allow,
    SkipIfRunning,
    Queue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSettings {
    #[serde(default = "default_owner")]
    pub owner_agent_id: String,
    #[serde(default = "default_parallel")]
    pub max_parallel_nodes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_timeout_minutes: Option<u32>,
    #[serde(default)]
    pub concurrency: Concurrency,
}

fn default_owner() -> String {
    "miku".into()
}

fn default_parallel() -> u32 {
    4
}

impl Default for WorkflowSettings {
    fn default() -> Self {
        WorkflowSettings {
            owner_agent_id: default_owner(),
            max_parallel_nodes: default_parallel(),
            run_timeout_minutes: None,
            concurrency: Concurrency::Allow,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDef {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub inputs: Vec<InputField>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceSlotDef>,
    #[serde(default)]
    pub nodes: Vec<NodeDef>,
    #[serde(default)]
    pub edges: Vec<EdgeDef>,
    #[serde(default)]
    pub settings: WorkflowSettings,
}

fn default_schema_version() -> u32 {
    2
}

impl Default for WorkflowDef {
    fn default() -> Self {
        WorkflowDef {
            schema_version: 2,
            inputs: Vec::new(),
            workspaces: Vec::new(),
            nodes: vec![NodeDef {
                id: "start".into(),
                kind: NodeKind::Trigger,
                title: "开始".into(),
                description: None,
                config: json!({ "sources": ["manual"] }),
                workspaces: Vec::new(),
                timeout_minutes: None,
                position: json!({ "x": 80, "y": 200 }),
            }],
            edges: Vec::new(),
            settings: WorkflowSettings::default(),
        }
    }
}

impl WorkflowDef {
    pub fn node(&self, id: &str) -> Option<&NodeDef> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn trigger(&self) -> Option<&NodeDef> {
        self.nodes.iter().find(|node| node.kind == NodeKind::Trigger)
    }

    pub fn slot(&self, key: &str) -> Option<&WorkspaceSlotDef> {
        self.workspaces.iter().find(|slot| slot.key == key)
    }

    pub fn outgoing<'a>(&'a self, id: &str) -> Vec<&'a EdgeDef> {
        self.edges.iter().filter(|edge| edge.source == id).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TargetRef {
    #[serde(rename = "type")]
    pub target_type: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    #[serde(default)]
    pub target: TargetRef,
    #[serde(default)]
    pub prompt: String,
    #[serde(default = "default_auto")]
    pub mode: String,
    /// `previous` 只带上一步正文；`all` 带上从开始到现在的每一步正文
    #[serde(default = "default_previous")]
    pub context: String,
    /// 多条出线时 `all` 都送下去，`auto` 由智能体选，`human` 暂停等人选。只有一条出线时忽略。
    #[serde(default = "default_auto")]
    pub route: String,
    /// 做完后停住，等人确认才进入下一步。
    #[serde(default)]
    pub human_confirm: bool,
    #[serde(default)]
    pub allow_task_commands: bool,
}

fn default_auto() -> String {
    "auto".into()
}

fn default_previous() -> String {
    "previous".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerConfig {
    #[serde(default = "default_sources")]
    pub sources: Vec<String>,
}

fn default_sources() -> Vec<String> {
    vec!["manual".into()]
}

pub fn config<T: serde::de::DeserializeOwned>(node: &NodeDef) -> Result<T, String> {
    let raw = if node.config.is_null() { json!({}) } else { node.config.clone() };
    serde_json::from_value(raw).map_err(|error| {
        tr!("节点 {node} 的配置无效：{error}", node = node.id, error = error)
    })
}

pub fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && id.len() <= 48
}

pub fn is_valid_name(name: &str) -> bool {
    !name.trim().is_empty()
}

/// 工作流名转成分支名可用的片段
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "flow".into()
    } else {
        trimmed.chars().take(32).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_definition_with_defaults() {
        let def: WorkflowDef = serde_json::from_value(json!({
            "nodes": [{ "id": "start", "kind": "trigger" }],
            "edges": []
        }))
        .unwrap();
        assert_eq!(def.settings.max_parallel_nodes, 4);
        assert!(def.trigger().is_some());
        assert!(def.nodes[0].timeout_minutes.is_none());
    }

    #[test]
    fn ids_and_slugs() {
        assert!(is_valid_id("code_2"));
        assert!(!is_valid_id("2code"));
        assert!(!is_valid_id("Code"));
        assert!(is_valid_name("需求说明"));
        assert!(!is_valid_name("  "));
        assert_eq!(slugify("需求 to Release!"), "to-release");
        assert_eq!(slugify("需求"), "flow");
    }
}
