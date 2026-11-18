<script setup lang="ts">
import { computed } from "vue";
import { BaseEdge, EdgeLabelRenderer, getBezierPath, Position } from "@vue-flow/core";
import { PORT_COLORS, type PortType } from "../../../domain/workflow";
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
  data?: { type?: PortType; control?: boolean; overlay?: EdgeOverlay; severity?: "error" | "warning" | null };
}>();

const path = computed(() => getBezierPath({
  sourceX: props.sourceX,
  sourceY: props.sourceY,
  sourcePosition: props.sourcePosition,
  targetX: props.targetX,
  targetY: props.targetY,
  targetPosition: props.targetPosition,
}));
const overlay = computed(() => props.data?.overlay);
const stroke = computed(() => {
  if (props.data?.severity === "error") return "var(--color-error)";
  if (props.data?.control || !props.data?.type) return "var(--color-text-tertiary)";
  return PORT_COLORS[props.data.type];
});
const style = computed(() => ({
  stroke: stroke.value,
  strokeWidth: props.selected || overlay.value?.state === "live" ? 2.5 : 1.6,
  opacity: overlay.value?.state === "dead" ? 0.22 : 1,
  strokeDasharray: props.data?.control ? "4 3" : undefined,
}));
</script>

<template>
  <BaseEdge :id="id" :path="path[0]" :style="style" :interaction-width="18" />
  <EdgeLabelRenderer v-if="overlay?.label">
    <div
      class="wf-edge-label"
      :title="overlay.tooltip"
      :style="{ transform: `translate(-50%, -50%) translate(${path[1]}px, ${path[2]}px)` }"
    >{{ overlay.label }}</div>
  </EdgeLabelRenderer>
</template>

<style>
.wf-edge-label {
  position: absolute;
  pointer-events: all;
  max-width: 160px;
  padding: 0 var(--spacing-1-5);
  border-radius: var(--radius-full);
  background: var(--color-surface-raised);
  border: 1px solid var(--color-border-strong);
  color: var(--color-text-secondary);
  font-size: var(--font-size-xs);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
