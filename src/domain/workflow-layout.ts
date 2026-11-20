import type { NodePosition, WorkflowDef } from "./workflow";
import { forwardEdges } from "./workflow-graph";

export interface NodeSize { width: number; height: number }

/** elk layered 自动布局：回边不参与布局（否则会把循环体拉成一团）。elkjs 按需加载。 */
export async function computeLayout(
  def: Pick<WorkflowDef, "nodes" | "edges">,
  sizes: Record<string, NodeSize>,
): Promise<Record<string, NodePosition>> {
  const { default: ELK } = await import("elkjs/lib/elk.bundled.js");
  const elk = new ELK();
  const ids = new Set(def.nodes.map((node) => node.id));
  const graph = {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "RIGHT",
      "elk.spacing.nodeNode": "40",
      "elk.layered.spacing.nodeNodeBetweenLayers": "80",
      "elk.layered.considerModelOrder.strategy": "NODES_AND_EDGES",
    },
    children: def.nodes.map((node) => ({
      id: node.id,
      width: sizes[node.id]?.width ?? 248,
      height: sizes[node.id]?.height ?? 120,
    })),
    edges: forwardEdges(def)
      .filter((edge) => ids.has(edge.source) && ids.has(edge.target))
      .map((edge) => ({ id: edge.id, sources: [edge.source], targets: [edge.target] })),
  };
  const result = await elk.layout(graph);
  const positions: Record<string, NodePosition> = {};
  for (const child of result.children ?? []) {
    positions[child.id] = { x: Math.round(child.x ?? 0), y: Math.round(child.y ?? 0) };
  }
  return positions;
}
