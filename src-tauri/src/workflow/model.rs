//! 工作流定义（DSL）。与前端 `src/domain/workflow.ts` 一一对应，序列化格式见 docs/工作流节点技术方案.md 第 2 节。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortType {
    Text,
    Markdown,
    Number,
    Boolean,
    Json,
    Url,
    File,
    Files,
    Any,
}

impl PortType {
    pub fn as_str(self) -> &'static str {
        match self {
            PortType::Text => "text",
            PortType::Markdown => "markdown",
            PortType::Number => "number",
            PortType::Boolean => "boolean",
            PortType::Json => "json",
            PortType::Url => "url",
            PortType::File => "file",
            PortType::Files => "files",
            PortType::Any => "any",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortDef {
    pub name: String,
    #[serde(rename = "type")]
    pub port_type: PortType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(default)]
    pub multi: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// 渠道消息触发时，消息正文填到这个入参
    #[serde(default)]
    pub primary: bool,
}

impl PortDef {
    pub fn new(name: &str, port_type: PortType) -> Self {
        PortDef {
            name: name.to_string(),
            port_type,
            label: None,
            description: None,
            required: false,
            schema: None,
            multi: false,
            default: None,
            primary: false,
        }
    }

    fn multi(mut self) -> Self {
        self.multi = true;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Trigger,
    Output,
    Agent,
    Script,
    Gate,
    Switch,
    Transform,
    Http,
    Git,
    Notify,
    Subflow,
    Map,
    Note,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Trigger => "trigger",
            NodeKind::Output => "output",
            NodeKind::Agent => "agent",
            NodeKind::Script => "script",
            NodeKind::Gate => "gate",
            NodeKind::Switch => "switch",
            NodeKind::Transform => "transform",
            NodeKind::Http => "http",
            NodeKind::Git => "git",
            NodeKind::Notify => "notify",
            NodeKind::Subflow => "subflow",
            NodeKind::Map => "map",
            NodeKind::Note => "note",
        }
    }

    /// 只有分支类节点的分支端口会「只触发其中一部分」
    pub fn is_branching(self) -> bool {
        matches!(self, NodeKind::Switch | NodeKind::Gate)
    }

    pub fn default_timeout_minutes(self) -> Option<u32> {
        match self {
            NodeKind::Agent => Some(120),
            NodeKind::Script => Some(30),
            NodeKind::Http | NodeKind::Notify => Some(2),
            NodeKind::Git => Some(10),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JoinMode {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    #[default]
    FailRun,
    Continue,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    pub max_attempts: u32,
    #[serde(default)]
    pub backoff_seconds: u32,
    #[serde(default = "default_retry_on")]
    pub on: Vec<String>,
}

fn default_retry_on() -> Vec<String> {
    vec!["failed".into(), "timeout".into()]
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
    pub inputs: Vec<PortDef>,
    #[serde(default)]
    pub outputs: Vec<PortDef>,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceUse>,
    #[serde(default)]
    pub join: JoinMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_minutes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default)]
    pub on_error: OnError,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(default)]
    pub position: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeDef {
    pub id: String,
    pub source: String,
    pub source_port: String,
    pub target: String,
    pub target_port: String,
    /// 有值即为回边（循环），只能从 switch / gate 指向祖先节点
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
    /// 需求 id，或模板，如 `{{ run.inputs.project }}`
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
    /// 子流程：继承父运行同名（或指定名）槽位，不另建工作区
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherit: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Concurrency {
    Allow,
    SkipIfRunning,
    #[default]
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
            concurrency: Concurrency::Queue,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDef {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub inputs: Vec<PortDef>,
    #[serde(default)]
    pub outputs: Vec<PortDef>,
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
    1
}

impl Default for WorkflowDef {
    fn default() -> Self {
        WorkflowDef {
            schema_version: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            workspaces: Vec::new(),
            nodes: vec![NodeDef {
                id: "start".into(),
                kind: NodeKind::Trigger,
                title: String::new(),
                description: None,
                inputs: Vec::new(),
                outputs: Vec::new(),
                config: json!({ "sources": ["manual"] }),
                workspaces: Vec::new(),
                join: JoinMode::All,
                timeout_minutes: None,
                retry: None,
                on_error: OnError::FailRun,
                template_id: None,
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
}

// ---------- 各类型配置 ----------

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
    #[serde(default = "default_repair")]
    pub max_repair_turns: u32,
    #[serde(default = "default_same_node")]
    pub continue_session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_text_port: Option<String>,
    #[serde(default)]
    pub allow_task_commands: bool,
}

fn default_auto() -> String {
    "auto".into()
}
fn default_repair() -> u32 {
    2
}
fn default_same_node() -> String {
    "same_node".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CwdRef {
    pub slot: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptConfig {
    #[serde(default)]
    pub command: String,
    #[serde(default = "default_shell")]
    pub shell: String,
    #[serde(default)]
    pub cwd: Option<CwdRef>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_success_codes")]
    pub success_exit_codes: Vec<i32>,
    #[serde(default = "default_true")]
    pub fail_on_non_zero: bool,
    #[serde(default = "default_parse")]
    pub parse_stdout: String,
    #[serde(default = "default_output_kb")]
    pub max_output_kb: u32,
}

fn default_shell() -> String {
    "default".into()
}
fn default_success_codes() -> Vec<i32> {
    vec![0]
}
fn default_parse() -> String {
    "none".into()
}
fn default_output_kb() -> u32 {
    256
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateDecision {
    pub port: String,
    pub label: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub require_comment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateConfig {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub decisions: Vec<GateDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<Value>,
    #[serde(default)]
    pub show_diff_of: Vec<String>,
    #[serde(default)]
    pub notify: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_timeout: Option<String>,
}

impl GateConfig {
    pub fn effective_decisions(&self) -> Vec<GateDecision> {
        if !self.decisions.is_empty() {
            return self.decisions.clone();
        }
        vec![
            GateDecision {
                port: "approve".into(),
                label: tr!("通过"),
                primary: true,
                require_comment: false,
            },
            GateDecision {
                port: "changes".into(),
                label: tr!("打回修改"),
                primary: false,
                require_comment: true,
            },
            GateDecision {
                port: "reject".into(),
                label: tr!("拒绝"),
                primary: false,
                require_comment: false,
            },
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchCase {
    pub port: String,
    pub when: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchConfig {
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub cases: Vec<SwitchCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_port: Option<String>,
    #[serde(default = "default_first")]
    pub mode: String,
}

fn default_first() -> String {
    "first".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub port: String,
    pub expr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransformConfig {
    #[serde(default)]
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpConfig {
    #[serde(default = "default_get")]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default = "default_body_type")]
    pub body_type: String,
    #[serde(default)]
    pub expect_status: Vec<u16>,
    #[serde(default = "default_response_type")]
    pub response_type: String,
}

fn default_get() -> String {
    "GET".into()
}
fn default_body_type() -> String {
    "none".into()
}
fn default_response_type() -> String {
    "json".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitConfig {
    #[serde(default)]
    pub slot: String,
    #[serde(default)]
    pub op: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub set_upstream: bool,
    #[serde(default)]
    pub skip_verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyConfig {
    #[serde(default = "default_inbox")]
    pub channel: String,
    #[serde(default)]
    pub text: String,
}

fn default_inbox() -> String {
    "inbox".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubflowConfig {
    #[serde(default)]
    pub workflow_id: String,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub inputs: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapConfig {
    #[serde(default)]
    pub items: String,
    #[serde(default)]
    pub workflow_id: String,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub item_input: String,
    #[serde(default)]
    pub extra_inputs: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_map_concurrency")]
    pub concurrency: u32,
    #[serde(default)]
    pub fail_fast: bool,
}

fn default_map_concurrency() -> u32 {
    2
}

pub fn config<T: serde::de::DeserializeOwned>(node: &NodeDef) -> Result<T, String> {
    let raw = if node.config.is_null() {
        json!({})
    } else {
        node.config.clone()
    };
    serde_json::from_value(raw).map_err(|error| {
        tr!(
            "节点 {node} 的配置无效：{error}",
            node = node.id,
            error = error
        )
    })
}

// ---------- 隐式端口 ----------

pub const IN_PORT: &str = "$in";
pub const OUT_PORT: &str = "$out";
pub const ERROR_PORT: &str = "$error";
pub const RESULT_PORT: &str = "$result";
pub const EXHAUSTED_PORT: &str = "exhausted";

/// 节点实际可连的输入端口（声明的 + 隐式）
pub fn effective_inputs(node: &NodeDef) -> Vec<PortDef> {
    let mut ports = node.inputs.clone();
    if node.kind != NodeKind::Note {
        ports.push(PortDef::new(IN_PORT, PortType::Any).multi());
    }
    ports
}

/// 分支端口：只有被选中的才是活边
pub fn branch_ports(node: &NodeDef) -> Vec<String> {
    match node.kind {
        NodeKind::Switch => {
            let Ok(cfg) = config::<SwitchConfig>(node) else {
                return vec![EXHAUSTED_PORT.into()];
            };
            let mut ports: Vec<String> = cfg.cases.iter().map(|case| case.port.clone()).collect();
            if let Some(default) = cfg.default_port {
                if !ports.contains(&default) {
                    ports.push(default);
                }
            }
            ports.push(EXHAUSTED_PORT.into());
            ports
        }
        NodeKind::Gate => {
            let Ok(cfg) = config::<GateConfig>(node) else {
                return vec![EXHAUSTED_PORT.into()];
            };
            let mut ports: Vec<String> = cfg
                .effective_decisions()
                .into_iter()
                .map(|decision| decision.port)
                .collect();
            ports.push(EXHAUSTED_PORT.into());
            ports
        }
        _ => Vec::new(),
    }
}

/// 节点实际可连的输出端口（声明的 + 隐式），隐式端口不覆盖同名声明
pub fn effective_outputs(node: &NodeDef, workflow: &WorkflowDef) -> Vec<PortDef> {
    let mut ports = match node.kind {
        NodeKind::Trigger => workflow.inputs.clone(),
        _ => node.outputs.clone(),
    };
    let mut add = |name: &str, port_type: PortType| {
        if !ports.iter().any(|port| port.name == name) {
            ports.push(PortDef::new(name, port_type));
        }
    };
    match node.kind {
        NodeKind::Note => return Vec::new(),
        NodeKind::Agent => {
            add("text", PortType::Markdown);
            add("session_id", PortType::Text);
        }
        NodeKind::Script => {
            add("exit_code", PortType::Number);
            add("stdout", PortType::Text);
            add("stderr", PortType::Text);
            add("log", PortType::File);
            add("ok", PortType::Boolean);
            add("parsed", PortType::Any);
            add(RESULT_PORT, PortType::Json);
        }
        NodeKind::Http => {
            add("status", PortType::Number);
            add("headers", PortType::Json);
            add("body", PortType::Any);
            add("ok", PortType::Boolean);
            add(RESULT_PORT, PortType::Json);
        }
        NodeKind::Git => {
            add("head", PortType::Text);
            add("branch", PortType::Text);
            add("diff", PortType::File);
            add("changed_files", PortType::Json);
            add(RESULT_PORT, PortType::Json);
        }
        NodeKind::Gate => {
            add("comment", PortType::Text);
            add("form", PortType::Json);
            add("decision", PortType::Text);
        }
        NodeKind::Map => {
            add("results", PortType::Json);
            add("failed_indexes", PortType::Json);
        }
        NodeKind::Subflow => {
            add("outputs", PortType::Json);
        }
        _ => {}
    }
    if node.workspaces.iter().any(|usage| usage.fork) {
        add("fork_branch", PortType::Text);
    }
    for port in branch_ports(node) {
        let port_type = if node.kind == NodeKind::Gate {
            PortType::Json
        } else {
            PortType::Any
        };
        add(&port, port_type);
    }
    add(OUT_PORT, PortType::Any);
    add(ERROR_PORT, PortType::Json);
    ports
}

pub fn find_port<'a>(ports: &'a [PortDef], name: &str) -> Option<&'a PortDef> {
    ports.iter().find(|port| port.name == name)
}

pub fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && id.len() <= 48
}

pub fn is_valid_port_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 48
}

/// `slug`：工作流名转成分支名可用的片段
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
        assert_eq!(def.nodes[0].join, JoinMode::All);
        assert!(def.trigger().is_some());
    }

    #[test]
    fn implicit_ports_for_switch_and_script() {
        let def = WorkflowDef::default();
        let switch: NodeDef = serde_json::from_value(json!({
            "id": "check", "kind": "switch",
            "config": { "cases": [{ "port": "pass", "when": "true" }], "defaultPort": "fail" }
        }))
        .unwrap();
        let names: Vec<String> = effective_outputs(&switch, &def)
            .into_iter()
            .map(|port| port.name)
            .collect();
        assert!(names.contains(&"pass".to_string()));
        assert!(names.contains(&"fail".to_string()));
        assert!(names.contains(&"exhausted".to_string()));
        let script: NodeDef =
            serde_json::from_value(json!({ "id": "test", "kind": "script" })).unwrap();
        let names: Vec<String> = effective_outputs(&script, &def)
            .into_iter()
            .map(|port| port.name)
            .collect();
        assert!(names.contains(&"$result".to_string()));
        assert!(names.contains(&"exit_code".to_string()));
    }

    #[test]
    fn ids_and_slugs() {
        assert!(is_valid_id("code_2"));
        assert!(!is_valid_id("2code"));
        assert!(!is_valid_id("Code"));
        assert!(is_valid_port_name("figma_links"));
        assert_eq!(slugify("需求 to Release!"), "to-release");
        assert_eq!(slugify("需求"), "flow");
    }
}
