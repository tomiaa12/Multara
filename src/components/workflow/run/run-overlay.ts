import type { EdgeOverlay, NodeOverlay, RunOverlay } from "../canvas-types";
import { currentNodeRun, type NodeRun, type RunDetail } from "../../../domain/workflow";
import { summarizeValue } from "./run-format";

const UNDECIDED = new Set(["pending", "ready", "running", "waiting_human", "waiting_children"]);
const NOT_RUN = new Set(["pending", "skipped", "cancelled"]);

function latestRun(runs: NodeRun[]): NodeRun | undefined {
  return runs.reduce<NodeRun | undefined>((best, run) => {
    if (!best || run.iteration > best.iteration || (run.iteration === best.iteration && run.attempt > best.attempt)) return run;
    return best;
  }, undefined);
}

function chose(nextNodeId: string | null | undefined, targetId: string): boolean {
  const text = nextNodeId?.trim() ?? "";
  if (!text) return false;
  if (text.startsWith("[")) {
    try {
      const ids = JSON.parse(text) as unknown;
      return Array.isArray(ids) && ids.includes(targetId);
    } catch {
      return false;
    }
  }
  return text === targetId;
}

/** 一条线是否被走过：看来源节点记下的下一步。多条时可以同时记下。 */
export function edgeState(sources: NodeRun[], targetId: string, target: NodeRun | undefined): EdgeOverlay["state"] {
  const latest = latestRun(sources);
  if (!latest || UNDECIDED.has(latest.status)) {
    return sources.some((item) => chose(item.nextNodeId, targetId) && !UNDECIDED.has(item.status)) ? "live" : "pending";
  }
  const walked = sources.filter((item) => item.status === "succeeded" || item.status === "reused");
  if (!walked.length) return "dead";
  if (walked.some((item) => item.nextNodeId)) return walked.some((item) => chose(item.nextNodeId, targetId)) ? "live" : "dead";
  if (!target || NOT_RUN.has(target.status)) return "dead";
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
      sessionId: current.sessionId,
    };
  }
  const edges: Record<string, EdgeOverlay> = {};
  for (const edge of detail.definition.edges) {
    const sources = detail.nodeRuns.filter((item) => item.nodeId === edge.source);
    const targetRun = currentNodeRun(detail.nodeRuns, edge.target);
    const state = edgeState(sources, edge.target, targetRun);
    const overlay: EdgeOverlay = { state, flowing: state === "live" && Boolean(targetRun && UNDECIDED.has(targetRun.status)) };
    const current = currentNodeRun(detail.nodeRuns, edge.source);
    if (state === "live" && current) {
      const value = detail.values.find((item) => item.nodeRunId === current.id);
      if (value && value.preview !== null && value.preview !== undefined) {
        overlay.label = summarizeValue(value.preview, 24);
        overlay.tooltip = summarizeValue(value.preview, 600);
      } else if (value) {
        overlay.label = `${value.size}B`;
      }
    }
    if (edge.maxIterations != null) overlay.count = detail.loopCounters[edge.id] ?? 0;
    edges[edge.id] = overlay;
    if (overlay.label) {
      const target = nodes[edge.target];
      if (target) target.incoming = target.incoming ? `${target.incoming} · ${overlay.label}` : overlay.label;
    }
  }
  return { nodes, edges };
}
