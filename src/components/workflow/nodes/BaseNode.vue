<script setup lang="ts">
import { computed, inject } from "vue";
import { Handle, Position } from "@vue-flow/core";
import { AlertTriangle, CircleAlert } from "@lucide/vue";
import NodeSummary from "../NodeSummary.vue";
import { NODE_KIND_META, kindLabelKey } from "../node-meta";
import { nodeStatusLabel } from "../run/run-format";
import { t } from "../../../i18n";
import {
  CONTROL_PORT_COLOR,
  ERROR_PORT,
  IN_PORT,
  OUT_PORT,
  PORT_COLORS,
  isImplicitPort,
  type NodeDef,
  type PortDef,
} from "../../../domain/workflow";
import type { NodeOverlay } from "../canvas-types";
import { CONNECTING_KEY, type ConnectingContext } from "../connecting";

const props = defineProps<{
  id: string;
  selected?: boolean;
  data: {
    node: NodeDef;
    inputs: PortDef[];
    outputs: PortDef[];
    overlay?: NodeOverlay;
    severity?: "error" | "warning" | null;
    readonly: boolean;
  };
}>();

const connecting = inject<ConnectingContext | null>(CONNECTING_KEY, null);
const node = computed(() => props.data.node);
const meta = computed(() => NODE_KIND_META[node.value.kind]);
const isNote = computed(() => node.value.kind === "note");
const dataInputs = computed(() => props.data.inputs.filter((port) => !isImplicitPort(port.name)));
// 控制类端口里 $result 是数据端口（整包连给 switch），照常显示成一行
const dataOutputs = computed(() =>
  props.data.outputs.filter((port) => !isImplicitPort(port.name) || port.name === "$result"),
);
const title = computed(() => node.value.title || node.value.id);
const status = computed(() => props.data.overlay?.status ?? "");
const iterLabel = computed(() => {
  const overlay = props.data.overlay;
  if (!overlay?.multiple) return "";
  return t("workflow.canvas.iterAttempt", { iteration: overlay.iteration + 1, attempt: overlay.attempt });
});

function color(port: PortDef) {
  return PORT_COLORS[port.type] ?? CONTROL_PORT_COLOR;
}

function targetDisabled(port: string) {
  return connecting ? !connecting.canTarget(props.id, port) : false;
}
</script>

<template>
  <div
    class="wf-node"
    :class="[
      `kind-${node.kind}`,
      status ? `st-${status}` : '',
      { selected, readonly: data.readonly, note: isNote, 'has-error': data.severity === 'error', 'has-warning': data.severity === 'warning' },
    ]"
    :style="{ '--kind-color': meta.color }"
  >
    <template v-if="!isNote">
      <Handle :id="IN_PORT" type="target" :position="Position.Left" class="wf-handle ctrl" :class="{ disabled: targetDisabled(IN_PORT) }" :connectable="!data.readonly" />
      <Handle :id="OUT_PORT" type="source" :position="Position.Right" class="wf-handle ctrl" :connectable="!data.readonly" />
      <Handle :id="ERROR_PORT" type="source" :position="Position.Bottom" class="wf-handle err" :connectable="!data.readonly" />
    </template>

    <header class="wf-head">
      <component :is="meta.icon" :size="14" class="wf-icon" />
      <span class="wf-title" :title="title">{{ title }}</span>
      <CircleAlert v-if="data.severity === 'error'" :size="13" class="wf-diag error" />
      <AlertTriangle v-else-if="data.severity === 'warning'" :size="13" class="wf-diag warning" />
      <span v-if="status" class="wf-status" :class="`st-${status}`">{{ nodeStatusLabel(status) }}</span>
      <span v-else-if="!isNote" class="wf-kind">{{ t(kindLabelKey(node.kind)) }}</span>
    </header>
    <div v-if="iterLabel" class="wf-iter">{{ iterLabel }}</div>

    <NodeSummary :node="node" />

    <div v-if="dataInputs.length || dataOutputs.length" class="wf-ports">
      <div class="wf-col in">
        <div v-for="port in dataInputs" :key="port.name" class="wf-port in" :title="port.description || port.type">
          <Handle
            :id="port.name"
            type="target"
            :position="Position.Left"
            class="wf-handle"
            :class="{ disabled: targetDisabled(port.name) }"
            :style="{ '--port-color': color(port) }"
            :connectable="!data.readonly"
          />
          <span class="wf-port-name">{{ port.label || port.name }}<i v-if="port.required" class="req">*</i></span>
        </div>
      </div>
      <div class="wf-col out">
        <div v-for="port in dataOutputs" :key="port.name" class="wf-port out" :title="port.description || port.type">
          <span class="wf-port-name">{{ port.name }}</span>
          <Handle
            :id="port.name"
            type="source"
            :position="Position.Right"
            class="wf-handle"
            :style="{ '--port-color': color(port) }"
            :connectable="!data.readonly"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wf-node {
  position: relative;
  width: 248px;
  background: var(--color-surface-raised);
  border: 1px solid var(--color-border-strong);
  border-left: 3px solid var(--kind-color);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
  font-size: var(--font-size-sm);
  color: var(--color-text);
  transition: border-color var(--motion-duration-fast) var(--motion-ease), box-shadow var(--motion-duration-fast) var(--motion-ease);
}
.wf-node:hover { border-color: var(--color-text-tertiary); border-left-color: var(--kind-color); }
.wf-node.selected { border-color: var(--color-focus); border-left-color: var(--kind-color); box-shadow: 0 0 0 2px var(--color-focus-ring); }
.wf-node.has-error { border-color: var(--color-error-border); border-left-color: var(--kind-color); }
.wf-node.has-warning:not(.has-error) { border-color: var(--color-warning-border); border-left-color: var(--kind-color); }
.wf-node.note { background: var(--color-warning-subtle); border-color: var(--color-warning-border); border-left-color: var(--color-warning); width: 200px; }

.wf-head { display: flex; align-items: center; gap: var(--spacing-1-5); padding: var(--spacing-2) var(--spacing-2-5); min-width: 0; }
.wf-icon { flex: none; color: var(--kind-color); }
.wf-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: var(--font-weight-medium); }
.wf-kind { flex: none; font-size: var(--font-size-xs); color: var(--color-text-tertiary); }
.wf-diag { flex: none; }
.wf-diag.error { color: var(--color-error); }
.wf-diag.warning { color: var(--color-warning); }
.wf-status { flex: none; font-size: var(--font-size-xs); padding: 0 var(--spacing-1-5); border-radius: var(--radius-full); border: 1px solid var(--color-border-strong); color: var(--color-text-secondary); }
.wf-iter { margin: 0 var(--spacing-2-5); font-size: var(--font-size-xs); color: var(--color-text-tertiary); }

/* 运行状态（doc 11.5） */
.wf-node.st-pending { opacity: 0.7; }
.wf-node.st-ready { border-color: var(--color-info); }
.wf-node.st-running { border-color: var(--color-info); animation: wf-pulse 1.6s var(--motion-ease) infinite; }
.wf-node.st-waiting_human, .wf-node.st-waiting_children { border-color: var(--color-warning); }
.wf-node.st-succeeded { border-color: var(--color-success-border); }
.wf-node.st-failed { border-color: var(--color-error); }
.wf-node.st-skipped { border-style: dashed; opacity: 0.55; }
.wf-node.st-cancelled {
  opacity: 0.6;
  background-image: repeating-linear-gradient(135deg, transparent 0 6px, var(--color-surface-hover) 6px 8px);
}
.wf-node.st-reused { opacity: 0.7; }
.wf-status.st-running, .wf-status.st-ready { color: var(--color-info); border-color: var(--color-info); }
.wf-status.st-waiting_human, .wf-status.st-waiting_children { color: var(--color-warning); border-color: var(--color-warning-border); }
.wf-status.st-succeeded { color: var(--color-success); border-color: var(--color-success-border); }
.wf-status.st-failed { color: var(--color-error); border-color: var(--color-error-border); }
@keyframes wf-pulse {
  0%, 100% { box-shadow: 0 0 0 0 var(--color-info-subtle); }
  50% { box-shadow: 0 0 0 4px var(--color-info-subtle); }
}

.wf-ports { display: grid; grid-template-columns: 1fr 1fr; padding: var(--spacing-1) 0 var(--spacing-2); border-top: 1px solid var(--color-border-light); }
.wf-col { display: grid; align-content: start; min-width: 0; }
.wf-port { position: relative; display: flex; align-items: center; height: 22px; padding: 0 var(--spacing-2-5); font-size: var(--font-size-xs); color: var(--color-text-secondary); min-width: 0; }
.wf-port.out { justify-content: flex-end; }
.wf-port-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.req { color: var(--color-error); font-style: normal; margin-left: 2px; }

.wf-handle {
  width: 9px; height: 9px; min-width: 9px; min-height: 9px;
  background: var(--port-color, var(--color-text-tertiary));
  border: 2px solid var(--color-surface-raised);
  border-radius: var(--radius-full);
}
.wf-node.readonly .wf-handle { pointer-events: none; }
.wf-handle.ctrl { background: var(--color-text-tertiary); border-radius: var(--radius-xs); top: 18px; }
.wf-handle.err { background: var(--color-error); border-radius: var(--radius-xs); width: 8px; height: 8px; min-width: 8px; min-height: 8px; }
.wf-handle.disabled { opacity: 0.2; pointer-events: none; }
</style>
