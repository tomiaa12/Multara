<script setup lang="ts">
// 全局的 Canvas iframe 层（挂在 App 里，keep-alive 外面）。
// keep-alive 隐藏页面时会把 DOM 挪进脱离文档的容器，iframe 一脱离就会卸载、SSE 断开；
// 放在这里的 iframe 不跟着页面走：前台时盖在显示位置（UiFrame 的 slot）上，切走后藏起来但继续连着，
// 智能体的 canvas action 照样能执行。后台个数和时长有上限（stores/canvas.ts 的 pruneLive）。
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useCanvasStore, type LiveFrame } from "../stores/canvas";

const canvas = useCanvasStore();
/** 每个活页面当前的位置；null = 藏起来 */
const rects = ref<Record<string, { top: number; left: number; width: number; height: number } | null>>({});
const frames = new Map<string, HTMLIFrameElement>();
let raf = 0;
let pruneTimer = 0;
let themeObserver: MutationObserver | null = null;

const entries = computed(() => Object.values(canvas.live));

function src(entry: LiveFrame) {
  return `${entry.url}${entry.url.includes("?") ? "&" : "?"}theme=${entry.theme}`;
}

/** 每帧把可见的 iframe 对齐到它的 slot；位置不变时不写 DOM */
function measure() {
  const next: typeof rects.value = {};
  let changed = false;
  for (const entry of entries.value) {
    const slot = canvas.visibleSlot(entry);
    const box = slot?.getBoundingClientRect();
    const rect = box && box.width > 0 && box.height > 0
      ? { top: box.top, left: box.left, width: box.width, height: box.height }
      : null;
    const previous = rects.value[entry.key];
    if (
      !previous !== !rect
      || (previous && rect && (previous.top !== rect.top || previous.left !== rect.left
        || previous.width !== rect.width || previous.height !== rect.height))
    ) {
      changed = true;
    }
    next[entry.key] = rect;
  }
  if (changed || Object.keys(next).length !== Object.keys(rects.value).length) rects.value = next;
  raf = requestAnimationFrame(measure);
}

function styleFor(entry: LiveFrame) {
  const rect = rects.value[entry.key];
  if (!rect) {
    // 藏起来但留在文档里：不卸载、连接不断；visibility 隐藏也不占交互
    return { visibility: "hidden" as const, top: "0px", left: "0px", width: "1px", height: "1px" };
  }
  return {
    visibility: "visible" as const,
    top: `${rect.top}px`,
    left: `${rect.left}px`,
    width: `${rect.width}px`,
    height: `${rect.height}px`,
  };
}

function bind(key: string, el: unknown) {
  if (el instanceof HTMLIFrameElement) frames.set(key, el);
  else frames.delete(key);
}

function themeColors() {
  const styles = getComputedStyle(document.documentElement);
  const read = (name: string) => styles.getPropertyValue(name).trim();
  return {
    background: read("--color-bg"),
    surface: read("--color-surface"),
    text: read("--color-text"),
    textSecondary: read("--color-text-secondary"),
    border: read("--color-border"),
    primary: read("--color-primary"),
    success: read("--color-success"),
    error: read("--color-error"),
    warning: read("--color-warning"),
  };
}

function currentTheme() {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

/** 切主题时通知所有活页面（单向；Canvas 不接收页面发来的 postMessage） */
function postTheme(entry: LiveFrame) {
  const target = frames.get(entry.key)?.contentWindow;
  if (!target) return;
  target.postMessage(
    { type: "multara:theme", theme: currentTheme(), colors: themeColors() },
    new URL(entry.url).origin,
  );
}

function onLoad(entry: LiveFrame) {
  canvas.markLoaded(entry.key, entry.frameKey);
  if (entry.theme !== currentTheme()) postTheme(entry);
}

onMounted(() => {
  raf = requestAnimationFrame(measure);
  pruneTimer = window.setInterval(() => canvas.pruneLive(), 30_000);
  themeObserver = new MutationObserver(() => {
    for (const entry of entries.value) postTheme(entry);
  });
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
});

onBeforeUnmount(() => {
  cancelAnimationFrame(raf);
  window.clearInterval(pruneTimer);
  themeObserver?.disconnect();
});
</script>

<template>
  <div class="canvas-frame-host" aria-hidden="false">
    <iframe
      v-for="entry in entries"
      :key="`${entry.key}#${entry.frameKey}`"
      :ref="(el) => bind(entry.key, el)"
      class="canvas-live-frame"
      :style="styleFor(entry)"
      :src="src(entry)"
      :title="entry.title"
      sandbox="allow-scripts allow-forms allow-same-origin"
      referrerpolicy="no-referrer"
      allow=""
      :tabindex="rects[entry.key] ? 0 : -1"
      @load="onLoad(entry)"
    />
  </div>
</template>
