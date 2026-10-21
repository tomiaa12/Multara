<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

const props = withDefaults(defineProps<{
  kind?: string;
  active?: boolean;
  items?: number;
}>(), {
  kind: "项目",
  active: true,
  items: 0,
});

const clip = ref<HTMLElement | null>(null);
const extra = ref(0);
const showAll = ref(false);
let observer: ResizeObserver | null = null;
let measuring = false;
let measureTimer = 0;

const moreLabel = computed(() => (showAll.value ? "收起" : `还有 ${extra.value} 个${props.kind}`));

function measure() {
  const root = clip.value;
  if (!root || measuring) return;
  if (!props.active) {
    extra.value = 0;
    root.style.removeProperty("--clip-height");
    return;
  }
  measuring = true;
  const list = root.querySelector(".req-ws-list");
  const children = [...(list?.children ?? [])] as HTMLElement[];
  const keepOpen = showAll.value;
  extra.value = 0;
  root.style.removeProperty("--clip-height");

  if (children.length < 2 || root.offsetParent === null) {
    requestAnimationFrame(() => {
      measuring = false;
    });
    return;
  }

  const firstTop = children[0].offsetTop;
  const row = children.filter((item) => Math.abs(item.offsetTop - firstTop) < 2);
  const leftover = children.length - row.length;
  extra.value = Math.max(0, leftover);
  if (leftover > 0 && !keepOpen) {
    const last = row[row.length - 1];
    root.style.setProperty(
      "--clip-height",
      `${last.offsetTop + last.offsetHeight - root.offsetTop + 12}px`,
    );
  }
  requestAnimationFrame(() => {
    measuring = false;
  });
}

function toggle() {
  showAll.value = !showAll.value;
  nextTick(measure);
}

function scheduleMeasure() {
  nextTick(() => {
    requestAnimationFrame(measure);
  });
}

onMounted(() => {
  scheduleMeasure();
  observer = new ResizeObserver(scheduleMeasure);
  if (clip.value) observer.observe(clip.value);
});

onBeforeUnmount(() => {
  window.clearTimeout(measureTimer);
  observer?.disconnect();
});

watch(
  () => [props.active, props.items, showAll.value],
  () => {
    window.clearTimeout(measureTimer);
    if (!props.active) {
      extra.value = 0;
      return;
    }
    scheduleMeasure();
    measureTimer = window.setTimeout(scheduleMeasure, 240);
  },
);
</script>

<template>
  <div
    ref="clip"
    class="req-clip"
    :class="{ 'has-more': extra > 0, 'is-all': extra > 0 && showAll }"
  >
    <div class="req-ws-list">
      <slot />
    </div>
    <button v-if="extra > 0" class="req-more" type="button" @click.stop="toggle">{{ moreLabel }}</button>
  </div>
</template>
