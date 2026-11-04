<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { Minus, Plus, RotateCcw, X } from "@lucide/vue";
import { t } from "../../i18n";
import { nextPopupZIndex } from "./popup-z-index";

const MIN_SCALE = 0.25;
const MAX_SCALE = 8;
const ZOOM_STEP = 1.25;

const props = defineProps<{
  src: string;
  name: string;
}>();

const emit = defineEmits<{
  close: [];
}>();

const overlayZ = nextPopupZIndex();
const stage = ref<HTMLElement | null>(null);
const scale = ref(1);
const tx = ref(0);
const ty = ref(0);
const dragging = ref(false);
let dragOrigin: { x: number; y: number; tx: number; ty: number } | null = null;
let activePointer: number | null = null;

const transformStyle = computed(() => ({
  transform: `translate(${tx.value}px, ${ty.value}px) scale(${scale.value})`,
}));

const scaleLabel = computed(() => `${Math.round(scale.value * 100)}%`);
const canZoomIn = computed(() => scale.value < MAX_SCALE - 0.001);
const canZoomOut = computed(() => scale.value > MIN_SCALE + 0.001);

function clampScale(value: number) {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, value));
}

function resetView() {
  scale.value = 1;
  tx.value = 0;
  ty.value = 0;
}

function zoomAt(nextScale: number, clientX?: number, clientY?: number) {
  const root = stage.value;
  const next = clampScale(nextScale);
  const prev = scale.value;
  if (next === prev) return;
  if (!root || clientX == null || clientY == null) {
    scale.value = next;
    return;
  }
  const rect = root.getBoundingClientRect();
  const mx = clientX - rect.left - rect.width / 2;
  const my = clientY - rect.top - rect.height / 2;
  const ratio = next / prev;
  tx.value = mx - (mx - tx.value) * ratio;
  ty.value = my - (my - ty.value) * ratio;
  scale.value = next;
}

function zoomBy(factor: number) {
  const root = stage.value;
  if (!root) {
    scale.value = clampScale(scale.value * factor);
    return;
  }
  const rect = root.getBoundingClientRect();
  zoomAt(scale.value * factor, rect.left + rect.width / 2, rect.top + rect.height / 2);
}

function close() {
  emit("close");
}

function onBackdropClick(event: MouseEvent) {
  if (event.target === event.currentTarget) close();
}

function onWheel(event: WheelEvent) {
  event.preventDefault();
  const factor = event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
  zoomAt(scale.value * factor, event.clientX, event.clientY);
}

function onPointerDown(event: PointerEvent) {
  if (event.button !== 0) return;
  event.preventDefault();
  activePointer = event.pointerId;
  dragging.value = true;
  dragOrigin = { x: event.clientX, y: event.clientY, tx: tx.value, ty: ty.value };
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function onPointerMove(event: PointerEvent) {
  if (!dragOrigin || activePointer !== event.pointerId) return;
  tx.value = dragOrigin.tx + (event.clientX - dragOrigin.x);
  ty.value = dragOrigin.ty + (event.clientY - dragOrigin.y);
}

function endDrag(event: PointerEvent) {
  if (activePointer !== event.pointerId) return;
  activePointer = null;
  dragging.value = false;
  dragOrigin = null;
}

function onDoubleClick(event: MouseEvent) {
  if (scale.value > 1.01) {
    resetView();
    return;
  }
  zoomAt(2, event.clientX, event.clientY);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    close();
    return;
  }
  if (event.key === "+" || event.key === "=") {
    event.preventDefault();
    zoomBy(ZOOM_STEP);
    return;
  }
  if (event.key === "-" || event.key === "_") {
    event.preventDefault();
    zoomBy(1 / ZOOM_STEP);
    return;
  }
  if (event.key === "0") {
    event.preventDefault();
    resetView();
  }
}

watch(
  () => props.src,
  () => resetView(),
);

window.addEventListener("keydown", onKeydown, true);
onBeforeUnmount(() => window.removeEventListener("keydown", onKeydown, true));
</script>

<template>
  <Teleport to="body">
    <div
      class="image-preview-lightbox"
      role="dialog"
      aria-modal="true"
      :aria-label="name"
      :style="{ zIndex: overlayZ }"
      @click="onBackdropClick"
    >
      <div class="image-preview-toolbar" @click.stop>
        <UiTooltip :content="t('runLog.zoomOut')">
          <button type="button" :disabled="!canZoomOut" @click="zoomBy(1 / ZOOM_STEP)">
            <Minus :size="14" />
          </button>
        </UiTooltip>
        <span>{{ scaleLabel }}</span>
        <UiTooltip :content="t('runLog.zoomIn')">
          <button type="button" :disabled="!canZoomIn" @click="zoomBy(ZOOM_STEP)">
            <Plus :size="14" />
          </button>
        </UiTooltip>
        <UiTooltip :content="t('chat.imagePreview.reset')">
          <button type="button" @click="resetView">
            <RotateCcw :size="14" />
          </button>
        </UiTooltip>
        <UiTooltip :content="t('common.close')">
          <button type="button" @click="close">
            <X :size="14" />
          </button>
        </UiTooltip>
      </div>
      <div
        ref="stage"
        class="image-preview-stage"
        :class="{ 'is-dragging': dragging }"
        @click.stop
        @wheel.prevent="onWheel"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="endDrag"
        @pointercancel="endDrag"
        @dblclick="onDoubleClick"
      >
        <img :src="src" :alt="name" draggable="false" :style="transformStyle" />
      </div>
    </div>
  </Teleport>
</template>
