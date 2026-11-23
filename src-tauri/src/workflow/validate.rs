//! 保存和发布前的静态校验。

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::graph::Graph;
use super::model::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: &'static str,
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub message: String,
}

pub trait Lookup {
    fn agent_exists(&self, id: &str) -> bool;
    fn squad_exists(&self, id: &str) -> bool;
    fn published(&self, workflow_id: &str, version: Option<u32>) -> Option<WorkflowDef>;
    fn secret_exists(&self, name: &str) -> bool;
}

struct Out {
    items: Vec<Diagnostic>,
}

impl Out {
    fn push(&mut self, severity: &'static str, code: &'static str, node: Option<&str>, edge: Option<&str>, field: Option<&str>, message: String) {
        self.items.push(Diagnostic {
            severity,
            code,
            node_id: node.map(str::to_string),
            edge_id: edge.map(str::to_string),
            field: field.map(str::to_string),
            message,
        });
    }
    fn error(&mut self, code: &'static str, node: Option<&str>, edge: Option<&str>, message: String) {
        self.push("error", code, node, edge, None, message);
    }
    fn error_at(&mut self, code: &'static str, node: Option<&str>, field: &str, message: String) {
        self.push("error", code, node, None, Some(field), message);
    }
}

pub fn has_errors(items: &[Diagnostic]) -> bool {
    items.iter().any(|item| item.severity == "error")
}

pub fn validate(def: &WorkflowDef, lookup: &dyn Lookup) -> Vec<Diagnostic> {
    let mut out = Out { items: Vec::new() };
    let mut ids: HashSet<&str> = HashSet::new();
    let triggers = def.nodes.iter().filter(|node| node.kind == NodeKind::Trigger).count();
    if triggers != 1 {
        out.error("trigger_count", None, None, tr!("一条工作流要有且只有一个开始"));
    }
    if !def.nodes.iter().any(|node| node.kind == NodeKind::Output) {
        out.error("missing_output", None, None, tr!("一条工作流要有一个结束"));
    }
    for input in &def.inputs {
        if !is_valid_name(&input.name) {
            out.error("input_name", None, None, tr!("开始节点的收集项要有名字"));
        }
    }
    let mut seen_inputs = HashSet::new();
    for input in &def.inputs {
        if !seen_inputs.insert(input.name.as_str()) {
            out.error("input_name", None, None, tr!("收集项 {name} 重复了", name = input.name));
        }
    }
    for node in &def.nodes {
        if !is_valid_id(&node.id) {
            out.error("node_id", Some(&node.id), None, tr!("节点 id 要以小写字母开头，只能包含小写字母、数字和下划线"));
        }
        if !ids.insert(node.id.as_str()) {
            out.error("node_id", Some(&node.id), None, tr!("节点 id {id} 重复了", id = node.id));
        }
        match node.kind {
            NodeKind::Agent => match config::<AgentConfig>(node) {
                Ok(cfg) => {
                    if cfg.prompt.trim().is_empty() {
                        out.error_at("agent_prompt", Some(&node.id), "config:prompt", tr!("写上这一步要做的事"));
                    }
                    let id = cfg.target.id.trim();
                    if id.is_empty() {
                        out.error_at("agent_target", Some(&node.id), "config:target", tr!("选择一个智能体或小队"));
                    } else if cfg.target.target_type == "squad" {
                        if !lookup.squad_exists(id) {
                            out.error_at("agent_target", Some(&node.id), "config:target", tr!("小队不存在"));
                        }
                    } else if !lookup.agent_exists(id) {
                        out.error_at("agent_target", Some(&node.id), "config:target", tr!("智能体不存在"));
                    }
                    if cfg.context != "previous" && cfg.context != "all" {
                        out.error_at("agent_context", Some(&node.id), "config:context", tr!("上下文只能是上一步，或从开始到现在"));
                    }
                    if cfg.route != "auto" && cfg.route != "human" && cfg.route != "all" {
                        out.error_at("agent_route", Some(&node.id), "config:route", tr!("多条出路可以都送下去，或由智能体选，或等人选"));
                    }
                }
                Err(error) => out.error("config", Some(&node.id), None, error),
            },
            NodeKind::Trigger | NodeKind::Output => {}
        }
        for usage in &node.workspaces {
            if def.slot(&usage.slot).is_none() {
                out.error("workspace", Some(&node.id), None, tr!("引用了未声明的工作区 {slot}", slot = usage.slot));
            }
        }
    }
    let mut edge_ids = HashSet::new();
    for edge in &def.edges {
        if !edge_ids.insert(edge.id.as_str()) {
            out.error("edge_id", None, Some(&edge.id), tr!("连线 id 重复了"));
        }
        if def.node(&edge.source).is_none() || def.node(&edge.target).is_none() {
            out.error("edge_dangling", None, Some(&edge.id), tr!("连线指向不存在的节点"));
            continue;
        }
        if edge.source == edge.target {
            out.error("edge_self", None, Some(&edge.id), tr!("连线不能指向自己"));
        }
        if let Some(limit) = edge.max_iterations {
            if !(1..=20).contains(&limit) {
                out.error("loop_limit", None, Some(&edge.id), tr!("回边的轮数上限要在 1 到 20 之间"));
            }
        }
    }
    let graph = Graph::new(def);
    if let Err(cyclic) = graph.topo_order() {
        out.error("cycle", None, None, tr!("去掉回边之后仍然有环：{nodes}", nodes = cyclic.join(", ")));
    }
    let ancestors = ancestor_map(&graph);
    for edge in &def.edges {
        if !edge.is_back() {
            continue;
        }
        let ok = ancestors.get(edge.source.as_str()).is_some_and(|set| set.contains(&edge.target));
        if !ok {
            out.error("back_edge", None, Some(&edge.id), tr!("回边只能指回上游节点"));
        }
    }
    if let Some(start) = def.trigger() {
        let mut reachable = HashSet::new();
        let mut stack = vec![start.id.as_str()];
        while let Some(id) = stack.pop() {
            if !reachable.insert(id) {
                continue;
            }
            for edge in def.outgoing(id) {
                stack.push(edge.target.as_str());
            }
        }
        for node in &def.nodes {
            if !reachable.contains(node.id.as_str()) {
                out.error("unreachable", Some(&node.id), None, tr!("从开始走不到这个节点"));
            }
        }
    }
    out.items
}

fn ancestor_map<'a>(graph: &Graph<'a>) -> HashMap<&'a str, HashSet<String>> {
    let mut map: HashMap<&str, HashSet<String>> = HashMap::new();
    if let Ok(order) = graph.topo_order() {
        for id in order {
            let mut set = HashSet::new();
            for edge in graph.forward_in.get(id).into_iter().flatten() {
                set.insert(edge.source.clone());
                if let Some(prior) = map.get(edge.source.as_str()) {
                    set.extend(prior.iter().cloned());
                }
            }
            map.insert(id, set);
        }
    }
    map
}
