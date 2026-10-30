<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref } from "vue";
import { nextPopupZIndex } from "./popup-z-index";
import {
  pickTooltipPlacement,
  type TooltipPlacement,
} from "./tooltip-placement";

/** 悬停 / 聚焦弹出 tip。Teleport 到 body，打开时领取递增 z-index（高于 modal）。 */
const props = withDefaults(defineProps<{
  content?: string;
  placement?: TooltipPlacement;
  maxWidth?: number;
  disabled?: boolean;
}>(), {
  content: "",
  placement: "top",
  maxWidth: 320,
  disabled: false,
});

const open = ref(false);
const ready = ref(false);
const triggerEl = ref<HTMLElement | null>(null);
const tipEl = ref<HTMLElement | null>(null);
const tipStyle = ref<Record<string, string>>({});
const resolved = ref<TooltipPlacement>(props.placement);
let hideTimer = 0;
let positionFrame = 0;

async function show() {
  if (props.disabled || !props.content.trim()) return;
  window.clearTimeout(hideTimer);
  open.value = true;
  ready.value = false;
  tipStyle.value = {
    zIndex: String(nextPopupZIndex()),
    maxWidth: `${props.maxWidth}px`,
  };
  await nextTick();
  cancelAnimationFrame(positionFrame);
  positionFrame = requestAnimationFrame(() => {
    positionFrame = 0;
    updatePosition();
    ready.value = true;
  });
}

function hide() {
  window.clearTimeout(hideTimer);
  hideTimer = window.setTimeout(() => {
    open.value = false;
    ready.value = false;
  }, 60);
}

function updatePosition() {
  const trigger = triggerEl.value?.getBoundingClientRect();
  const tip = tipEl.value?.getBoundingClientRect();
  if (!trigger || !tip?.width || !tip.height) return;
  const next = pickTooltipPlacement(
    trigger,
    { width: tip.width, height: tip.height },
    props.placement,
  );
  resolved.value = next.placement;
  tipStyle.value = {
    ...tipStyle.value,
    top: `${next.top}px`,
    left: `${next.left}px`,
    maxWidth: `${props.maxWidth}px`,
  };
}

onBeforeUnmount(() => {
  window.clearTimeout(hideTimer);
  cancelAnimationFrame(positionFrame);
});
</script>

<template>
  <span
    ref="triggerEl"
    class="ds-tooltip-trigger"
    @mouseenter="show"
    @mouseleave="hide"
    @focusin="show"
    @focusout="hide"
  >
    <slot />
  </span>
  <Teleport to="body">
    <div
      v-if="open && content.trim()"
      ref="tipEl"
      class="ds-tooltip"
      :class="{ 'is-ready': ready }"
      role="tooltip"
      :data-placement="resolved"
      :style="tipStyle"
    >{{ content }}</div>
  </Teleport>
</template>
