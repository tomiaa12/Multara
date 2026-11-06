<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import Codicon from "../ui/Codicon.vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import UiButton from "../ui/UiButton.vue";
import UiPane from "../ui/UiPane.vue";
import UiSelect from "../ui/UiSelect.vue";
import type { SelectOption } from "../ui/select-types";
import type { GitRepositoryStatus, Workspace } from "../../domain/types";
import * as backend from "../../services/backend";
import { t } from "../../i18n";
import {
  buildTreemapFromEntries,
  countTreemapFiles,
  extensionHue,
  filterEntriesByScope,
  findTreemapPath,
  loadCommitFrequencies,
  treemapLeafValue,
  type TreemapMode,
  type VisualHistoryScope,
} from "./scm-inspect";
import ScmInspectBranchPicker from "./ScmInspectBranchPicker.vue";
import { PERIOD_OPTIONS, periodToMs, type TimelinePeriod } from "./scm-timeline-chart";
import { descendants, leaves, squarify, type TreemapRect } from "./squarify";
import type { TreemapInput } from "./squarify";

const props = withDefaults(defineProps<{
  mode: TreemapMode;
  title: string;
  storageKey: string;
  sizeKey: string;
  workspaceId: string;
  epoch: number;
  path?: string;
  branch?: string;
  isolationMode?: string;
  defaultScope?: VisualHistoryScope;
  workspaces?: Workspace[];
  repositories?: GitRepositoryStatus[];
  pinned?: boolean;
}>(), {
  path: "",
  branch: "",
  isolationMode: "",
  defaultScope: "repo",
  workspaces: () => [],
  repositories: () => [],
  pinned: false,
});

const emit = defineEmits<{
  openFile: [path: string];
  selectWorkspace: [id: string];
  togglePin: [];
}>();

const host = ref<HTMLElement | null>(null);
const canvas = ref<HTMLCanvasElement | null>(null);
const root = ref<TreemapInput | null>(null);
const layout = ref<TreemapRect<TreemapInput> | null>(null);
const loading = ref(false);
const tip = ref<{ x: number; y: number; path: string; value: number; folder?: boolean } | null>(null);
const hover = ref<TreemapRect<TreemapInput> | null>(null);
const zoomPath = ref<TreemapInput[]>([]);
const maxFreq = ref(1);
const scope = ref<VisualHistoryScope>(props.defaultScope === "current" ? "current" : "repo");
/** 提交矩形图时间窗，对齐 GitLens 默认 3 个月。 */
const period = ref<TimelinePeriod>("3|M");
let ticket = 0;
let resizeObs: ResizeObserver | null = null;
let tree: TreemapInput | null = null;
let layoutKey = "";

const zoomedRoot = computed(() => zoomPath.value.at(-1) ?? tree);
const crumbs = computed(() => {
  if (!tree || !zoomPath.value.length) return [] as TreemapInput[];
  return zoomPath.value;
});
const pathParts = computed(() => {
  if (scope.value === "repo" || !props.path) return [] as string[];
  return props.path.split("/").filter(Boolean);
});
const periodOptions = computed<SelectOption<TimelinePeriod>[]>(() =>
  PERIOD_OPTIONS.map((item) => ({ value: item.id, label: t(item.labelKey) })),
);
const scopeFileCount = computed(() => countTreemapFiles(zoomedRoot.value));
const countLabel = computed(() => {
  const n = scopeFileCount.value;
  if (!n) return "";
  return t("scm.inspect.treemapFiles", { n });
});
const paneTitle = computed(() => (
  props.pinned ? t("scm.inspect.pinnedTitle", { title: props.title }) : props.title
));
const emptyText = computed(() => {
  if (!props.workspaceId) return t("scm.inspect.needWorkspace");
  if (scope.value === "current" && !props.path) return t("scm.inspect.needFile");
  if (!loading.value && !root.value) return t("scm.inspect.emptyTreemap");
  return "";
});

function applyTree(
  files: Parameters<typeof buildTreemapFromEntries>[0],
  freq: Map<string, number> | undefined,
  resetZoom: boolean,
) {
  if (freq) {
    let max = 1;
    for (const value of freq.values()) max = Math.max(max, value);
    maxFreq.value = max;
  } else {
    maxFreq.value = 1;
  }
  tree = buildTreemapFromEntries(files, props.mode === "commits" ? freq : undefined);
  root.value = tree;
  layoutKey = "";
  layout.value = null;
  if (resetZoom) zoomPath.value = [];
  else if (zoomPath.value.length && tree) {
    // 热度回填后尽量保持当前下钻路径。
    const next: typeof zoomPath.value = [];
    let cursor: typeof tree | null = tree;
    for (const step of zoomPath.value) {
      const hit = cursor?.children?.find((child) => child.path === step.path) ?? null;
      if (!hit) break;
      next.push(hit);
      cursor = hit;
    }
    zoomPath.value = next;
  }
}

async function load() {
  const id = ++ticket;
  loading.value = true;
  if (!props.workspaceId) {
    root.value = null;
    layout.value = null;
    tree = null;
    zoomPath.value = [];
    loading.value = false;
    return;
  }
  // 默认整仓；仅「当前文件」且没有路径时才空态，避免误进 current 后整图不显示。
  if (scope.value === "current" && !props.path) {
    root.value = null;
    layout.value = null;
    tree = null;
    zoomPath.value = [];
    loading.value = false;
    return;
  }
  try {
    const entries = await backend.listWorkspaceTree(props.workspaceId);
    if (id !== ticket) return;
    const scoped = filterEntriesByScope(entries, scope.value, props.path || "");
    const files = scoped.filter((item) => !item.isDirectory).slice(0, 4000);
    // 先出树再补提交热度，避免等 gitCommitFiles 时长时间空白。
    applyTree(files, undefined, true);
    await nextTick();
    paint();
    loading.value = false;
    if (props.mode !== "commits") return;
    const span = periodToMs(period.value);
    const sinceMs = span == null ? null : Date.now() - span;
    const freq = await loadCommitFrequencies(props.workspaceId, sinceMs);
    if (id !== ticket) return;
    applyTree(files, freq, false);
    await nextTick();
    paint();
  } catch {
    if (id !== ticket) return;
    root.value = null;
    layout.value = null;
    tree = null;
    zoomPath.value = [];
  } finally {
    if (id === ticket) loading.value = false;
  }
}

function ensureLayout(width: number, height: number): TreemapRect<TreemapInput> | null {
  const scopeRoot = zoomedRoot.value;
  if (!scopeRoot) {
    layout.value = null;
    layoutKey = "";
    return null;
  }
  const key = `${scopeRoot.path || "$"}::${width}x${height}`;
  if (layout.value && layoutKey === key) return layout.value;
  layoutKey = key;
  layout.value = squarify(scopeRoot, treemapLeafValue, {
    width,
    height,
    paddingOuter: 3,
    paddingTop: 20,
    paddingInner: 1,
    round: true,
  });
  return layout.value;
}

function paint() {
  const box = host.value;
  const el = canvas.value;
  if (!box || !el || !zoomedRoot.value) {
    layout.value = null;
    return;
  }
  const width = Math.max(40, box.clientWidth);
  const height = Math.max(80, box.clientHeight);
  const next = ensureLayout(width, height);
  if (!next) return;

  const dpr = window.devicePixelRatio || 1;
  const targetW = Math.round(width * dpr);
  const targetH = Math.round(height * dpr);
  if (el.width !== targetW || el.height !== targetH) {
    el.width = targetW;
    el.height = targetH;
  }
  el.style.width = `${width}px`;
  el.style.height = `${height}px`;
  const ctx = el.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  drawFolders(ctx, next);
  drawLeaves(ctx, next);
}

function leafColor(node: TreemapRect<TreemapInput>, hovered: boolean): string {
  if (props.mode === "commits") {
    const count = node.value;
    if (maxFreq.value <= 0 || count <= 0) {
      return hovered ? "rgba(80, 80, 80, 0.5)" : "rgba(50, 50, 50, 0.4)";
    }
    const ratio = Math.log1p(count) / Math.log1p(maxFreq.value);
    const boost = hovered ? 15 : 0;
    if (ratio < 0.01) {
      return hovered ? "rgba(60, 70, 60, 0.5)" : "rgba(40, 50, 40, 0.35)";
    }
    return `hsla(${120 - ratio * 120}, ${60 + ratio * 20}%, ${25 + ratio * 20 + boost}%, ${0.5 + ratio * 0.35})`;
  }
  const hue = extensionHue(node.data.name || node.data.path);
  return `hsla(${hue}, 50%, ${hovered ? 50 : 38}%, ${hovered ? 0.8 : 0.65})`;
}

function drawFolders(ctx: CanvasRenderingContext2D, rootRect: TreemapRect<TreemapInput>) {
  for (const node of descendants(rootRect)) {
    if (node.children.length === 0 || node === rootRect) continue;
    const w = node.x1 - node.x0;
    const h = node.y1 - node.y0;
    if (w < 2 || h < 2) continue;
    const isHovered = node === hover.value;
    if (isHovered) {
      ctx.fillStyle = "rgba(255, 255, 255, 0.08)";
      ctx.fillRect(node.x0, node.y0, w, h);
      ctx.strokeStyle = "rgba(255, 255, 255, 0.5)";
      ctx.lineWidth = 2;
      ctx.strokeRect(node.x0, node.y0, w, h);
    } else {
      ctx.fillStyle = `rgba(255, 255, 255, ${0.02 + node.depth * 0.008})`;
      ctx.fillRect(node.x0, node.y0, w, h);
      ctx.strokeStyle = `rgba(255, 255, 255, ${0.06 + node.depth * 0.03})`;
      ctx.lineWidth = 1;
      ctx.strokeRect(node.x0, node.y0, w, h);
    }
    ctx.lineWidth = 1;
    if (w > 30 && node.data.name) {
      ctx.fillStyle = isHovered ? "rgba(255, 255, 255, 0.9)" : "rgba(255, 255, 255, 0.6)";
      ctx.font = "11px var(--font-family, sans-serif)";
      ctx.save();
      ctx.beginPath();
      ctx.rect(node.x0, node.y0, w, 18);
      ctx.clip();
      ctx.fillText(node.data.name, node.x0 + 4, node.y0 + 14);
      ctx.restore();
    }
  }
}

function drawLeaves(ctx: CanvasRenderingContext2D, rootRect: TreemapRect<TreemapInput>) {
  for (const node of leaves(rootRect)) {
    if (node.data.type !== "file") continue;
    const w = node.x1 - node.x0;
    const h = node.y1 - node.y0;
    if (w < 1 || h < 1) continue;
    const isHovered = node === hover.value;
    ctx.fillStyle = leafColor(node, isHovered);
    ctx.fillRect(node.x0, node.y0, w, h);
    ctx.strokeStyle = isHovered ? "rgba(255, 255, 255, 0.8)" : "rgba(255, 255, 255, 0.12)";
    ctx.lineWidth = isHovered ? 2 : 1;
    ctx.strokeRect(node.x0, node.y0, w, h);
    ctx.lineWidth = 1;
    if (w > 40 && h > 16) {
      ctx.fillStyle = isHovered ? "rgba(255, 255, 255, 1)" : "rgba(255, 255, 255, 0.85)";
      ctx.font = `${h > 30 ? 11 : 10}px var(--font-family, sans-serif)`;
      ctx.save();
      ctx.beginPath();
      ctx.rect(node.x0 + 2, node.y0 + 2, w - 4, h - 4);
      ctx.clip();
      ctx.fillText(node.data.name, node.x0 + 4, node.y0 + 13);
      ctx.restore();
    }
  }
}

function hitTest(x: number, y: number): TreemapRect<TreemapInput> | null {
  const rootRect = layout.value;
  if (!rootRect) return null;
  let best: TreemapRect<TreemapInput> | null = null;
  for (const node of descendants(rootRect)) {
    if (node === rootRect) continue;
    if (x < node.x0 || x > node.x1 || y < node.y0 || y > node.y1) continue;
    if (!best || node.depth > best.depth) best = node;
  }
  return best;
}

function zoomTo(node: TreemapInput | null) {
  if (!tree) return;
  if (!node || node === tree || !node.path) {
    zoomPath.value = [];
  } else {
    zoomPath.value = findTreemapPath(tree, node);
  }
  layoutKey = "";
  layout.value = null;
  hover.value = null;
  tip.value = null;
  nextTick(() => paint());
}

function onMove(event: MouseEvent) {
  const el = canvas.value;
  const box = host.value?.getBoundingClientRect();
  if (!el || !box) return;
  const x = event.clientX - box.left;
  const y = event.clientY - box.top;
  const hit = hitTest(x, y);
  hover.value = hit;
  el.style.cursor = hit ? "pointer" : "default";
  if (hit) {
    tip.value = {
      x: x + 8,
      y: y + 8,
      path: hit.data.path || hit.data.name,
      value: hit.value,
      folder: hit.data.type === "folder",
    };
  } else {
    tip.value = null;
  }
  paint();
}

function onLeave() {
  hover.value = null;
  tip.value = null;
  if (canvas.value) canvas.value.style.cursor = "default";
  paint();
}

function onClick(event: MouseEvent) {
  const box = host.value?.getBoundingClientRect();
  if (!box) return;
  const hit = hitTest(event.clientX - box.left, event.clientY - box.top);
  if (!hit) return;
  if (hit.data.type === "file" && hit.data.path) {
    emit("openFile", hit.data.path);
    return;
  }
  zoomTo(hit.data);
}

function setScope(next: VisualHistoryScope) {
  scope.value = next;
}

function pathAt(index: number) {
  return pathParts.value.slice(0, index + 1).join("/");
}

watch(
  () => [
    props.workspaceId,
    props.epoch,
    props.mode,
    scope.value,
    period.value,
    scope.value === "current" ? props.path : "",
  ] as const,
  () => void load(),
  { immediate: true },
);

watch(host, (el) => {
  resizeObs?.disconnect();
  resizeObs = null;
  if (!el) return;
  resizeObs = new ResizeObserver(() => paint());
  resizeObs.observe(el);
  nextTick(() => paint());
});

onBeforeUnmount(() => resizeObs?.disconnect());
</script>

<template>
  <UiPane
    :title="paneTitle"
    :storage-key="storageKey"
    :size-key="sizeKey"
    :default-size="220"
    :default-expanded="false"
    :min-body="80"
    :actions-visible="pinned"
  >
    <template #actions>
      <UiButton
        variant="ghost"
        size="icon"
        :tip="pinned ? t('scm.inspect.unpinWorkspace') : t('scm.inspect.pinWorkspace')"
        :aria-pressed="pinned ? 'true' : 'false'"
        @click="emit('togglePin')"
      >
        <Codicon :name="pinned ? 'pinned' : 'pin'" />
      </UiButton>
      <UiButton
        v-if="zoomPath.length"
        variant="ghost"
        size="icon"
        :tip="t('scm.inspect.treemapBack')"
        @click="zoomTo(null)"
      >
        <Codicon name="arrow-left" />
      </UiButton>
      <UiButton
        v-if="path"
        variant="ghost"
        size="icon"
        :tip="t('scm.inspect.revealFile')"
        @click="emit('openFile', path)"
      >
        <Codicon name="go-to-file" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :tip="scope === 'repo' ? t('scm.inspect.scopeCurrent') : t('scm.inspect.scopeRepo')"
        @click="setScope(scope === 'repo' ? 'current' : 'repo')"
      >
        <Codicon :name="scope === 'repo' ? 'file' : 'repo'" />
      </UiButton>
      <UiButton variant="ghost" size="icon" :tip="t('scm.inspect.refresh')" :disabled="loading" @click="load">
        <Codicon name="refresh" />
      </UiButton>
    </template>
    <div class="scm-treemap">
      <div class="scm-inspect-toolbar">
        <div class="scm-inspect-crumbs" role="navigation">
          <ScmInspectBranchPicker
            :workspace-id="workspaceId"
            :workspaces="workspaces"
            :repositories="repositories"
            :pinned="pinned"
            @update:workspace-id="emit('selectWorkspace', $event)"
          />
          <template v-if="scope === 'current' && pathParts.length">
            <template v-for="(part, index) in pathParts" :key="pathAt(index)">
              <span class="scm-inspect-crumb-sep" aria-hidden="true">›</span>
              <button
                type="button"
                class="scm-inspect-crumb"
                :class="{ current: index === pathParts.length - 1 && !crumbs.length }"
                :title="part"
                @click="index === pathParts.length - 1 ? emit('openFile', path) : undefined"
              >
                <FileTypeIcon :name="part" :directory="index < pathParts.length - 1" :size="12" />
                <span class="scm-inspect-crumb-label">{{ part }}</span>
              </button>
            </template>
          </template>
          <template v-else-if="crumbs.length">
            <template v-for="(crumb, index) in crumbs" :key="crumb.path">
              <span class="scm-inspect-crumb-sep" aria-hidden="true">›</span>
              <button
                type="button"
                class="scm-inspect-crumb"
                :class="{ current: index === crumbs.length - 1 }"
                :title="crumb.name"
                @click="zoomTo(crumb)"
              >
                <FileTypeIcon :name="crumb.name" directory :size="12" />
                <span class="scm-inspect-crumb-label">{{ crumb.name }}</span>
              </button>
            </template>
          </template>
        </div>
        <div class="scm-inspect-toolbar-end">
          <span v-if="countLabel" class="scm-inspect-count">{{ countLabel }}</span>
          <UiSelect
            v-if="mode === 'commits'"
            v-model="period"
            class="scm-treemap-period"
            size="sm"
            :searchable="false"
            :options="periodOptions"
            :aria-label="t('scm.inspect.period')"
          />
        </div>
      </div>
      <p v-if="emptyText" class="tree-empty">{{ emptyText }}</p>
      <p v-else-if="loading && !root" class="tree-empty">{{ t("scm.inspect.loading") }}</p>
      <div v-show="root && !emptyText" ref="host" class="scm-treemap-host">
        <canvas
          ref="canvas"
          class="scm-treemap-canvas"
          @mousemove="onMove"
          @mouseleave="onLeave"
          @click="onClick"
        />
        <div
          v-if="tip"
          class="scm-treemap-tip"
          :style="{ left: `${tip.x}px`, top: `${tip.y}px` }"
        >
          <strong>{{ tip.path }}</strong>
          <span v-if="tip.folder">{{ t("scm.inspect.treemapZoomHint") }}</span>
          <span v-else-if="mode === 'commits'">{{ t("scm.inspect.commitHits", { n: tip.value }) }}</span>
          <span v-else>{{ t("scm.inspect.fileBytes", { n: tip.value }) }}</span>
        </div>
      </div>
    </div>
  </UiPane>
</template>
