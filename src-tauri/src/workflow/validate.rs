//! 静态校验（技术方案第 10 节）。保存草稿时返回诊断，发布时 error 级阻止发布。

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::Serialize;

use super::graph::Graph;
use super::model::*;
use super::template;
use super::values::compatible;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: &'static str,
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
    pub message: String,
}

/// 校验时需要查库的部分，测试里用假实现
pub trait Lookup {
    fn agent_exists(&self, id: &str) -> bool;
    fn squad_exists(&self, id: &str) -> bool;
    /// 子流程的已发布定义（version None = 最新）
    fn published(&self, workflow_id: &str, version: Option<u32>) -> Option<WorkflowDef>;
    fn secret_exists(&self, name: &str) -> bool;
}

struct Out {
    items: Vec<Diagnostic>,
}

impl Out {
    fn push(&mut self, severity: &'static str, code: &'static str, node: Option<&str>, edge: Option<&str>, message: String) {
        self.items.push(Diagnostic {
            severity,
            code,
            node_id: node.map(str::to_string),
            edge_id: edge.map(str::to_string),
            message,
        });
    }
    fn error(&mut self, code: &'static str, node: Option<&str>, edge: Option<&str>, message: String) {
        self.push("error", code, node, edge, message);
    }
    fn warn(&mut self, code: &'static str, node: Option<&str>, edge: Option<&str>, message: String) {
        self.push("warning", code, node, edge, message);
    }
}

pub fn has_errors(items: &[Diagnostic]) -> bool {
    items.iter().any(|item| item.severity == "error")
}

/// 节点里所有模板字段：(字段名, 源码, 是否表达式, 是否允许 secret)
pub fn template_fields(node: &NodeDef) -> Vec<(String, String, bool, bool)> {
    let mut fields = Vec::new();
    let mut text = |name: &str, value: &str, expr: bool, secret: bool| {
        if !value.trim().is_empty() {
            fields.push((name.to_string(), value.to_string(), expr, secret));
        }
    };
    match node.kind {
        NodeKind::Agent => {
            if let Ok(cfg) = config::<AgentConfig>(node) {
                text("prompt", &cfg.prompt, false, false);
            }
        }
        NodeKind::Script => {
            if let Ok(cfg) = config::<ScriptConfig>(node) {
                text("command", &cfg.command, false, false);
                for (key, value) in &cfg.env {
                    text(&format!("env.{key}"), value, false, true);
                }
            }
        }
        NodeKind::Gate => {
            if let Ok(cfg) = config::<GateConfig>(node) {
                text("title", &cfg.title, false, false);
                text("body", &cfg.body, false, false);
            }
        }
        NodeKind::Switch => {
            if let Ok(cfg) = config::<SwitchConfig>(node) {
                if let Some(value) = &cfg.value {
                    text("value", value, true, false);
                }
                for case in &cfg.cases {
                    text(&format!("cases.{}", case.port), &case.when, true, false);
                }
            }
        }
        NodeKind::Transform => {
            if let Ok(cfg) = config::<TransformConfig>(node) {
                for item in &cfg.assignments {
                    text(&format!("assignments.{}", item.port), &item.expr, true, false);
                }
            }
        }
        NodeKind::Http => {
            if let Ok(cfg) = config::<HttpConfig>(node) {
                text("url", &cfg.url, false, true);
                for (key, value) in &cfg.headers {
                    text(&format!("headers.{key}"), value, false, true);
                }
                if let Some(body) = &cfg.body {
                    text("body", body, false, true);
                }
            }
        }
        NodeKind::Git => {
            if let Ok(cfg) = config::<GitConfig>(node) {
                if let Some(message) = &cfg.message {
                    text("message", message, false, false);
                }
                if let Some(branch) = &cfg.branch {
                    text("branch", branch, false, false);
                }
            }
        }
        NodeKind::Notify => {
            if let Ok(cfg) = config::<NotifyConfig>(node) {
                text("text", &cfg.text, false, false);
            }
        }
        NodeKind::Subflow => {
            if let Ok(cfg) = config::<SubflowConfig>(node) {
                for (key, value) in &cfg.inputs {
                    text(&format!("inputs.{key}"), value, false, false);
                }
            }
        }
        NodeKind::Map => {
            if let Ok(cfg) = config::<MapConfig>(node) {
                text("items", &cfg.items, true, false);
                for (key, value) in &cfg.extra_inputs {
                    text(&format!("extraInputs.{key}"), value, false, false);
                }
            }
        }
        _ => {}
    }
    fields
}

const ROOTS: &[&str] = &[
    "inputs", "nodes", "run", "workflow", "ws", "iteration", "attempt", "item", "index", "parent",
    "true", "false", "none", "loop", "range", "namespace", "dict", "debug", "lipsum", "cycler",
    "joiner", "secret",
];

pub fn validate(def: &WorkflowDef, lookup: &dyn Lookup) -> Vec<Diagnostic> {
    let mut out = Out { items: Vec::new() };

    // id 与端口名
    let mut seen = HashSet::new();
    for node in &def.nodes {
        if !is_valid_id(&node.id) {
            out.error("id_invalid", Some(&node.id), None, tr!("节点 id「{id}」只能用小写字母、数字和下划线，并以字母开头", id = node.id));
        }
        if !seen.insert(node.id.as_str()) {
            out.error("id_duplicate", Some(&node.id), None, tr!("节点 id「{id}」重复", id = node.id));
        }
        let mut names = HashSet::new();
        for port in node.inputs.iter().chain(node.outputs.iter()) {
            if !is_valid_port_name(&port.name) {
                out.error("id_invalid", Some(&node.id), None, tr!("端口名「{name}」无效", name = port.name));
            }
            let _ = names.insert(port.name.as_str());
        }
        let mut input_names = HashSet::new();
        for port in &node.inputs {
            if !input_names.insert(port.name.as_str()) {
                out.error("id_duplicate", Some(&node.id), None, tr!("端口名「{name}」重复", name = port.name));
            }
        }
        let mut output_names = HashSet::new();
        for port in &node.outputs {
            if !output_names.insert(port.name.as_str()) {
                out.error("id_duplicate", Some(&node.id), None, tr!("端口名「{name}」重复", name = port.name));
            }
        }
    }
    for port in &def.inputs {
        if !is_valid_port_name(&port.name) {
            out.error("id_invalid", None, None, tr!("端口名「{name}」无效", name = port.name));
        }
    }

    let triggers = def.nodes.iter().filter(|node| node.kind == NodeKind::Trigger).count();
    if triggers != 1 {
        out.error("trigger_count", None, None, tr!("工作流必须有且只有一个触发器节点"));
    }

    let mut edge_ids = HashSet::new();
    for edge in &def.edges {
        if !edge_ids.insert(edge.id.as_str()) {
            out.error("id_duplicate", None, Some(&edge.id), tr!("连线 id「{id}」重复", id = edge.id));
        }
    }

    let graph = Graph::new(def);

    // 连线
    let mut input_edge_count: HashMap<(&str, &str), usize> = HashMap::new();
    for edge in &def.edges {
        let (Some(source), Some(target)) = (def.node(&edge.source), def.node(&edge.target)) else {
            out.error("edge_dangling", None, Some(&edge.id), tr!("连线指向不存在的节点"));
            continue;
        };
        if source.kind == NodeKind::Note || target.kind == NodeKind::Note {
            out.error("edge_dangling", None, Some(&edge.id), tr!("便签不能连线"));
            continue;
        }
        let outputs = effective_outputs(source, def);
        let inputs = effective_inputs(target);
        let (Some(from), Some(to)) = (find_port(&outputs, &edge.source_port), find_port(&inputs, &edge.target_port)) else {
            out.error("edge_dangling", None, Some(&edge.id), tr!("连线指向不存在的端口：{from} → {to}", from = format!("{}.{}", edge.source, edge.source_port), to = format!("{}.{}", edge.target, edge.target_port)));
            continue;
        };
        if !compatible(from.port_type, to.port_type) {
            out.error("edge_type_mismatch", None, Some(&edge.id), tr!("类型不兼容：{from} 不能连到 {to}", from = from.port_type.as_str(), to = to.port_type.as_str()));
        }
        if !edge.is_back() {
            *input_edge_count.entry((edge.target.as_str(), edge.target_port.as_str())).or_default() += 1;
        }
        if edge.is_back() {
            if !source.kind.is_branching() {
                out.error("back_edge_source", None, Some(&edge.id), tr!("只有分支和人工关卡节点可以发出回边"));
            }
            if !graph.ancestors(&edge.source).contains(&edge.target) {
                out.error("back_edge_target_not_ancestor", None, Some(&edge.id), tr!("回边必须指向上游节点"));
            }
            match edge.max_iterations {
                Some(n) if (1..=20).contains(&n) => {}
                _ => out.error("back_edge_max_missing", None, Some(&edge.id), tr!("回边的最多轮数必须在 1 到 20 之间")),
            }
            if to.required {
                out.error("back_edge_target_port_required", None, Some(&edge.id), tr!("回边指向的端口不能是必填（第一轮没有值）"));
            }
        }
    }
    for node in &def.nodes {
        for port in &node.inputs {
            let count = input_edge_count.get(&(node.id.as_str(), port.name.as_str())).copied().unwrap_or(0);
            if count > 1 && !port.multi {
                out.error("input_multi_violation", Some(&node.id), None, tr!("输入端口 {port} 不接受多条连线，请开启「多值」", port = port.name));
            }
            let back = graph.back_edges_into(&node.id).iter().any(|edge| edge.target_port == port.name);
            if port.required && count == 0 && port.default.is_none() && !back {
                out.error("required_input_unbound", Some(&node.id), None, tr!("必填输入 {port} 没有连线也没有默认值", port = port.name));
            }
        }
    }

    // 环
    let order = match graph.topo_order() {
        Ok(order) => order,
        Err(cyclic) => {
            out.error("cycle_without_back_edge", cyclic.first().copied(), None, tr!("存在环：{nodes}。循环请从分支或人工关卡节点连回边，并设置最多轮数", nodes = cyclic.join(", ")));
            Vec::new()
        }
    };

    // 可达性
    if let Some(trigger) = def.trigger() {
        let reachable = graph.descendants(&trigger.id);
        for node in &def.nodes {
            if node.kind != NodeKind::Note && node.kind != NodeKind::Trigger && !reachable.contains(&node.id) {
                out.warn("node_unreachable", Some(&node.id), None, tr!("从触发器出发到不了这个节点"));
            }
        }
    }

    // 模板
    let slot_keys: HashSet<&str> = def.workspaces.iter().map(|slot| slot.key.as_str()).collect();
    let run_inputs: HashSet<&str> = def.inputs.iter().map(|port| port.name.as_str()).collect();
    for node in &def.nodes {
        let ancestors = graph.ancestors(&node.id);
        let input_names: HashSet<&str> = node.inputs.iter().map(|port| port.name.as_str()).collect();
        for (field, source, is_expr, secret_ok) in template_fields(node) {
            if let Err(error) = template::check_syntax(&source, is_expr) {
                out.error("template_syntax", Some(&node.id), None, tr!("{field} 模板语法错误：{error}", field = field, error = error));
                continue;
            }
            if template::uses_secret(&source) && !secret_ok {
                out.error("secret_forbidden_here", Some(&node.id), None, tr!("{field} 里不能使用 secret()，只能用在脚本环境变量和 HTTP 请求里", field = field));
            }
            if secret_ok {
                for name in secret_names(&source) {
                    if !lookup.secret_exists(&name) {
                        out.warn("secret_unknown", Some(&node.id), None, tr!("密钥 {name} 还没有配置", name = name));
                    }
                }
            }
            for path in template::referenced_paths(&source, is_expr) {
                let parts: Vec<&str> = path.split('.').collect();
                let root = parts[0];
                if !ROOTS.contains(&root) {
                    out.error("template_ref_unknown", Some(&node.id), None, tr!("{field} 引用了未知变量 {name}", field = field, name = root));
                    continue;
                }
                match (root, parts.get(1)) {
                    ("inputs", Some(name)) if !input_names.contains(name) => {
                        out.error("template_ref_unknown", Some(&node.id), None, tr!("{field} 引用了不存在的输入端口 {name}", field = field, name = name));
                    }
                    ("nodes", Some(id)) => {
                        if def.node(id).is_none() {
                            out.error("template_ref_unknown", Some(&node.id), None, tr!("{field} 引用了不存在的节点 {name}", field = field, name = id));
                        } else if !ancestors.contains(*id) {
                            out.error("template_ref_not_upstream", Some(&node.id), None, tr!("{field} 只能引用上游节点，{name} 不在上游", field = field, name = id));
                        }
                    }
                    ("ws", Some(key)) if !slot_keys.contains(key) => {
                        out.error("template_ref_unknown", Some(&node.id), None, tr!("{field} 引用了不存在的工作区槽位 {name}", field = field, name = key));
                    }
                    ("run", Some(&"inputs")) => {
                        if let Some(name) = parts.get(2) {
                            if !run_inputs.contains(name) {
                                out.error("template_ref_unknown", Some(&node.id), None, tr!("{field} 引用了不存在的运行入参 {name}", field = field, name = name));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // 各类型
    for node in &def.nodes {
        for usage in &node.workspaces {
            if !slot_keys.contains(usage.slot.as_str()) {
                out.error("slot_unknown", Some(&node.id), None, tr!("引用了未声明的工作区槽位 {slot}", slot = usage.slot));
            }
        }
        match node.kind {
            NodeKind::Agent => match config::<AgentConfig>(node) {
                Ok(cfg) => {
                    let exists = match cfg.target.target_type.as_str() {
                        "squad" => lookup.squad_exists(&cfg.target.id),
                        _ => lookup.agent_exists(&cfg.target.id),
                    };
                    if !exists {
                        let code = if cfg.target.target_type == "squad" { "squad_missing" } else { "agent_missing" };
                        out.error(code, Some(&node.id), None, tr!("没有选择智能体或小队，或者它已被删除"));
                    }
                    if !matches!(cfg.mode.as_str(), "auto" | "plan") {
                        out.error("config_invalid", Some(&node.id), None, tr!("运行模式只能是 auto 或 plan"));
                    }
                    if cfg.prompt.trim().is_empty() {
                        out.error("config_invalid", Some(&node.id), None, tr!("智能体节点必须填写任务说明"));
                    }
                }
                Err(error) => out.error("config_invalid", Some(&node.id), None, error),
            },
            NodeKind::Script => match config::<ScriptConfig>(node) {
                Ok(cfg) => {
                    if cfg.command.trim().is_empty() {
                        out.error("config_invalid", Some(&node.id), None, tr!("脚本节点必须填写命令"));
                    }
                    if let Some(cwd) = &cfg.cwd {
                        if !slot_keys.contains(cwd.slot.as_str()) {
                            out.error("slot_unknown", Some(&node.id), None, tr!("引用了未声明的工作区槽位 {slot}", slot = cwd.slot));
                        }
                    }
                    if cfg.command.contains("raw_unsafe") {
                        out.warn("raw_unsafe", Some(&node.id), None, tr!("命令里使用了 raw_unsafe，插值不会被转义"));
                    }
                }
                Err(error) => out.error("config_invalid", Some(&node.id), None, error),
            },
            NodeKind::Switch => match config::<SwitchConfig>(node) {
                Ok(cfg) => {
                    if cfg.cases.is_empty() {
                        out.error("config_invalid", Some(&node.id), None, tr!("分支节点至少要有一个条件"));
                    }
                    if cfg.default_port.is_none() && !cfg.cases.iter().any(|case| case.when.trim() == "true") {
                        out.warn("switch_no_default", Some(&node.id), None, tr!("分支节点没有默认分支，条件都不满足时会失败"));
                    }
                }
                Err(error) => out.error("config_invalid", Some(&node.id), None, error),
            },
            NodeKind::Gate => match config::<GateConfig>(node) {
                Ok(cfg) => {
                    for decision in cfg.effective_decisions() {
                        let connected = def.edges.iter().any(|edge| edge.source == node.id && edge.source_port == decision.port);
                        if !connected {
                            out.warn("gate_decision_unconnected", Some(&node.id), None, tr!("「{label}」没有连线，选择后该分支直接结束", label = decision.label));
                        }
                    }
                }
                Err(error) => out.error("config_invalid", Some(&node.id), None, error),
            },
            NodeKind::Git => match config::<GitConfig>(node) {
                Ok(cfg) => {
                    if !slot_keys.contains(cfg.slot.as_str()) {
                        out.error("slot_unknown", Some(&node.id), None, tr!("引用了未声明的工作区槽位 {slot}", slot = cfg.slot));
                    }
                    if !matches!(cfg.op.as_str(), "commit" | "squash" | "push" | "merge" | "create_branch" | "diff") {
                        out.error("config_invalid", Some(&node.id), None, tr!("不支持的 Git 操作 {op}", op = cfg.op));
                    }
                    if matches!(cfg.op.as_str(), "push" | "merge") {
                        let ancestors = graph.ancestors(&node.id);
                        let gated = def.nodes.iter().any(|other| other.kind == NodeKind::Gate && ancestors.contains(&other.id));
                        if !gated {
                            out.warn("git_without_gate", Some(&node.id), None, tr!("推送或合并之前没有人工关卡"));
                        }
                    }
                }
                Err(error) => out.error("config_invalid", Some(&node.id), None, error),
            },
            NodeKind::Http => match config::<HttpConfig>(node) {
                Ok(cfg) => {
                    if cfg.url.trim().is_empty() {
                        out.error("config_invalid", Some(&node.id), None, tr!("HTTP 节点必须填写地址"));
                    }
                }
                Err(error) => out.error("config_invalid", Some(&node.id), None, error),
            },
            NodeKind::Subflow | NodeKind::Map => {
                let (workflow_id, version) = if node.kind == NodeKind::Subflow {
                    match config::<SubflowConfig>(node) {
                        Ok(cfg) => (cfg.workflow_id, cfg.version),
                        Err(error) => {
                            out.error("config_invalid", Some(&node.id), None, error);
                            continue;
                        }
                    }
                } else {
                    match config::<MapConfig>(node) {
                        Ok(cfg) => (cfg.workflow_id, cfg.version),
                        Err(error) => {
                            out.error("config_invalid", Some(&node.id), None, error);
                            continue;
                        }
                    }
                };
                if lookup.published(&workflow_id, version).is_none() {
                    out.error("subflow_unpublished", Some(&node.id), None, tr!("引用的子工作流不存在或还没有发布"));
                } else if subflow_depth(&workflow_id, version, lookup, &mut BTreeSet::new(), 1) {
                    out.error("subflow_recursion", Some(&node.id), None, tr!("子工作流引用成环或嵌套超过 5 层"));
                }
            }
            _ => {}
        }
    }

    // 并行写同一槽位
    let writers: Vec<(&str, &str)> = def
        .nodes
        .iter()
        .flat_map(|node| {
            node.workspaces
                .iter()
                .filter(|usage| usage.access == WorkspaceAccess::Write && !usage.fork)
                .map(move |usage| (node.id.as_str(), usage.slot.as_str()))
        })
        .collect();
    for (i, (a, slot_a)) in writers.iter().enumerate() {
        for (b, slot_b) in writers.iter().skip(i + 1) {
            if slot_a == slot_b && graph.may_run_parallel(a, b) {
                out.warn("slot_parallel_writers", Some(a), None, tr!("{a} 和 {b} 可能并行写同一个工作区，运行时会被串行化", a = a, b = b));
            }
        }
    }

    // 工作流输出
    for port in &def.outputs {
        let filled = def.nodes.iter().any(|node| node.kind == NodeKind::Output && node.inputs.iter().any(|input| input.name == port.name));
        if !filled {
            out.warn("output_unbound", None, None, tr!("工作流输出 {name} 没有任何输出节点填写", name = port.name));
        }
    }

    let _ = order;
    out.items
}

fn secret_names(source: &str) -> Vec<String> {
    let regex = regex::Regex::new(r#"secret\s*\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();
    regex
        .captures_iter(source)
        .map(|capture| capture[1].to_string())
        .collect()
}

/// true = 成环或过深
fn subflow_depth(id: &str, version: Option<u32>, lookup: &dyn Lookup, stack: &mut BTreeSet<String>, depth: u32) -> bool {
    if depth > 5 || !stack.insert(id.to_string()) {
        return true;
    }
    let Some(def) = lookup.published(id, version) else {
        stack.remove(id);
        return false;
    };
    for node in &def.nodes {
        let child = match node.kind {
            NodeKind::Subflow => config::<SubflowConfig>(node).ok().map(|cfg| (cfg.workflow_id, cfg.version)),
            NodeKind::Map => config::<MapConfig>(node).ok().map(|cfg| (cfg.workflow_id, cfg.version)),
            _ => None,
        };
        if let Some((child, child_version)) = child {
            if subflow_depth(&child, child_version, lookup, stack, depth + 1) {
                return true;
            }
        }
    }
    stack.remove(id);
    false
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    pub struct FakeLookup;
    impl Lookup for FakeLookup {
        fn agent_exists(&self, id: &str) -> bool {
            id == "coder"
        }
        fn squad_exists(&self, _id: &str) -> bool {
            false
        }
        fn published(&self, _id: &str, _v: Option<u32>) -> Option<WorkflowDef> {
            None
        }
        fn secret_exists(&self, name: &str) -> bool {
            name == "TOKEN"
        }
    }

    fn base() -> serde_json::Value {
        json!({
            "inputs": [{ "name": "url", "type": "url", "required": true }],
            "workspaces": [{ "key": "app", "project": "p1", "isolation": "worktree_new" }],
            "nodes": [
                { "id": "start", "kind": "trigger" },
                { "id": "code", "kind": "agent",
                  "inputs": [{ "name": "url", "type": "text", "required": true }, { "name": "feedback", "type": "markdown", "multi": true }],
                  "outputs": [{ "name": "summary", "type": "markdown", "required": true }],
                  "workspaces": [{ "slot": "app", "access": "write" }],
                  "config": { "target": { "type": "agent", "id": "coder" }, "prompt": "do {{ inputs.url }} {{ ws.app.branch }}" } },
                { "id": "test", "kind": "script", "config": { "command": "pnpm test", "cwd": { "slot": "app" }, "failOnNonZero": false } },
                { "id": "check", "kind": "switch", "inputs": [{ "name": "value", "type": "json", "required": true }],
                  "config": { "cases": [{ "port": "pass", "when": "inputs.value.exit_code == 0" }], "defaultPort": "fail" } }
            ],
            "edges": [
                { "id": "e1", "source": "start", "sourcePort": "url", "target": "code", "targetPort": "url" },
                { "id": "e2", "source": "code", "sourcePort": "$out", "target": "test", "targetPort": "$in" },
                { "id": "e3", "source": "test", "sourcePort": "$result", "target": "check", "targetPort": "value" },
                { "id": "e4", "source": "check", "sourcePort": "fail", "target": "code", "targetPort": "feedback", "maxIterations": 3 }
            ]
        })
    }

    fn codes(value: serde_json::Value) -> Vec<&'static str> {
        let def: WorkflowDef = serde_json::from_value(value).unwrap();
        validate(&def, &FakeLookup)
            .into_iter()
            .filter(|d| d.severity == "error")
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn valid_loop_flow_has_no_errors() {
        assert_eq!(codes(base()), Vec::<&str>::new());
    }

    #[test]
    fn catches_common_errors() {
        let mut value = base();
        value["edges"][3]["maxIterations"] = json!(null);
        assert!(codes(value).contains(&"cycle_without_back_edge"));

        let mut value = base();
        value["edges"][3]["maxIterations"] = json!(0);
        assert!(codes(value).contains(&"back_edge_max_missing"));

        let mut value = base();
        value["nodes"][1]["config"]["prompt"] = json!("{{ nodes.check.outputs.x }}");
        assert!(codes(value).contains(&"template_ref_not_upstream"));

        let mut value = base();
        value["nodes"][1]["config"]["prompt"] = json!("{{ secret('TOKEN') }}");
        assert!(codes(value).contains(&"secret_forbidden_here"));

        let mut value = base();
        value["nodes"][1]["config"]["target"]["id"] = json!("ghost");
        assert!(codes(value).contains(&"agent_missing"));

        let mut value = base();
        value["edges"][0]["targetPort"] = json!("nope");
        assert!(codes(value).contains(&"edge_dangling"));

        let mut value = base();
        value["nodes"][3]["inputs"][0]["type"] = json!("number");
        assert!(codes(value).contains(&"edge_type_mismatch"));

        let mut value = base();
        value["edges"].as_array_mut().unwrap().remove(0);
        assert!(codes(value).contains(&"required_input_unbound"));

        let mut value = base();
        value["nodes"][1]["config"]["prompt"] = json!("{{ inputs.url ");
        assert!(codes(value).contains(&"template_syntax"));

        let mut value = base();
        value["nodes"].as_array_mut().unwrap().push(json!({ "id": "start2", "kind": "trigger" }));
        assert!(codes(value).contains(&"trigger_count"));
    }

    #[test]
    fn back_edge_must_come_from_branching_node() {
        let mut value = base();
        value["edges"][3]["source"] = json!("test");
        value["edges"][3]["sourcePort"] = json!("stdout");
        assert!(codes(value).contains(&"back_edge_source"));
    }
}
