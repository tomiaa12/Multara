<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import Codicon from "../ui/Codicon.vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import { fileNameOf } from "../workbench/workspace-mode";
import { gitStatusClass } from "../workbench/tree-git-status";
import {
  CommitMultiDiffPool,
  type MultiDiffPoolFile,
} from "./commit-multi-diff-pool";

export type CommitDiffFile = {
  path: string;
  status: string;
  original: string;
  modified: string;
  language: string;
  binary: boolean;
};

const props = defineProps<{
  diffId: string;
  files: CommitDiffFile[];
  theme: string;
}>();

/** 对齐 VS Code：header 高度 + 未测量前默认内容高度 */
const HEADER_HEIGHT = 28;
const DEFAULT_CONTENT_HEIGHT = 500;
const ITEM_GAP = 8;
const OVERSCAN_PX = 240;

const rootEl = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);
/** path → 是否折叠 */
const collapsed = reactive(new Set<string>());
/** path → 已测内容高度（release 时保留，对齐 lastTemplateData） */
const contentHeights = reactive<Record<string, number>>({});

let pool: CommitMultiDiffPool | null = null;
const bodyEls = new Map<string, HTMLElement>();
let resizeObserver: ResizeObserver | null = null;

const layout = computed(() => {
  let top = 0;
  return props.files.map((file) => {
    const folded = collapsed.has(file.path);
    const content = folded ? 0 : contentHeights[file.path] ?? DEFAULT_CONTENT_HEIGHT;
    const height = HEADER_HEIGHT + content;
    const item = { file, top, height, content, folded };
    top += height + ITEM_GAP;
    return item;
  });
});

const totalHeight = computed(() => {
  const last = layout.value[layout.value.length - 1];
  return last ? last.top + last.height : 0;
});

const visibleLayout = computed(() => {
  const start = scrollTop.value - OVERSCAN_PX;
  const end = scrollTop.value + viewportHeight.value + OVERSCAN_PX;
  return layout.value.filter((item) => item.top + item.height > start && item.top < end);
});

watch(
  () => props.files.map((file) => file.path).join("\0"),
  () => {
    for (const path of [...collapsed]) {
      if (!props.files.some((file) => file.path === path)) collapsed.delete(path);
    }
    for (const path of Object.keys(contentHeights)) {
      if (!props.files.some((file) => file.path === path)) delete contentHeights[path];
    }
    pool?.releaseAll();
    void nextTick(syncBindings);
  },
);

watch(
  () => props.theme,
  (theme) => pool?.setTheme(theme),
);

watch(visibleLayout, () => void nextTick(syncBindings), { flush: "post" });

function letter(status: string) {
  return status === "?" ? "U" : status || "M";
}

function toggle(path: string) {
  if (collapsed.has(path)) collapsed.delete(path);
  else {
    collapsed.add(path);
    pool?.release(path);
    bodyEls.delete(path);
  }
  void nextTick(syncBindings);
}

function stickyOffset(item: { top: number; height: number; folded: boolean }) {
  if (item.folded) return 0;
  const maxDelta = Math.max(0, item.height - HEADER_HEIGHT);
  return Math.max(0, Math.min(scrollTop.value - item.top, maxDelta));
}

function poolFile(file: CommitDiffFile): MultiDiffPoolFile {
  return {
    diffId: `${props.diffId}:${file.path}`,
    path: file.path,
    original: file.original,
    modified: file.modified,
    language: file.language,
    binary: file.binary,
  };
}

function setBodyEl(path: string, el: Element | null) {
  if (!(el instanceof HTMLElement)) {
    if (bodyEls.get(path)) {
      pool?.release(path);
      bodyEls.delete(path);
    }
    return;
  }
  bodyEls.set(path, el);
  void nextTick(syncBindings);
}

function syncBindings() {
  if (!pool) return;
  const want = new Set(
    visibleLayout.value.filter((item) => !item.folded).map((item) => item.file.path),
  );
  for (const path of [...bodyEls.keys()]) {
    if (!want.has(path)) {
      pool.release(path);
      bodyEls.delete(path);
    }
  }
  for (const item of visibleLayout.value) {
    if (item.folded) continue;
    const body = bodyEls.get(item.file.path);
    if (!body) continue;
    pool.bind(body, poolFile(item.file));
  }
}

function onScroll() {
  const root = rootEl.value;
  if (!root) return;
  scrollTop.value = root.scrollTop;
  viewportHeight.value = root.clientHeight;
}

function measureViewport() {
  const root = rootEl.value;
  if (!root) return;
  viewportHeight.value = root.clientHeight;
  scrollTop.value = root.scrollTop;
}

onMounted(() => {
  pool = new CommitMultiDiffPool(props.theme, (path, height) => {
    if (contentHeights[path] === height) return;
    contentHeights[path] = height;
  });
  measureViewport();
  resizeObserver = new ResizeObserver(() => {
    measureViewport();
    void nextTick(syncBindings);
  });
  if (rootEl.value) resizeObserver.observe(rootEl.value);
  void nextTick(syncBindings);
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  resizeObserver = null;
  pool?.dispose();
  pool = null;
  bodyEls.clear();
});
</script>

<template>
  <div ref="rootEl" class="commit-multi-diff vs-scroll" @scroll.passive="onScroll">
    <p v-if="!files.length" class="git-diff-binary">没有可比较的文件。</p>
    <div v-else class="commit-multi-diff-space" :style="{ height: `${totalHeight}px` }">
      <section
        v-for="item in visibleLayout"
        :key="item.file.path"
        class="commit-multi-diff-entry"
        :class="{ 'is-collapsed': item.folded }"
        :style="{ top: `${item.top}px`, height: `${item.height}px` }"
      >
        <header
          class="commit-multi-diff-header"
          role="button"
          tabindex="0"
          :style="{ transform: `translateY(${stickyOffset(item)}px)` }"
          @click="toggle(item.file.path)"
          @keydown.enter.prevent="toggle(item.file.path)"
          @keydown.space.prevent="toggle(item.file.path)"
        >
          <Codicon
            class="commit-multi-diff-twistie"
            :name="item.folded ? 'chevron-right' : 'chevron-down'"
            :size="16"
          />
          <FileTypeIcon :name="fileNameOf(item.file.path)" />
          <span class="commit-multi-diff-name">{{ fileNameOf(item.file.path) }}</span>
          <span class="commit-multi-diff-path">{{ item.file.path }}</span>
          <b class="commit-multi-diff-status" :class="gitStatusClass(letter(item.file.status))">
            {{ letter(item.file.status) }}
          </b>
        </header>
        <div
          v-if="!item.folded"
          :ref="(el) => setBodyEl(item.file.path, el as Element | null)"
          class="commit-multi-diff-body"
          :style="{ height: `${item.content}px` }"
        />
      </section>
    </div>
  </div>
</template>
