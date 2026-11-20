//! 图工具：去掉回边后的前向图、拓扑序、祖先/后代。

use std::collections::{BTreeSet, HashMap, VecDeque};

use super::model::{EdgeDef, NodeKind, WorkflowDef};

pub struct Graph<'a> {
    pub def: &'a WorkflowDef,
    /// 节点 id → 在 def.nodes 中的下标
    pub index: HashMap<&'a str, usize>,
    /// 前向图（不含回边、不含 note）
    pub forward_out: HashMap<&'a str, Vec<&'a EdgeDef>>,
    pub forward_in: HashMap<&'a str, Vec<&'a EdgeDef>>,
    pub back_edges: Vec<&'a EdgeDef>,
}

impl<'a> Graph<'a> {
    pub fn new(def: &'a WorkflowDef) -> Self {
        let mut index = HashMap::new();
        let mut forward_out: HashMap<&str, Vec<&EdgeDef>> = HashMap::new();
        let mut forward_in: HashMap<&str, Vec<&EdgeDef>> = HashMap::new();
        for (i, node) in def.nodes.iter().enumerate() {
            index.insert(node.id.as_str(), i);
            forward_out.entry(node.id.as_str()).or_default();
            forward_in.entry(node.id.as_str()).or_default();
        }
        let mut back_edges = Vec::new();
        for edge in &def.edges {
            if !index.contains_key(edge.source.as_str()) || !index.contains_key(edge.target.as_str())
            {
                continue;
            }
            if edge.is_back() {
                back_edges.push(edge);
                continue;
            }
            forward_out
                .entry(edge.source.as_str())
                .or_default()
                .push(edge);
            forward_in.entry(edge.target.as_str()).or_default().push(edge);
        }
        Graph {
            def,
            index,
            forward_out,
            forward_in,
            back_edges,
        }
    }

    pub fn executable_ids(&self) -> Vec<&'a str> {
        self.def
            .nodes
            .iter()
            .filter(|node| node.kind != NodeKind::Note)
            .map(|node| node.id.as_str())
            .collect()
    }

    /// Kahn 拓扑序；有环时返回 Err(环上的节点)
    pub fn topo_order(&self) -> Result<Vec<&'a str>, Vec<&'a str>> {
        let ids = self.executable_ids();
        let mut indegree: HashMap<&str, usize> = ids.iter().map(|id| (*id, 0)).collect();
        for id in &ids {
            for edge in self.forward_in.get(id).into_iter().flatten() {
                if indegree.contains_key(edge.source.as_str()) {
                    *indegree.get_mut(id).unwrap() += 1;
                }
            }
        }
        // 稳定：按定义顺序出队
        let mut queue: VecDeque<&str> = ids
            .iter()
            .copied()
            .filter(|id| indegree[id] == 0)
            .collect();
        let mut order = Vec::new();
        while let Some(id) = queue.pop_front() {
            order.push(id);
            let mut next: Vec<&str> = Vec::new();
            for edge in self.forward_out.get(id).into_iter().flatten() {
                let target = edge.target.as_str();
                if let Some(value) = indegree.get_mut(target) {
                    *value -= 1;
                    if *value == 0 {
                        next.push(target);
                    }
                }
            }
            next.sort_by_key(|id| self.index[id]);
            next.dedup();
            queue.extend(next);
        }
        if order.len() == ids.len() {
            Ok(order)
        } else {
            let cyclic = ids
                .into_iter()
                .filter(|id| !order.contains(id))
                .collect();
            Err(cyclic)
        }
    }

    pub fn descendants(&self, id: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![id.to_string()];
        while let Some(current) = stack.pop() {
            for edge in self.forward_out.get(current.as_str()).into_iter().flatten() {
                if seen.insert(edge.target.clone()) {
                    stack.push(edge.target.clone());
                }
            }
        }
        seen
    }

    pub fn ancestors(&self, id: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![id.to_string()];
        while let Some(current) = stack.pop() {
            for edge in self.forward_in.get(current.as_str()).into_iter().flatten() {
                if seen.insert(edge.source.clone()) {
                    stack.push(edge.source.clone());
                }
            }
        }
        seen
    }

    /// 两个节点之间没有任何前向路径 → 可能并行
    pub fn may_run_parallel(&self, a: &str, b: &str) -> bool {
        a != b && !self.descendants(a).contains(b) && !self.descendants(b).contains(a)
    }

    pub fn incoming(&self, id: &str) -> &[&'a EdgeDef] {
        self.forward_in.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn back_edges_from(&self, id: &str) -> Vec<&'a EdgeDef> {
        self.back_edges
            .iter()
            .copied()
            .filter(|edge| edge.source == id)
            .collect()
    }

    pub fn back_edges_into(&self, id: &str) -> Vec<&'a EdgeDef> {
        self.back_edges
            .iter()
            .copied()
            .filter(|edge| edge.target == id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn def() -> WorkflowDef {
        serde_json::from_value(json!({
            "nodes": [
                { "id": "start", "kind": "trigger" },
                { "id": "a", "kind": "transform" },
                { "id": "b", "kind": "transform" },
                { "id": "c", "kind": "switch" },
                { "id": "n", "kind": "note" }
            ],
            "edges": [
                { "id": "e1", "source": "start", "sourcePort": "$out", "target": "a", "targetPort": "$in" },
                { "id": "e2", "source": "start", "sourcePort": "$out", "target": "b", "targetPort": "$in" },
                { "id": "e3", "source": "a", "sourcePort": "$out", "target": "c", "targetPort": "$in" },
                { "id": "e4", "source": "b", "sourcePort": "$out", "target": "c", "targetPort": "$in" },
                { "id": "e5", "source": "c", "sourcePort": "x", "target": "a", "targetPort": "$in", "maxIterations": 2 }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn topo_ignores_back_edges_and_notes() {
        let def = def();
        let graph = Graph::new(&def);
        assert_eq!(graph.topo_order().unwrap(), vec!["start", "a", "b", "c"]);
        assert_eq!(graph.back_edges.len(), 1);
        assert!(graph.ancestors("c").contains("start"));
        assert!(graph.descendants("a").contains("c"));
        assert!(graph.may_run_parallel("a", "b"));
        assert!(!graph.may_run_parallel("a", "c"));
    }

    #[test]
    fn detects_cycles() {
        let mut def = def();
        def.edges[4].max_iterations = None;
        let graph = Graph::new(&def);
        let cyclic = graph.topo_order().unwrap_err();
        assert!(cyclic.contains(&"a") && cyclic.contains(&"c"));
    }
}
