<script setup lang="ts">
import { computed, nextTick, provide, ref, shallowRef, watch } from "vue";
import { VueFlow, useVueFlow, ConnectionMode, MarkerType, type Connection, type Edge, type Node } from "@vue-flow/core";
import { Background } from "@vue-flow/background";
import { Controls } from "@vue-flow/controls";
import { MiniMap } from "@vue-flow/minimap";
import "@vue-flow/core/dist/style.css";
import "@vue-flow/controls/dist/style.css";
import "@vue-flow/minimap/dist/style.css";
import BaseNode from "./nodes/BaseNode.vue";
import DataEdge from "./edges/DataEdge.vue";
import BackEdge from "./edges/BackEdge.vue";
import { NODE_KIND_META } from "./node-meta";
import { CONNECTING_KEY, type ConnectingContext } from "./connecting";
import type { RunOverlay } from "./canvas-types";
import { checkConnection, isBackEdge } from "../../domain/workflow-graph";
import { computeLayout, type NodeSize } from "../../domain/workflow-layout";
import {
  effectiveInputs,
  effectiveOutputs,
  isTypeCompatible,
  type Diagnostic,
  type NodePosition,
  type PortType,
  type WorkflowDef,
} from "../../domain/workflow";

const props = withDefaults(defineProps<{
  definition: WorkflowDef;
  readonly?: boolean;
  overlay?: RunOverlay;
  diagnostics?: Diagnostic[];
  selectedNodeId?: string | null;
  selectedEdgeId?: string | null;
  minimap?: boolean;
}>(), { readonly: false, overlay: undefined, diagnostics: () => [], selectedNodeId: null, selectedEdgeId: null, minimap: true });

export interface CanvasConnect {
  source: string;
  sourcePort: string;
  target: string;
  targetPort: string;
}

const emit = defineEmits<{
  "select-node": [id: string | null];
  "select-edge": [id: string | null];
  "selection-change": [selection: { nodes: string[]; edges: string[] }];
  connect: [connection: CanvasConnect];
  "move-nodes": [positions: Record<string, NodePosition>];
  "drop-payload": [payload: string, position: NodePosition];
  "pane-click": [];
}>();

const flowId = `wf-canvas-${Math.random().toString(36).slice(2, 8)}`;
const {
  onConnectStart, onConnectEnd, onNodeDragStop, screenToFlowCoordinate, fitView, findNode,
  getNodes, getSelectedNodes, getSelectedEdges, setCenter,
} = useVueFlow(flowId);

const selNodes = ref<Set<string>>(new Set());
const selEdges = ref<Set<string>>(new Set());

watch(() => props.selectedNodeId, (id) => { if (id !== undefined) selNodes.value = new Set(id ? [id] : []); }, { immediate: true });
watch(() => props.selectedEdgeId, (id) => { if (id !== undefined) selEdges.value = new Set(id ? [id] : []); }, { immediate: true });

const severity = computed(() => {
  const nodes = new Map<string, "error" | "warning">();
  const edges = new Map<string, "error" | "warning">();
  for (const item of props.diagnostics) {
    const level = item.severity;
    const put = (map: Map<string, "error" | "warning">, id: string) => {
      if (map.get(id) !== "error") map.set(id, level);
    };
    if (item.nodeId) put(nodes, item.nodeId);
    if (item.edgeId) put(edges, item.edgeId);
  }
  return { nodes, edges };
});

const nodeById = computed(() => new Map(props.definition.nodes.map((node) => [node.id, node])));

const flowNodes = computed<Node[]>(() =>
  props.definition.nodes.map((node) => ({
    id: node.id,
    type: "wf",
    position: { x: node.position?.x ?? 0, y: node.position?.y ?? 0 },
    selected: selNodes.value.has(node.id),
    draggable: !props.readonly,
    deletable: false,
    data: {
      node,
      inputs: effectiveInputs(node),
      outputs: effectiveOutputs(node, props.definition),
      overlay: props.overlay?.nodes[node.id],
      severity: severity.value.nodes.get(node.id) ?? null,
      readonly: props.readonly,
    },
  })),
);

const flowEdges = computed<Edge[]>(() =>
  props.definition.edges.flatMap((edge) => {
    const source = nodeById.value.get(edge.source);
    if (!source || !nodeById.value.has(edge.target)) return [];
    const port = effectiveOutputs(source, props.definition).find((item) => item.name === edge.sourcePort);
    const back = isBackEdge(edge);
    const control = edge.sourcePort.startsWith("$") && edge.sourcePort !== "$result";
    const overlay = props.overlay?.edges[edge.id];
    return [{
      id: edge.id,
      source: edge.source,
      sourceHandle: edge.sourcePort,
      target: edge.target,
      targetHandle: edge.targetPort,
      type: back ? "wfback" : "wfdata",
      selected: selEdges.value.has(edge.id),
      markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14, color: back ? "var(--color-warning)" : "var(--color-text-tertiary)" },
      data: {
        type: (port?.type ?? "any") as PortType,
        control,
        maxIterations: edge.maxIterations,
        overlay,
        severity: severity.value.edges.get(edge.id) ?? null,
      },
    }];
  }),
);

// ---------- 连线时的兼容提示 ----------
const connectingState = ref<ConnectingContext["state"]["value"]>(null);

function canTarget(nodeId: string, port: string): boolean {
  const from = connectingState.value;
  if (!from) return true;
  const source = nodeById.value.get(from.nodeId);
  const target = nodeById.value.get(nodeId);
  if (!source || !target) return false;
  if (checkConnection(props.definition, source, target).kind === "invalid") return false;
  const targetPort = effectiveInputs(target).find((item) => item.name === port);
  if (!targetPort) return false;
  return isTypeCompatible(from.type, targetPort.type);
}

provide(CONNECTING_KEY, { state: connectingState, canTarget } satisfies ConnectingContext);

onConnectStart(({ nodeId, handleId, handleType }) => {
  if (props.readonly || handleType !== "source" || !nodeId || !handleId) return;
  const node = nodeById.value.get(nodeId);
  const port = node ? effectiveOutputs(node, props.definition).find((item) => item.name === handleId) : undefined;
  connectingState.value = { nodeId, port: handleId, type: port?.type ?? "any" };
});
onConnectEnd(() => {
  connectingState.value = null;
});

function isValidConnection(connection: Connection): boolean {
  if (!connection.source || !connection.target || !connection.sourceHandle || !connection.targetHandle) return false;
  const from = nodeById.value.get(connection.source);
  const to = nodeById.value.get(connection.target);
  if (!from || !to) return false;
  if (checkConnection(props.definition, from, to).kind === "invalid") return false;
  const out = effectiveOutputs(from, props.definition).find((item) => item.name === connection.sourceHandle);
  const input = effectiveInputs(to).find((item) => item.name === connection.targetHandle);
  return Boolean(out && input && isTypeCompatible(out.type, input.type));
}

function onConnect(connection: Connection) {
  if (props.readonly || !isValidConnection(connection)) return;
  emit("connect", {
    source: connection.source,
    sourcePort: connection.sourceHandle as string,
    target: connection.target,
    targetPort: connection.targetHandle as string,
  });
}

// ---------- 选择 / 拖动 ----------
function sameSet(left: Set<string>, right: string[]) {
  return left.size === right.length && right.every((id) => left.has(id));
}

watch([getSelectedNodes, getSelectedEdges], ([nodes, edges]) => {
  const nodeIds = nodes.map((item) => item.id);
  const edgeIds = edges.map((item) => item.id);
  if (sameSet(selNodes.value, nodeIds) && sameSet(selEdges.value, edgeIds)) return;
  selNodes.value = new Set(nodeIds);
  selEdges.value = new Set(edgeIds);
  emit("selection-change", { nodes: nodeIds, edges: edgeIds });
  emit("select-node", nodeIds.length === 1 && !edgeIds.length ? nodeIds[0] : null);
  emit("select-edge", edgeIds.length === 1 && !nodeIds.length ? edgeIds[0] : null);
});

onNodeDragStop(({ nodes }) => {
  if (props.readonly) return;
  const dragged = nodes.length ? nodes : getSelectedNodes.value;
  emit("move-nodes", Object.fromEntries(dragged.map((item) => [item.id, { x: item.position.x, y: item.position.y }])));
});

function onDrop(event: DragEvent) {
  if (props.readonly) return;
  const payload = event.dataTransfer?.getData("application/x-multara-workflow");
  if (!payload) return;
  event.preventDefault();
  emit("drop-payload", payload, screenToFlowCoordinate({ x: event.clientX, y: event.clientY }));
}

function onDragOver(event: DragEvent) {
  if (props.readonly) return;
  if (event.dataTransfer?.types.includes("application/x-multara-workflow")) {
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
  }
}

// ---------- 对外方法 ----------
const sizes = shallowRef<Record<string, NodeSize>>({});

function measuredSizes(): Record<string, NodeSize> {
  const result: Record<string, NodeSize> = {};
  for (const node of getNodes.value) {
    result[node.id] = { width: node.dimensions?.width || 248, height: node.dimensions?.height || 120 };
  }
  sizes.value = result;
  return result;
}

async function autoLayout() {
  const positions = await computeLayout(props.definition, measuredSizes());
  emit("move-nodes", positions);
  await nextTick();
  window.setTimeout(() => void fitView({ padding: 0.2, duration: 200 }), 60);
}

function select(nodeIds: string[], edgeIds: string[] = [], center = false) {
  selNodes.value = new Set(nodeIds);
  selEdges.value = new Set(edgeIds);
  emit("selection-change", { nodes: nodeIds, edges: edgeIds });
  emit("select-node", nodeIds.length === 1 && !edgeIds.length ? nodeIds[0] : null);
  emit("select-edge", edgeIds.length === 1 && !nodeIds.length ? edgeIds[0] : null);
  if (center && nodeIds[0]) {
    const node = findNode(nodeIds[0]);
    if (node) void setCenter(node.position.x + 124, node.position.y + 60, { zoom: 1, duration: 250 });
  }
}

function selectAll() {
  select(props.definition.nodes.map((node) => node.id));
}

function viewportCenter(): NodePosition {
  const rect = document.getElementById(flowId)?.getBoundingClientRect();
  return screenToFlowCoordinate({ x: (rect?.left ?? 0) + (rect?.width ?? 600) / 2, y: (rect?.top ?? 0) + (rect?.height ?? 400) / 2 });
}

defineExpose({ autoLayout, select, selectAll, fitView: () => fitView({ padding: 0.2, duration: 200 }), viewportCenter });

const minimapColor = (node: Node) => NODE_KIND_META[(node.data?.node?.kind ?? "note") as keyof typeof NODE_KIND_META]?.color ?? "var(--color-text-tertiary)";
</script>

<template>
  <div :id="flowId" class="wf-canvas" :class="{ readonly }" @drop="onDrop" @dragover="onDragOver">
    <VueFlow
      :id="flowId"
      :nodes="flowNodes"
      :edges="flowEdges"
      :connection-mode="ConnectionMode.Strict"
      :is-valid-connection="isValidConnection"
      :nodes-draggable="!readonly"
      :nodes-connectable="!readonly"
      :edges-updatable="false"
      :delete-key-code="null"
      :selection-key-code="readonly ? null : 'Shift'"
      :multi-selection-key-code="['Meta', 'Control']"
      :min-zoom="0.15"
      :max-zoom="1.6"
      :fit-view-on-init="true"
      :default-viewport="{ x: 40, y: 40, zoom: 0.85 }"
      :snap-to-grid="true"
      :snap-grid="[8, 8]"
      @connect="onConnect"
      @pane-click="emit('pane-click')"
    >
      <template #node-wf="nodeProps"><BaseNode v-bind="nodeProps" /></template>
      <template #edge-wfdata="edgeProps"><DataEdge v-bind="edgeProps" /></template>
      <template #edge-wfback="edgeProps"><BackEdge v-bind="edgeProps" /></template>
      <Background :gap="20" pattern-color="var(--color-border)" />
      <Controls :show-interactive="false" position="bottom-left" />
      <MiniMap v-if="minimap" pannable zoomable :node-color="minimapColor" mask-color="rgb(0 0 0 / 40%)" />
    </VueFlow>
  </div>
</template>

<style>
.wf-canvas { position: relative; width: 100%; height: 100%; min-height: 0; background: var(--color-bg); }
.wf-canvas .vue-flow { --vf-handle: var(--color-text-tertiary); }
.wf-canvas .vue-flow__node { cursor: default; }
.wf-canvas .vue-flow__edge { cursor: pointer; }
.wf-canvas .vue-flow__selection { background: var(--color-primary-subtle); border: 1px solid var(--color-primary-border); }
.wf-canvas .vue-flow__connection-path { stroke: var(--color-focus); stroke-width: 2; stroke-dasharray: 5 4; }
.wf-canvas .vue-flow__controls { box-shadow: var(--shadow-md); border-radius: var(--radius-md); overflow: hidden; }
.wf-canvas .vue-flow__controls-button { background: var(--color-surface-raised); border-bottom: 1px solid var(--color-border); color: var(--color-text-secondary); fill: var(--color-text-secondary); }
.wf-canvas .vue-flow__controls-button:hover { background: var(--color-surface-hover); }
.wf-canvas .vue-flow__minimap { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: var(--radius-md); }
.wf-canvas .vue-flow__edge.selected .vue-flow__edge-path { stroke-width: 2.5; }
.wf-canvas .vue-flow__background { background: var(--color-bg); }
</style>
