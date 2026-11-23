<script setup lang="ts">
import { computed } from "vue";
import { BaseEdge, getBezierPath, Position } from "@vue-flow/core";
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
const taken = computed(() => overlay.value?.state === "live");
const style = computed(() => ({
  stroke: taken.value ? "var(--color-info)" : stroke.value,
  strokeWidth: props.selected || taken.value ? 2.5 : 1.6,
  opacity: overlay.value?.state === "dead" ? 0.22 : 1,
  strokeDasharray: taken.value ? undefined : props.data?.control ? "4 3" : undefined,
}));
</script>

<template>
  <BaseEdge :id="id" :path="path[0]" :style="style" :interaction-width="18" />
  <path v-if="overlay?.flowing" class="wf-edge-flow" :d="path[0]" fill="none" />
</template>

<style>
.wf-edge-flow {
  stroke: var(--color-info);
  stroke-width: 2;
  stroke-dasharray: 6 8;
  stroke-linecap: round;
  pointer-events: none;
  animation: wf-edge-flow 0.8s linear infinite;
}
@keyframes wf-edge-flow {
  to { stroke-dashoffset: -28px; }
}
</style>
