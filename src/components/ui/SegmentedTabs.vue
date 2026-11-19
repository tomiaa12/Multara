<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

export type SegmentedTab = {
  id: string;
  label: string;
  count?: number;
  /** 一直显示数字。其余项只在选中时显示。 */
  pinCount?: boolean;
};

const props = defineProps<{
  modelValue: string;
  items: SegmentedTab[];
  label: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

function showCount(item: SegmentedTab) {
  if (item.count == null) return false;
  return Boolean(item.pinCount) || props.modelValue === item.id;
}

/* 选中底块：单独一个元素，切换时滑到新位置（而不是旧的消失、新的出现）。
 * 首次量到位置前不加过渡，避免从左上角滑进来。 */
const navEl = ref<HTMLElement | null>(null);
const indicator = ref<{ x: number; width: number } | null>(null);
const indicatorReady = ref(false);
let observer: ResizeObserver | null = null;

function measure() {
  const nav = navEl.value;
  const active = nav?.querySelector<HTMLElement>(":scope > button.active");
  if (!nav || !active) {
    indicator.value = null;
    return;
  }
  // 用 rect 差值而不是 offsetLeft：首次量时 nav 还没 position:relative，offsetParent 不是它
  const x = active.getBoundingClientRect().left - nav.getBoundingClientRect().left - nav.clientLeft;
  indicator.value = { x, width: active.offsetWidth };
  if (!indicatorReady.value) requestAnimationFrame(() => (indicatorReady.value = true));
}

watch(
  () => [props.modelValue, props.items.map((item) => `${item.id}:${item.label}:${item.count ?? ""}`).join("|")],
  () => void nextTick(measure),
);

onMounted(() => {
  measure();
  if (typeof ResizeObserver === "undefined" || !navEl.value) return;
  observer = new ResizeObserver(() => measure());
  observer.observe(navEl.value);
  for (const button of navEl.value.querySelectorAll(":scope > button")) observer.observe(button);
});
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <nav ref="navEl" class="ds-segmented" :class="{ 'has-indicator': indicator }" :aria-label="label">
    <span
      v-if="indicator"
      class="ds-segmented-indicator"
      :class="{ 'is-ready': indicatorReady }"
      aria-hidden="true"
      :style="{ width: `${indicator.width}px`, transform: `translateX(${indicator.x}px)` }"
    />
    <button
      v-for="item in items"
      :key="item.id"
      type="button"
      role="tab"
      :aria-selected="modelValue === item.id"
      :class="{ active: modelValue === item.id }"
      @click="emit('update:modelValue', item.id)"
    >
      <slot name="leading" :item="item" />
      {{ item.label }}<em v-if="showCount(item)">{{ item.count }}</em>
    </button>
  </nav>
</template>
