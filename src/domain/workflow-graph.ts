/** 前端用的轻量图工具：回边识别、祖先/后代集合、连线判定。权威校验在后端 validate.rs。 */
import type { EdgeDef, NodeDef, WorkflowDef } from "./workflow";
import { isBranchingKind } from "./workflow";

type GraphInput = Pick<WorkflowDef, "nodes" | "edges">;

export function isBackEdge(edge: EdgeDef): boolean {
  return edge.maxIterations != null;
}

export function forwardEdges(def: GraphInput): EdgeDef[] {
  return def.edges.filter((edge) => !isBackEdge(edge));
}

function adjacency(edges: EdgeDef[], reverse: boolean): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const edge of edges) {
    const from = reverse ? edge.target : edge.source;
    const to = reverse ? edge.source : edge.target;
    const list = map.get(from);
    if (list) list.push(to);
    else map.set(from, [to]);
  }
  return map;
}

function reach(start: string, map: Map<string, string[]>): Set<string> {
  const seen = new Set<string>();
  const stack = [...(map.get(start) ?? [])];
  while (stack.length) {
    const id = stack.pop() as string;
    if (seen.has(id)) continue;
    seen.add(id);
    stack.push(...(map.get(id) ?? []));
  }
  return seen;
}

/** 前向图（去掉回边）里 nodeId 的全部祖先，不含自身 */
export function ancestors(def: GraphInput, nodeId: string): Set<string> {
  const result = reach(nodeId, adjacency(forwardEdges(def), true));
  result.delete(nodeId);
  return result;
}

/** 前向图里 nodeId 的全部后代，不含自身 */
export function descendants(def: GraphInput, nodeId: string): Set<string> {
  const result = reach(nodeId, adjacency(forwardEdges(def), false));
  result.delete(nodeId);
  return result;
}

/** 回边循环体：目标节点 ∪ 其全部前向后代（引擎重跑的范围） */
export function loopBody(def: GraphInput, targetId: string): Set<string> {
  const body = descendants(def, targetId);
  body.add(targetId);
  return body;
}

export type ConnectKind = "forward" | "back" | "invalid";

export interface ConnectCheck {
  kind: ConnectKind;
  /** invalid 时的原因 key（i18n: workflow.connect.<reason>） */
  reason?: "self" | "note" | "back_source" | "duplicate" | "trigger_target" | "output_source";
}

/**
 * 判断 source → target 这条连线的性质：
 * 目标是源的祖先 ⇒ 必须是回边，且源只能是 switch / gate。
 */
export function checkConnection(def: GraphInput, source: NodeDef, target: NodeDef): ConnectCheck {
  if (source.id === target.id) return { kind: "invalid", reason: "self" };
  if (target.kind === "trigger") return { kind: "invalid", reason: "trigger_target" };
  if (source.kind === "output") return { kind: "invalid", reason: "output_source" };
  if (ancestors(def, source.id).has(target.id)) {
    return isBranchingKind(source.kind) ? { kind: "back" } : { kind: "invalid", reason: "back_source" };
  }
  return { kind: "forward" };
}

/** 删除节点时一并清理的连线 */
export function edgesOfNode(def: GraphInput, nodeId: string): EdgeDef[] {
  return def.edges.filter((edge) => edge.source === nodeId || edge.target === nodeId);
}

/** 拓扑序（只看前向边）；有环时把剩余节点追加在末尾 */
export function topoOrder(def: GraphInput): string[] {
  const indegree = new Map(def.nodes.map((node) => [node.id, 0]));
  const out = adjacency(forwardEdges(def), false);
  for (const edge of forwardEdges(def)) indegree.set(edge.target, (indegree.get(edge.target) ?? 0) + 1);
  const queue = def.nodes.filter((node) => (indegree.get(node.id) ?? 0) === 0).map((node) => node.id);
  const order: string[] = [];
  while (queue.length) {
    const id = queue.shift() as string;
    order.push(id);
    for (const next of out.get(id) ?? []) {
      const rest = (indegree.get(next) ?? 1) - 1;
      indegree.set(next, rest);
      if (rest === 0) queue.push(next);
    }
  }
  for (const node of def.nodes) if (!order.includes(node.id)) order.push(node.id);
  return order;
}

/** 复制粘贴：重新生成节点 / 连线 id，并修正选区内部的连线；选区外的连线丢弃 */
export function remapClipboard(
  nodes: NodeDef[],
  edges: EdgeDef[],
  takenNodeIds: Iterable<string>,
  takenEdgeIds: Iterable<string>,
  offset: { x: number; y: number },
): { nodes: NodeDef[]; edges: EdgeDef[]; idMap: Map<string, string> } {
  const usedNodes = new Set(takenNodeIds);
  const idMap = new Map<string, string>();
  const nextNodes = nodes.map((node) => {
    const base = node.id.replace(/_\d+$/, "") || node.kind;
    let id = base;
    let index = 1;
    while (usedNodes.has(id)) {
      index += 1;
      id = `${base}_${index}`;
    }
    usedNodes.add(id);
    idMap.set(node.id, id);
    const copy = JSON.parse(JSON.stringify(node)) as NodeDef;
    copy.id = id;
    // 内置节点模板 JSON 常不带 position；缺省按 (0,0) 再叠投放偏移。
    const x = node.position?.x ?? 0;
    const y = node.position?.y ?? 0;
    copy.position = { x: x + offset.x, y: y + offset.y };
    return copy;
  });
  const usedEdges = new Set(takenEdgeIds);
  const nextEdges: EdgeDef[] = [];
  let counter = usedEdges.size;
  for (const edge of edges) {
    const source = idMap.get(edge.source);
    const target = idMap.get(edge.target);
    if (!source || !target) continue;
    let id = `e${++counter}`;
    while (usedEdges.has(id)) id = `e${++counter}`;
    usedEdges.add(id);
    nextEdges.push({ ...JSON.parse(JSON.stringify(edge)), id, source, target });
  }
  return { nodes: nextNodes, edges: nextEdges, idMap };
}
