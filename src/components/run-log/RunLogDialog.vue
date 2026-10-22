<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  ChevronRight,
  Copy,
  FilePen,
  Filter,
  Minus,
  MoreHorizontal,
  Plus,
  Search,
  Terminal,
  X,
} from "@lucide/vue";
import type { OpenRunLogOptions } from "./run-log-state";
import {
  buildLanes,
  buildRunLogSteps,
  buildRunOutcome,
  formatElapsedMs,
  formatLaneTotal,
  formatOffset,
  formatRunTime,
  formatStepDuration,
  formatTick,
  groupRunLogSteps,
  isGroupRow,
  laneSegmentStyle,
  resolveRunDurationMs,
  runStatusLabel,
  shouldShowTimeline,
  stepDetailText,
  stepFilterKey,
  stepHaystack,
  stepKindClass,
  stepSummary,
  stepTitle,
  timelineTicks,
  type LaneSegment,
  type RunLogFilterKey,
  type RunLogGroup,
  type RunLogRow,
  type RunLogStep,
} from "./build-run-log";
import { t } from "../../i18n";

const props = defineProps<{
  payload: OpenRunLogOptions;
}>();

const emit = defineEmits<{
  close: [];
}>();

/** 时间轴缩放档位：1 / 2 / 4 / 8。 */
const ZOOM_STEPS = [1, 2, 4, 8] as const;

const query = ref("");
const expandedGroups = ref<Set<number>>(new Set());
const selectedSeq = ref<number | null>(null);
const filterKeys = ref<RunLogFilterKey[]>([]);
const sortNewest = ref(false);
const filterOpen = ref(false);
const moreOpen = ref(false);
const copied = ref(false);
const zoom = ref<(typeof ZOOM_STEPS)[number]>(1);
const laneScroller = ref<HTMLElement | null>(null);
const nowMs = ref(Date.now());
let tickTimer: ReturnType<typeof setInterval> | null = null;

const isLive = computed(() => props.payload.status === "queued" || props.payload.status === "running");
const steps = computed(() => buildRunLogSteps(props.payload.timeline));
const outcome = computed(() => buildRunOutcome(steps.value));
const lanes = computed(() =>
  buildLanes(steps.value, props.payload.createdAt, props.payload.endedAt ?? undefined),
);
const showTimeline = computed(() => shouldShowTimeline(steps.value, lanes.value));
const durationMs = computed(() =>
  resolveRunDurationMs({
    elapsedMs: props.payload.elapsedMs,
    createdAt: props.payload.createdAt,
    endedAt: props.payload.endedAt,
    steps: steps.value,
    live: isLive.value,
    nowMs: nowMs.value,
  }),
);
const runStartMs = computed(() => {
  const ms = new Date(props.payload.createdAt).getTime();
  return Number.isFinite(ms) ? ms : undefined;
});

const filterOptions = computed(() => {
  const seen = new Map<RunLogFilterKey, string>();
  for (const step of steps.value) {
    const key = stepFilterKey(step);
    if (!seen.has(key)) seen.set(key, stepTitle(step));
  }
  return [...seen.entries()];
});

const filteredSteps = computed(() => {
  const needle = query.value.trim().toLowerCase();
  const active = new Set(filterKeys.value);
  return steps.value.filter((step) => {
    if (active.size > 0 && !active.has(stepFilterKey(step))) return false;
    if (needle && !stepHaystack(step).includes(needle)) return false;
    return true;
  });
});

const rows = computed(() => {
  const grouped = groupRunLogSteps(filteredSteps.value);
  return sortNewest.value ? [...grouped].reverse() : grouped;
});

const ticks = computed(() => (lanes.value ? timelineTicks(lanes.value.totalMs, 6 * zoom.value) : []));
const zoomIndex = computed(() => ZOOM_STEPS.indexOf(zoom.value));
const canZoomIn = computed(() => zoomIndex.value >= 0 && zoomIndex.value < ZOOM_STEPS.length - 1);
const canZoomOut = computed(() => zoomIndex.value > 0);

const selectedStep = computed(() => {
  if (selectedSeq.value == null) return null;
  return steps.value.find((step) => step.seq === selectedSeq.value) ?? null;
});

const selectedOffsetMs = computed(() => {
  const step = selectedStep.value;
  if (!step?.startedAt || runStartMs.value === undefined) return undefined;
  const at = new Date(step.startedAt).getTime();
  if (!Number.isFinite(at)) return undefined;
  return Math.max(0, at - runStartMs.value);
});

watch(
  () => props.payload.runId,
  () => {
    query.value = "";
    expandedGroups.value = new Set();
    selectedSeq.value = null;
    filterKeys.value = [];
    sortNewest.value = false;
    filterOpen.value = false;
    moreOpen.value = false;
    zoom.value = 1;
  },
);

function startLiveTick() {
  if (tickTimer) return;
  tickTimer = setInterval(() => {
    nowMs.value = Date.now();
  }, 1000);
}

function stopLiveTick() {
  if (!tickTimer) return;
  clearInterval(tickTimer);
  tickTimer = null;
}

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (filterOpen.value || moreOpen.value) {
      filterOpen.value = false;
      moreOpen.value = false;
      return;
    }
    if (selectedSeq.value != null) {
      selectedSeq.value = null;
      return;
    }
    emit("close");
  }
}

function onDocPointer(event: PointerEvent) {
  const target = event.target as HTMLElement | null;
  if (!target?.closest(".run-log-menu, .run-log-menu-btn")) {
    filterOpen.value = false;
    moreOpen.value = false;
  }
}

onMounted(() => {
  document.addEventListener("keydown", onKey);
  document.addEventListener("pointerdown", onDocPointer);
  if (isLive.value) startLiveTick();
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKey);
  document.removeEventListener("pointerdown", onDocPointer);
  stopLiveTick();
});

watch(isLive, (live) => {
  if (live) startLiveTick();
  else stopLiveTick();
});

function offsetLabel(startedAt?: string): string {
  if (runStartMs.value === undefined || !startedAt) return "";
  const at = new Date(startedAt).getTime();
  if (!Number.isFinite(at)) return "";
  return formatOffset(Math.max(0, at - runStartMs.value));
}

function selectStep(seq: number) {
  selectedSeq.value = selectedSeq.value === seq ? null : seq;
}

function toggleGroup(seq: number) {
  const next = new Set(expandedGroups.value);
  if (next.has(seq)) next.delete(seq);
  else next.add(seq);
  expandedGroups.value = next;
}

function toggleFilter(key: RunLogFilterKey) {
  const next = filterKeys.value.includes(key)
    ? filterKeys.value.filter((item) => item !== key)
    : [...filterKeys.value, key];
  filterKeys.value = next;
}

function seek(offsetMs: number) {
  if (runStartMs.value === undefined) return;
  const target = runStartMs.value + offsetMs;
  const hit = steps.value.find((step) => {
    if (!step.startedAt) return false;
    const at = new Date(step.startedAt).getTime();
    return Number.isFinite(at) && at >= target;
  });
  if (!hit) return;
  selectedSeq.value = hit.seq;
  document.getElementById(`run-log-step-${hit.seq}`)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
}

function setZoom(next: (typeof ZOOM_STEPS)[number]) {
  const el = laneScroller.value;
  const previous = zoom.value;
  zoom.value = next;
  if (!el || previous === next) return;
  // 缩放后保持视口中心对应的时间点，避免时间轴跳走。
  const centerRatio = (el.scrollLeft + el.clientWidth / 2) / (el.clientWidth * previous);
  requestAnimationFrame(() => {
    el.scrollLeft = centerRatio * el.clientWidth * next - el.clientWidth / 2;
  });
}

function segmentClass(kind: LaneSegment["kind"]): string {
  return `is-${kind}`;
}

function rowClass(row: RunLogRow): string {
  if (isGroupRow(row)) return "is-group";
  return `is-${stepKindClass(row)}`;
}

function groupContains(group: RunLogGroup, seq: number | null): boolean {
  return seq != null && group.steps.some((step) => step.seq === seq);
}

async function copyTranscript() {
  const text = filteredSteps.value
    .map((step) => {
      const title = stepTitle(step);
      const summary = stepSummary(step);
      const detail = stepDetailText(step);
      return [title, summary, detail].filter(Boolean).join("\n");
    })
    .join("\n\n");
  try {
    await navigator.clipboard.writeText(text);
    copied.value = true;
    window.setTimeout(() => {
      copied.value = false;
    }, 1200);
  } catch {
    copied.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop run-log-backdrop" @click.self="emit('close')">
    <div
      class="run-log-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="run-log-title"
    >
      <header class="run-log-header">
        <div class="run-log-header-main">
          <h2 id="run-log-title" class="run-log-sr">{{ payload.title || t("runLog.title") }}</h2>
          <p class="run-log-facts">
            <span class="run-log-status" :data-status="payload.status">{{ runStatusLabel(payload.status) }}</span>
            <template v-if="payload.agentName">
              <span class="run-log-agent">{{ payload.agentName }}</span>
            </template>
            <span aria-hidden="true">·</span>
            <span>{{ t("runLog.elapsed", { time: formatElapsedMs(durationMs) }) }}</span>
            <template v-if="payload.createdAt">
              <span aria-hidden="true">·</span>
              <span>{{ formatRunTime(payload.createdAt) }}</span>
            </template>
          </p>
          <p v-if="payload.error" class="run-log-error">{{ payload.error }}</p>
        </div>
        <button type="button" class="run-log-close" :title="t('common.close')" :aria-label="t('common.close')" @click="emit('close')">
          <X :size="16" />
        </button>
      </header>

      <div v-if="outcome" class="run-log-outcome">
        <span class="run-log-outcome-label">{{ t("runLog.outcome") }}</span>
        <span v-if="outcome.paths.length" class="run-log-chip" :title="outcome.paths.join('\n')">
          <FilePen :size="12" />
          {{ t("runLog.filesCount", { n: outcome.paths.length }) }}
        </span>
        <span v-if="outcome.commandCount" class="run-log-chip">
          <Terminal :size="12" />
          {{ t("runLog.commandsCount", { n: outcome.commandCount }) }}
        </span>
      </div>

      <section v-if="showTimeline && lanes" class="run-log-timeline" :aria-label="t('runLog.timelineAria')">
        <div class="run-log-lane-labels">
          <div class="run-log-lane-spacer" />
          <div class="run-log-lane-label">
            <span>{{ t("runLog.laneModel") }}</span>
            <span>{{ formatLaneTotal(lanes.modelMs) }}</span>
          </div>
          <div class="run-log-lane-label">
            <span>{{ t("runLog.laneTool") }}</span>
            <span>{{ formatLaneTotal(lanes.toolMs) }}</span>
          </div>
        </div>
        <div ref="laneScroller" class="run-log-lane-tracks">
          <div class="run-log-lane-canvas" :style="{ width: `${zoom * 100}%` }">
            <div
              v-if="selectedOffsetMs !== undefined"
              class="run-log-playhead"
              :style="{ left: `${(Math.min(selectedOffsetMs, lanes.totalMs) / lanes.totalMs) * 100}%` }"
            />
            <div class="run-log-ticks">
              <span
                v-for="(tick, index) in ticks"
                :key="tick"
                class="run-log-tick"
                :class="{ 'is-last': index === ticks.length - 1 && index !== 0 }"
                :style="{ left: `${(tick / lanes.totalMs) * 100}%` }"
              >{{ formatTick(tick) }}</span>
            </div>
            <div class="run-log-track">
              <button
                v-for="(segment, index) in lanes.model"
                :key="`model-${index}`"
                type="button"
                class="run-log-seg"
                :class="segmentClass(segment.kind)"
                :style="laneSegmentStyle(segment, lanes.totalMs)"
                :title="t('runLog.laneTitleModel', { time: formatLaneTotal(segment.durationMs) })"
                @click="seek(segment.startMs)"
              />
            </div>
            <div class="run-log-track">
              <button
                v-for="(segment, index) in lanes.tool"
                :key="`tool-${index}`"
                type="button"
                class="run-log-seg is-tool"
                :style="laneSegmentStyle(segment, lanes.totalMs)"
                :title="t('runLog.laneTitleTool', { time: formatLaneTotal(segment.durationMs) })"
                @click="seek(segment.startMs)"
              />
            </div>
          </div>
        </div>
        <div class="run-log-zoom" :class="{ 'is-on': zoom > 1 }">
          <button type="button" :title="t('runLog.zoomOut')" :disabled="!canZoomOut" @click="setZoom(ZOOM_STEPS[zoomIndex - 1] ?? 1)">
            <Minus :size="12" />
          </button>
          <span>{{ zoom }}×</span>
          <button type="button" :title="t('runLog.zoomIn')" :disabled="!canZoomIn" @click="setZoom(ZOOM_STEPS[zoomIndex + 1] ?? zoom)">
            <Plus :size="12" />
          </button>
        </div>
      </section>

      <div class="run-log-toolbar">
        <label class="run-log-search">
          <Search :size="13" aria-hidden="true" />
          <input
            v-model="query"
            type="search"
            :placeholder="t('runLog.searchPlaceholder')"
            :aria-label="t('runLog.searchAria')"
          />
        </label>
        <span class="run-log-count">
          <template v-if="query.trim() || filterKeys.length">{{ t("runLog.stepsFiltered", { filtered: filteredSteps.length, total: steps.length }) }}</template>
          <template v-else>{{ t("runLog.stepsCount", { n: steps.length }) }}</template>
        </span>
        <div v-if="filterOptions.length" class="run-log-menu-wrap">
          <button
            type="button"
            class="run-log-menu-btn"
            :class="{ 'is-on': filterKeys.length }"
            :title="t('common.filter')"
            @click="filterOpen = !filterOpen; moreOpen = false"
          >
            <Filter :size="13" />
            {{ t("common.filter") }}
            <span v-if="filterKeys.length">{{ filterKeys.length }}</span>
          </button>
          <div v-if="filterOpen" class="run-log-menu" role="menu">
            <button
              v-for="[key, label] in filterOptions"
              :key="key"
              type="button"
              role="menuitemcheckbox"
              :aria-checked="filterKeys.includes(key)"
              @click="toggleFilter(key)"
            >
              <span class="run-log-check" :class="{ 'is-on': filterKeys.includes(key) }" />
              {{ label }}
            </button>
            <button v-if="filterKeys.length" type="button" class="is-muted" @click="filterKeys = []">{{ t("runLog.clearFilters") }}</button>
          </div>
        </div>
        <div class="run-log-menu-wrap">
          <button
            type="button"
            class="run-log-menu-btn is-icon"
            :title="t('scm.moreActions')"
            @click="moreOpen = !moreOpen; filterOpen = false"
          >
            <MoreHorizontal :size="14" />
          </button>
          <div v-if="moreOpen" class="run-log-menu" role="menu">
            <button type="button" @click="sortNewest = !sortNewest; moreOpen = false">
              {{ sortNewest ? t("runLog.sortOldest") : t("runLog.sortNewest") }}
            </button>
            <button type="button" @click="copyTranscript(); moreOpen = false">
              <Copy :size="13" />
              {{ copied ? t("runLog.copied") : t("runLog.copyRun") }}
            </button>
          </div>
        </div>
      </div>

      <div class="run-log-body">
        <div class="run-log-steps vs-scroll">
          <p v-if="!steps.length" class="run-log-empty">{{ t("runLog.emptySteps") }}</p>
          <p v-else-if="!rows.length" class="run-log-empty">{{ t("runLog.noMatchSteps") }}</p>
          <ul v-else class="run-log-step-list">
            <li
              v-for="row in rows"
              :id="`run-log-step-${row.seq}`"
              :key="row.seq"
              class="run-log-step"
              :class="[rowClass(row), { 'is-open': isGroupRow(row) ? expandedGroups.has(row.seq) : selectedSeq === row.seq }]"
            >
              <template v-if="isGroupRow(row)">
                <button type="button" class="run-log-step-head" @click="toggleGroup(row.seq)">
                  <span class="run-log-offset">{{ offsetLabel(row.startedAt) }}</span>
                  <ChevronRight class="run-log-chevron" :size="12" />
                  <span class="run-log-step-kind">{{ row.tool }}</span>
                  <span class="run-log-step-summary">{{ stepSummary(row.steps[0]!) }}</span>
                  <span class="run-log-group-count">{{ t("runLog.groupCount", { n: row.steps.length }) }}</span>
                  <span class="run-log-step-meta">
                    <span v-if="row.durationMs != null">{{ formatStepDuration(row.durationMs) }}</span>
                  </span>
                </button>
                <ul v-if="expandedGroups.has(row.seq)" class="run-log-group-list">
                  <li v-for="step in row.steps" :key="step.seq" :class="{ 'is-selected': selectedSeq === step.seq || groupContains(row, selectedSeq) && selectedSeq === step.seq }">
                    <button type="button" class="run-log-step-head" @click="selectStep(step.seq)">
                      <span class="run-log-offset">{{ offsetLabel(step.startedAt) }}</span>
                      <span class="run-log-step-kind">{{ stepTitle(step) }}</span>
                      <span class="run-log-step-summary">{{ stepSummary(step) }}</span>
                      <span class="run-log-step-meta">
                        <span v-if="step.durationMs != null">{{ formatStepDuration(step.durationMs) }}</span>
                      </span>
                    </button>
                  </li>
                </ul>
              </template>
              <button
                v-else-if="row.kind !== 'text'"
                type="button"
                class="run-log-step-head"
                @click="selectStep(row.seq)"
              >
                <span class="run-log-offset">{{ offsetLabel(row.startedAt) }}</span>
                <span class="run-log-rail" />
                <span class="run-log-step-kind">{{ stepTitle(row) }}</span>
                <span class="run-log-step-summary">{{ stepSummary(row) }}</span>
                <span class="run-log-step-meta">
                  <span v-if="row.kind === 'call' && row.durationMs != null">{{ formatStepDuration(row.durationMs) }}</span>
                  <span v-else-if="row.kind === 'call' && !row.result && isLive">{{ t("runLog.inProgress") }}</span>
                </span>
              </button>
              <div v-else class="run-log-prose">
                <span class="run-log-offset">{{ offsetLabel(row.startedAt) }}</span>
                <span class="run-log-rail is-report" />
                <p>{{ row.item.content }}</p>
              </div>
            </li>
          </ul>
        </div>

        <aside v-if="selectedStep" class="run-log-inspector">
          <header>
            <strong>{{ stepTitle(selectedStep) }}</strong>
            <span>{{ offsetLabel(selectedStep.startedAt) }}</span>
            <button type="button" :title="t('runLog.closeDetail')" :aria-label="t('runLog.closeDetail')" @click="selectedSeq = null">
              <X :size="14" />
            </button>
          </header>
          <pre class="vs-scroll">{{ stepDetailText(selectedStep) || t("runLog.noDetail") }}</pre>
        </aside>
      </div>
    </div>
  </div>
</template>
