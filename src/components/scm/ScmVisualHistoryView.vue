<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import Codicon from "../ui/Codicon.vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import UiButton from "../ui/UiButton.vue";
import UiPane from "../ui/UiPane.vue";
import UiSelect from "../ui/UiSelect.vue";
import type { SelectOption } from "../ui/select-types";
import type { GitRepositoryStatus, Workspace } from "../../domain/types";
import * as backend from "../../services/backend";
import { t } from "../../i18n";
import { attachTimelineWeights, historyToTimelinePoints, type TimelinePoint, type VisualHistoryScope } from "./scm-inspect";
import ScmInspectBranchPicker from "./ScmInspectBranchPicker.vue";
import {
  HOVER_ANIM_MS,
  PERIOD_OPTIONS,
  SLICE_PALETTE,
  buildTimelineViewModel,
  computeTimelineLayout,
  drawTimeline,
  filterByPeriod,
  findNearestVolumeCommit,
  hitTestBubble,
  pickRailWidth,
  readTimelineTheme,
  sameBubbleHit,
  type BubbleHit,
  type TimelineCommit,
  type TimelineHoverState,
  type TimelinePeriod,
  type TimelineSliceBy,
  type TimelineViewModel,
} from "./scm-timeline-chart";
import type { ScmOpenTarget } from "../workbench/scm-groups";

const props = withDefaults(defineProps<{
  title: string;
  storageKey: string;
  sizeKey: string;
  workspaceId: string;
  path: string;
  epoch: number;
  branch?: string;
  isolationMode?: string;
  defaultScope?: VisualHistoryScope;
  workspaces?: Workspace[];
  repositories?: GitRepositoryStatus[];
  pinned?: boolean;
}>(), {
  branch: "",
  isolationMode: "",
  defaultScope: "repo",
  workspaces: () => [],
  repositories: () => [],
  pinned: false,
});

const emit = defineEmits<{
  open: [target: ScmOpenTarget];
  openFile: [path: string];
  selectWorkspace: [id: string];
  togglePin: [];
}>();

const scope = ref<VisualHistoryScope>(props.defaultScope === "current" ? "current" : "repo");
const period = ref<TimelinePeriod>("3|M");
const sliceBy = ref<TimelineSliceBy>("author");
const points = ref<TimelinePoint[]>([]);
const loading = ref(false);
const canvasHost = ref<HTMLElement | null>(null);
const canvas = ref<HTMLCanvasElement | null>(null);
const rail = ref<HTMLElement | null>(null);
const tip = ref<{ x: number; y: number; title: string; meta: string } | null>(null);
const scrollY = ref(0);
const viewModel = shallowRef<TimelineViewModel | null>(null);
const layout = ref(computeTimelineLayout(320, 220, 1));
let ticket = 0;
let resizeObs: ResizeObserver | null = null;
// 对齐 GitLens：rAF 驱动 hover intensity，进出交叉淡入淡出。
let hoverHit: BubbleHit | null = null;
let hoverIntensity = 0;
let hoverIntensityTarget = 0;
let outgoingHit: BubbleHit | null = null;
let outgoingIntensity = 0;
let volumeCommitIndex: number | null = null;
let lastFrameTime = 0;
let rafId = 0;

const pathParts = computed(() => {
  if (scope.value === "repo" || !props.path) return [] as string[];
  return props.path.split("/").filter(Boolean);
});

const periodOptions = computed<SelectOption<TimelinePeriod>[]>(() =>
  PERIOD_OPTIONS.map((item) => ({ value: item.id, label: t(item.labelKey) })),
);
const paneTitle = computed(() => (
  props.pinned ? t("scm.inspect.pinnedTitle", { title: props.title }) : props.title
));

const emptyText = computed(() => {
  if (scope.value === "current" && !props.path) return t("scm.inspect.needFile");
  if (!loading.value && !points.value.length) return t("scm.inspect.emptyHistory");
  if (!loading.value && points.value.length && viewModel.value && !viewModel.value.commits.length) {
    return t("scm.inspect.emptyPeriod");
  }
  return "";
});

const railSlices = computed(() => viewModel.value?.slices ?? []);
const showBranchLabels = computed(() => sliceBy.value === "branch");

function toCommits(list: TimelinePoint[]): TimelineCommit[] {
  return list.map((item) => ({
    hash: item.hash,
    subject: item.subject,
    author: item.author,
    at: item.at,
    weight: Math.max(1, item.weight || 1),
    refs: item.refs ?? [],
  }));
}

function rebuildModel() {
  const filtered = filterByPeriod(toCommits(points.value), period.value);
  viewModel.value = buildTimelineViewModel(filtered, sliceBy.value, props.branch || "HEAD");
  scrollY.value = 0;
}

async function load() {
  const id = ++ticket;
  points.value = [];
  viewModel.value = null;
  if (!props.workspaceId) {
    loading.value = false;
    return;
  }
  const path = scope.value === "repo" ? undefined : props.path || undefined;
  if (scope.value === "current" && !path) {
    loading.value = false;
    return;
  }
  loading.value = true;
  try {
    const page = await backend.gitLog(props.workspaceId, 0, 200, path);
    if (id !== ticket) return;
    const base = historyToTimelinePoints(page.items);
    points.value = base;
    rebuildModel();
    await nextTick();
    paint();
    // 权重异步补齐；即便失败也保留 weight=1 的可见气泡。
    const weighted = await attachTimelineWeights(props.workspaceId, base);
    if (id !== ticket) return;
    points.value = weighted;
    rebuildModel();
    await nextTick();
    paint();
  } catch {
    if (id !== ticket) return;
    points.value = [];
    viewModel.value = null;
  } finally {
    if (id === ticket) {
      loading.value = false;
      await nextTick();
      paint();
    }
  }
}

watch(
  () => [props.workspaceId, props.path, props.epoch, scope.value] as const,
  () => void load(),
  { immediate: true },
);

watch([period, sliceBy, () => props.branch], () => {
  rebuildModel();
  nextTick(() => paint());
});

function hoverDrawState(): TimelineHoverState {
  return {
    hit: hoverHit,
    intensity: hoverIntensity,
    outgoing: outgoingHit,
    outgoingIntensity,
    volumeCommitIndex,
  };
}

function stepHoverIntensity(step: number): boolean {
  let animating = false;
  if (hoverIntensity < hoverIntensityTarget) {
    hoverIntensity = Math.min(hoverIntensityTarget, hoverIntensity + step);
    if (hoverIntensity < hoverIntensityTarget) animating = true;
  } else if (hoverIntensity > hoverIntensityTarget) {
    hoverIntensity = Math.max(hoverIntensityTarget, hoverIntensity - step);
    if (hoverIntensity > hoverIntensityTarget) animating = true;
  }
  if (outgoingHit) {
    outgoingIntensity = Math.max(0, outgoingIntensity - step);
    if (outgoingIntensity > 0) animating = true;
    else outgoingHit = null;
  }
  return animating;
}

function requestPaint() {
  if (rafId) return;
  rafId = requestAnimationFrame((now) => {
    rafId = 0;
    const dt = lastFrameTime ? Math.min(48, now - lastFrameTime) : 16;
    lastFrameTime = now;
    const animating = stepHoverIntensity(dt / HOVER_ANIM_MS);
    paintFrame();
    if (animating) requestPaint();
    else lastFrameTime = 0;
  });
}

function paintFrame() {
  const host = canvasHost.value;
  const el = canvas.value;
  const vm = viewModel.value;
  if (!host || !el || !vm) return;
  const width = Math.max(1, host.clientWidth);
  const height = Math.max(180, host.clientHeight);
  const railWidth = pickRailWidth(sliceBy.value, vm.slices, width);
  layout.value = computeTimelineLayout(width, height, vm.slices.length, railWidth);
  const dpr = window.devicePixelRatio || 1;
  const tw = Math.round(width * dpr);
  const th = Math.round(height * dpr);
  if (el.width !== tw || el.height !== th) {
    el.width = tw;
    el.height = th;
  }
  el.style.width = `${width}px`;
  el.style.height = `${height}px`;
  const ctx = el.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  drawTimeline(ctx, vm, layout.value, readTimelineTheme(host), hoverDrawState(), scrollY.value);
  syncRail();
}

function paint() {
  requestPaint();
}

function setHover(hit: BubbleHit | null) {
  if (sameBubbleHit(hit, hoverHit)) return;
  if (hoverHit && hoverIntensity > 0.05) {
    outgoingHit = hoverHit;
    outgoingIntensity = hoverIntensity;
  }
  hoverHit = hit;
  hoverIntensity = 0;
  hoverIntensityTarget = hit ? 1 : 0;
  if (!lastFrameTime) lastFrameTime = performance.now();
  requestPaint();
}

function showTip(hit: BubbleHit | null, canvasWidth: number) {
  const vm = viewModel.value;
  if (!hit || !vm) {
    tip.value = null;
    return;
  }
  const commit = vm.commits[hit.commitIndex];
  const slice = vm.slices[hit.sliceIndex];
  tip.value = {
    x: Math.min(canvasWidth - 200, Math.max(layout.value.chartLeft, hit.x + 12)),
    y: Math.max(8, hit.y - 40),
    title: commit?.subject ?? "",
    meta: `${slice?.name ?? ""} · ${commit?.hash.slice(0, 7) ?? ""} · ${t("scm.inspect.treemapFiles", { n: commit?.weight ?? 1 })}`,
  };
}

function syncRail() {
  const el = rail.value;
  if (!el || !viewModel.value) return;
  const box = layout.value;
  el.style.height = `${box.swimlaneBottom - box.swimlaneTop}px`;
  el.style.top = `${box.swimlaneTop}px`;
  el.style.width = `${box.railWidth}px`;
}

function onMove(event: MouseEvent) {
  const el = canvas.value;
  const vm = viewModel.value;
  if (!el || !vm) {
    setHover(null);
    volumeCommitIndex = null;
    tip.value = null;
    return;
  }
  const rect = el.getBoundingClientRect();
  const x = event.clientX - rect.left;
  const y = event.clientY - rect.top;
  const bubbleHit = hitTestBubble(x, y, vm, layout.value, scrollY.value);
  const volumeHit = bubbleHit ? null : findNearestVolumeCommit(x, y, vm, layout.value);
  const nextVolume = volumeHit?.commitIndex ?? null;
  const volumeChanged = nextVolume !== volumeCommitIndex;
  volumeCommitIndex = nextVolume;
  const focused = bubbleHit ?? volumeHit;
  // volume scrub 时 y 取真实泳道中心，tooltip / 高亮对齐气泡。
  if (focused && volumeHit && !bubbleHit) {
    focused.y = sliceCenterYForTip(focused.sliceIndex);
  }
  setHover(focused);
  if (volumeChanged) requestPaint();
  showTip(focused, rect.width);
  if (bubbleHit) el.style.cursor = "pointer";
  else if (volumeHit) el.style.cursor = "zoom-in";
  else el.style.cursor = "default";
}

function sliceCenterYForTip(sliceIndex: number) {
  const box = layout.value;
  return box.swimlaneTop + sliceIndex * box.rowHeight + box.rowHeight / 2 - scrollY.value;
}

function onLeave() {
  setHover(null);
  volumeCommitIndex = null;
  tip.value = null;
  if (canvas.value) canvas.value.style.cursor = "default";
}

function onClick() {
  const hit = hoverHit;
  const vm = viewModel.value;
  if (!hit || !vm || !props.workspaceId) return;
  const commit = vm.commits[hit.commitIndex];
  if (!commit) return;
  emit("open", {
    workspaceId: props.workspaceId,
    path: props.path || ".",
    group: "commit",
    commit: commit.hash,
    parent: null,
  });
}

function onWheel(event: WheelEvent) {
  if (!viewModel.value) return;
  const box = layout.value;
  const maxScroll = Math.max(0, box.virtualSwimlaneHeight - (box.swimlaneBottom - box.swimlaneTop));
  if (maxScroll <= 0) return;
  event.preventDefault();
  scrollY.value = Math.min(maxScroll, Math.max(0, scrollY.value + event.deltaY));
  paint();
}

function setScope(next: VisualHistoryScope) {
  scope.value = next;
}

function pathAt(index: number) {
  return pathParts.value.slice(0, index + 1).join("/");
}

function sliceColor(index: number) {
  return SLICE_PALETTE[index % SLICE_PALETTE.length];
}

watch(canvasHost, (host) => {
  resizeObs?.disconnect();
  resizeObs = null;
  if (!host) return;
  resizeObs = new ResizeObserver(() => paint());
  resizeObs.observe(host);
});

onBeforeUnmount(() => {
  resizeObs?.disconnect();
  if (rafId) cancelAnimationFrame(rafId);
});
</script>

<template>
  <UiPane
    :title="paneTitle"
    :storage-key="storageKey"
    :size-key="sizeKey"
    :default-size="280"
    :default-expanded="false"
    :min-body="140"
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
    <div class="scm-visual-history">
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
                :class="{ current: index === pathParts.length - 1 }"
                :title="part"
                @click="index === pathParts.length - 1 ? emit('openFile', path) : undefined"
              >
                <FileTypeIcon :name="part" :directory="index < pathParts.length - 1" :size="12" />
                <span class="scm-inspect-crumb-label">{{ part }}</span>
              </button>
            </template>
          </template>
        </div>
        <div class="scm-timeline-tools">
          <div class="scm-timeline-slice" role="group" :aria-label="t('scm.inspect.sliceBy')">
            <UiButton
              variant="ghost"
              size="icon"
              :aria-pressed="sliceBy === 'author' ? 'true' : 'false'"
              :class="{ 'is-active': sliceBy === 'author' }"
              :tip="t('scm.inspect.sliceAuthor')"
              @click="sliceBy = 'author'"
            >
              <Codicon name="account" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon"
              :aria-pressed="sliceBy === 'branch' ? 'true' : 'false'"
              :class="{ 'is-active': sliceBy === 'branch' }"
              :tip="t('scm.inspect.sliceBranch')"
              @click="sliceBy = 'branch'"
            >
              <Codicon name="git-branch" />
            </UiButton>
          </div>
          <UiSelect
            v-model="period"
            class="scm-timeline-period"
            size="sm"
            :searchable="false"
            :options="periodOptions"
            :aria-label="t('scm.inspect.period')"
          />
        </div>
      </div>
      <p v-if="emptyText" class="tree-empty">{{ emptyText }}</p>
      <p v-else-if="loading && !points.length" class="tree-empty">{{ t("scm.inspect.loading") }}</p>
      <div v-else ref="canvasHost" class="scm-visual-canvas-host">
        <div ref="rail" class="scm-timeline-rail" :class="{ 'is-wide': showBranchLabels }" aria-hidden="true">
          <div class="scm-timeline-rail-inner" :style="{ transform: `translateY(${-scrollY}px)` }">
            <div
              v-for="(slice, index) in railSlices"
              :key="`${slice.name}-${index}`"
              class="scm-timeline-avatar"
              :style="{ height: `${layout.rowHeight}px` }"
              :title="slice.name"
            >
              <span class="scm-timeline-avatar-dot" :style="{ background: sliceColor(slice.colorIndex) }">
                {{ slice.initials }}
              </span>
              <span v-if="showBranchLabels" class="scm-timeline-avatar-name">{{ slice.name }}</span>
            </div>
          </div>
        </div>
        <canvas
          ref="canvas"
          class="scm-visual-canvas"
          @mousemove="onMove"
          @mouseleave="onLeave"
          @click="onClick"
          @wheel="onWheel"
        />
        <div v-if="tip" class="scm-visual-tip" :style="{ left: `${tip.x}px`, top: `${tip.y}px` }">
          <strong>{{ tip.title }}</strong>
          <span>{{ tip.meta }}</span>
        </div>
      </div>
    </div>
  </UiPane>
</template>
