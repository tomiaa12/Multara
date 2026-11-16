<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

const props = withDefaults(defineProps<{
  /** 折叠预览贴住末尾，看到最新输出，上面被挡住时出渐变。 */
  pinEnd?: boolean;
}>(), { pinEnd: false });

const CLIP_TOLERANCE_PX = 2;

const root = ref<HTMLElement | null>(null);
const clipTop = ref(false);
const clipBottom = ref(false);
let observer: ResizeObserver | undefined;
let mutations: MutationObserver | undefined;

function measure() {
  const el = root.value;
  if (!el) return;
  if (props.pinEnd) el.scrollTop = el.scrollHeight;
  clipTop.value = el.scrollTop > CLIP_TOLERANCE_PX;
  clipBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight > CLIP_TOLERANCE_PX;
}

onMounted(() => {
  measure();
  const el = root.value;
  if (!el) return;
  if (typeof ResizeObserver !== "undefined") {
    observer = new ResizeObserver(() => measure());
    observer.observe(el);
  }
  mutations = new MutationObserver(() => measure());
  mutations.observe(el, { childList: true, subtree: true, characterData: true });
});

onBeforeUnmount(() => {
  observer?.disconnect();
  mutations?.disconnect();
});

watch(() => props.pinEnd, () => {
  void nextTick(measure);
});
</script>

<template>
  <div class="clip-scroll" :class="{ 'has-clip-top': clipTop, 'has-clip-bottom': clipBottom }">
    <div ref="root" class="clip-scroll-view vs-scroll" @scroll.passive="measure">
      <slot />
    </div>
  </div>
</template>
