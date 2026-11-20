<script setup lang="ts">
import { computed } from "vue";
import { BaseEdge, EdgeLabelRenderer, getBezierPath, Position } from "@vue-flow/core";
import { t } from "../../../i18n";
import type { EdgeOverlay } from "../canvas-types";

const props = defineProps<{
  id: string;
  sourceX: number;
  sourceY: number;
  targetX: number;
  targetY: number;
  sourcePosition: Position;
  targetPosition: Position;
  selected?: boolean;
  data?: { maxIterations?: number; overlay?: EdgeOverlay; severity?: "error" | "warning" | null };
}>();

const path = computed(() => getBezierPath({
  sourceX: props.sourceX,
  sourceY: props.sourceY,
  sourcePosition: props.sourcePosition,
  targetX: props.targetX,
  targetY: props.targetY,
  targetPosition: props.targetPosition,
  curvature: 0.6,
}));
const overlay = computed(() => props.data?.overlay);
const max = computed(() => props.data?.maxIterations ?? 0);
const badge = computed(() =>
  overlay.value?.count != null ? `${overlay.value.count}/${max.value}` : t("workflow.canvas.maxRounds", { count: max.value }),
);
const style = computed(() => ({
  stroke: props.data?.severity === "error" ? "var(--color-error)" : "var(--color-warning)",
  strokeWidth: props.selected || overlay.value?.state === "live" ? 2.5 : 1.6,
  strokeDasharray: "6 4",
  opacity: overlay.value?.state === "dead" ? 0.3 : 1,
}));
</script>

<template>
  <BaseEdge :id="id" :path="path[0]" :style="style" :interaction-width="18" />
  <EdgeLabelRenderer>
    <div
      class="wf-back-badge"
      :class="{ active: overlay?.count != null && overlay.count > 0 }"
      :style="{ transform: `translate(-50%, -50%) translate(${path[1]}px, ${path[2]}px)` }"
      :title="overlay?.tooltip"
    >{{ badge }}</div>
  </EdgeLabelRenderer>
</template>

<style>
.wf-back-badge {
  position: absolute;
  pointer-events: all;
  padding: 0 var(--spacing-1-5);
  border-radius: var(--radius-full);
  background: var(--color-warning-subtle);
  border: 1px solid var(--color-warning-border);
  color: var(--color-warning);
  font-size: var(--font-size-xs);
  line-height: 16px;
  white-space: nowrap;
}
.wf-back-badge.active { background: var(--color-warning); color: var(--color-bg); }
</style>
