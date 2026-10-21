<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { createTextMeasurer, fitMiddleEllipsis } from "./middle-ellipsis-path";
import UiTooltip from "./UiTooltip.vue";

/** 占满宽度；超宽时按像素动态中间省略。悬停用 UiTooltip 显示全路径。 */
const props = withDefaults(defineProps<{
  path: string;
  tag?: string;
}>(), {
  tag: "span",
});

const root = ref<HTMLElement | null>(null);
const display = ref(props.path);
const tip = computed(() => (display.value === props.path ? "" : props.path));

let measure: ((value: string) => number) | null = null;
let observer: ResizeObserver | null = null;

function relayout() {
  const el = root.value;
  if (!el) return;
  measure ??= createTextMeasurer(el);
  const width = el.clientWidth;
  display.value = width > 0
    ? fitMiddleEllipsis(props.path, width, measure)
    : props.path;
}

onMounted(() => {
  const el = root.value;
  if (!el) return;
  measure = createTextMeasurer(el);
  observer = new ResizeObserver(() => relayout());
  observer.observe(el);
  relayout();
});

onBeforeUnmount(() => {
  observer?.disconnect();
  observer = null;
  measure = null;
});

watch(() => props.path, relayout);
</script>

<template>
  <UiTooltip :content="tip" :disabled="!tip">
    <component :is="tag" ref="root" class="ds-path-text">{{ display }}</component>
  </UiTooltip>
</template>
