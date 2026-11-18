import type { EdgeOverlay, NodeOverlay, RunOverlay } from "../canvas-types";
import { branchPorts, currentNodeRun, ERROR_PORT, isBranchingKind, type NodeRun, type RunDetail } from "../../../domain/workflow";
import { summarizeValue } from "./run-format";

const UNDECIDED = new Set(["pending", "ready", "running", "waiting_human", "waiting_children"]);

/** 连线存活判定（技术方案 3.3）：只看来源节点的当前节点运行 */
export function edgeState(
  source: NodeRun | undefined,
  sourceKind: string,
  sourcePort: string,
  onError: string | undefined,
  branch: string[],
): EdgeOverlay["state"] {
  if (!source || UNDECIDED.has(source.status)) return "pending";
  if (source.status === "skipped" || source.status === "cancelled") return "dead";
  if (source.status === "failed") {
    return onError === "continue" && sourcePort === ERROR_PORT ? "live" : "dead";
  }
  if (sourcePort === ERROR_PORT) return "dead";
  if (isBranchingKind(sourceKind as never) && branch.includes(sourcePort)) {
    return source.firedPorts.includes(sourcePort) ? "live" : "dead";
  }
  return "live";
}

export function buildRunOverlay(detail: RunDetail): RunOverlay {
  const nodes: Record<string, NodeOverlay> = {};
  for (const node of detail.definition.nodes) {
    const current = currentNodeRun(detail.nodeRuns, node.id);
    if (!current) continue;
    nodes[node.id] = {
      status: current.status,
      iteration: current.iteration,
      attempt: current.attempt,
      multiple: current.iteration > 0 || current.attempt > 1,
    };
  }
  const edges: Record<string, EdgeOverlay> = {};
  for (const edge of detail.definition.edges) {
    const source = detail.definition.nodes.find((node) => node.id === edge.source);
    if (!source) continue;
    const current = currentNodeRun(detail.nodeRuns, source.id);
    const state = edgeState(current, source.kind, edge.sourcePort, source.onError, branchPorts(source));
    const overlay: EdgeOverlay = { state };
    if (state === "live" && current) {
      const value = detail.values.find((item) => item.nodeRunId === current.id && item.port === edge.sourcePort);
      if (value && value.preview !== null && value.preview !== undefined) {
        overlay.label = summarizeValue(value.preview, 24);
        overlay.tooltip = summarizeValue(value.preview, 600);
      } else if (value) {
        overlay.label = `${value.size}B`;
      }
    }
    if (edge.maxIterations != null) overlay.count = detail.loopCounters[edge.id] ?? 0;
    edges[edge.id] = overlay;
  }
  return { nodes, edges };
}
