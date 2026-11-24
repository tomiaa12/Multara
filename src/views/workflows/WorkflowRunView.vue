<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { isRunTerminal } from "../../domain/workflow";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import { useWorkflowsStore } from "../../stores/workflows";
import SegmentedTabs from "../../components/ui/SegmentedTabs.vue";
import FlowCanvas from "../../components/workflow/FlowCanvas.vue";
import AgentSessionPane from "../../components/workflow/run/AgentSessionPane.vue";
import NodeRunDrawer from "../../components/workflow/run/NodeRunDrawer.vue";
import RunHeader from "../../components/workflow/run/RunHeader.vue";
import InboxToolRail from "../inbox/InboxToolRail.vue";
import type { Workspace } from "../../domain/types";
import RunTimeline from "../../components/workflow/run/RunTimeline.vue";
import { buildRunOverlay } from "../../components/workflow/run/run-overlay";

const props = withDefaults(defineProps<{
  /** 收件箱右侧面板嵌入时传入；页面路由不传。 */
  runId?: string;
  embedded?: boolean;
}>(), {
  runId: "",
  embedded: false,
});

const route = useRoute();
const store = useWorkflowsStore();
const app = useAppStore();

const runId = computed(() => props.runId || String(route.params.runId ?? ""));
const detail = computed(() => store.runs[runId.value]);
const error = ref("");
const view = ref("canvas");
const selectedNodeId = ref<string | null>(null);
const selectedEdgeId = ref<string | null>(null);
const overviewOpen = ref(false);
const stageOpen = ref(false);
const stageWidth = ref(420);
const DRAWER_WIDTH_KEY = "multara.workflow.runDrawerWidth";
const OVERVIEW_WIDTH_KEY = "multara.workflow.runOverviewWidth";
const SIDE_MIN = 280;
const SIDE_MAX = 760;

function readSideWidth(key: string, fallback: number) {
  const value = Number(localStorage.getItem(key));
  return Number.isFinite(value) ? Math.min(SIDE_MAX, Math.max(SIDE_MIN, value)) : fallback;
}

const drawerWidth = ref(readSideWidth(DRAWER_WIDTH_KEY, 420));
const overviewWidth = ref(readSideWidth(OVERVIEW_WIDTH_KEY, 380));

function beginSideResize(which: "drawer" | "overview", event: PointerEvent) {
  const handle = event.currentTarget;
  if (!(handle instanceof HTMLElement) || event.button !== 0) return;
  event.preventDefault();
  handle.setPointerCapture(event.pointerId);
  const width = which === "drawer" ? drawerWidth : overviewWidth;
  const startX = event.clientX;
  const startWidth = width.value;
  document.body.classList.add("is-resizing-panels");
  const move = (moveEvent: PointerEvent) => {
    width.value = Math.min(SIDE_MAX, Math.max(SIDE_MIN, startWidth + startX - moveEvent.clientX));
  };
  const finish = () => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", finish);
    handle.removeEventListener("pointercancel", finish);
    if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId);
    document.body.classList.remove("is-resizing-panels");
    localStorage.setItem(which === "drawer" ? DRAWER_WIDTH_KEY : OVERVIEW_WIDTH_KEY, String(width.value));
  };
  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", finish);
  handle.addEventListener("pointercancel", finish);
}

const overlay = computed(() => (detail.value ? buildRunOverlay(detail.value) : undefined));
const selectedEdge = computed(() => {
  const id = selectedEdgeId.value;
  if (!id || !detail.value) return null;
  const edge = detail.value.definition.edges.find((item) => item.id === id);
  return edge ? { edge, info: overlay.value?.edges[id] } : null;
});

async function load() {
  if (!runId.value) return;
  error.value = "";
  try {
    await store.loadRun(runId.value);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function selectNode(id: string | null) {
  selectedNodeId.value = id;
  if (id) {
    selectedEdgeId.value = null;
    overviewOpen.value = false;
  }
}

const sidebar = computed<"node" | "overview" | null>(() => {
  if (selectedNodeId.value) return "node";
  if (!props.embedded && overviewOpen.value) return "overview";
  return null;
});

function toggleOverview() {
  if (sidebar.value === "overview") {
    overviewOpen.value = false;
    return;
  }
  selectedNodeId.value = null;
  overviewOpen.value = true;
}

const nodeSessionIds = computed(() =>
  (detail.value?.nodeRuns ?? []).map((item) => item.sessionId).filter((id): id is string => Boolean(id)),
);

const toolWorkspaces = computed<Workspace[]>(() => {
  const known = app.data?.workspaces ?? [];
  return (detail.value?.workspaces ?? []).map((slot) => {
    const found = known.find((item) => item.id === slot.workspaceId);
    if (found) return found;
    return {
      id: slot.workspaceId,
      projectId: slot.projectId,
      name: slot.slotKey,
      kind: "git",
      sourcePath: slot.path,
      workPath: slot.path,
      branch: slot.branch,
      isolationMode: "worktree",
      createdAt: "",
    };
  });
});

function onToolLayout(payload: { content: boolean; width: number }) {
  stageOpen.value = payload.content;
  stageWidth.value = payload.width;
  void nextTick(() => window.dispatchEvent(new Event("resize")));
}

function selectEdge(id: string | null) {
  selectedEdgeId.value = id;
  if (id) selectedNodeId.value = null;
}

let poll: ReturnType<typeof setInterval> | undefined;

function stopPoll() {
  if (!poll) return;
  clearInterval(poll);
  poll = undefined;
}

/** 事件没到时，卡片和总览会话会停在「开始运行」。进行中每秒对一次库。 */
async function refreshLive() {
  await load();
  const sessionId = store.runs[runId.value]?.run.overviewSessionId;
  if (sessionId) await app.reloadPersistedMessages(sessionId).catch(() => undefined);
  const status = store.runs[runId.value]?.run.status;
  if (!status || isRunTerminal(status)) stopPoll();
}

function watchLive() {
  if (poll) return;
  const status = store.runs[runId.value]?.run.status;
  if (status && isRunTerminal(status)) return;
  poll = setInterval(() => { void refreshLive(); }, 1000);
}

onMounted(() => {
  void store.subscribe();
  void refreshLive().then(watchLive);
});
onBeforeUnmount(() => {
  stopPoll();
  store.unsubscribe();
});
watch(runId, () => {
  selectedNodeId.value = null;
  selectedEdgeId.value = null;
  stopPoll();
  void refreshLive().then(watchLive);
});
</script>

<template>
  <div class="run-page" :class="{ 'is-embedded': embedded }">
    <div v-if="error" class="error-card run-error">{{ error }}</div>
    <div v-else-if="!detail" class="empty">{{ t("app.loading") }}</div>
    <template v-else>
      <RunHeader :detail="detail" :overview-open="sidebar === 'overview'" :hide-overview="embedded" @toggle-overview="toggleOverview">
        <template #actions><slot name="actions" /></template>
      </RunHeader>
      <div class="run-body" :class="{ 'has-stage': stageOpen }" :style="stageOpen ? { '--inbox-stage-width': `${stageWidth}px` } : undefined">
        <section class="run-main">
          <div class="run-toolbar">
            <SegmentedTabs
              v-model="view"
              :label="t('workflow.run.view')"
              :items="[{ id: 'canvas', label: t('workflow.run.viewCanvas') }, { id: 'timeline', label: t('workflow.run.viewTimeline') }]"
            />
            <span v-if="selectedEdge" class="edge-info">
              <code>{{ selectedEdge.edge.source }} → {{ selectedEdge.edge.target }}</code>
              <span class="ds-badge" :data-tone="selectedEdge.info?.state === 'live' ? 'success' : undefined">{{ t(`workflow.run.edge.${selectedEdge.info?.state ?? 'pending'}`) }}</span>
              <span v-if="selectedEdge.info?.tooltip" class="edge-value">{{ selectedEdge.info.tooltip }}</span>
            </span>
          </div>
          <div class="run-stage">
            <FlowCanvas
              v-show="view === 'canvas'"
              :definition="detail.definition"
              readonly
              :overlay="overlay"
              :selected-node-id="selectedNodeId"
              :selected-edge-id="selectedEdgeId"
              @select-node="selectNode"
              @select-edge="selectEdge"
              @pane-click="selectNode(null)"
            />
            <RunTimeline v-if="view === 'timeline'" :detail="detail" :selected-node-id="selectedNodeId" @select-node="selectNode" />
          </div>
        </section>
        <div v-if="sidebar" class="run-side" :style="{ width: `${drawerWidth}px` }">
          <div
            class="panel-resize-handle"
            role="separator"
            :aria-label="t('app.resizeSidebar')"
            aria-orientation="vertical"
            :aria-valuemin="SIDE_MIN"
            :aria-valuemax="SIDE_MAX"
            :aria-valuenow="drawerWidth"
            tabindex="0"
            @pointerdown="beginSideResize('drawer', $event)"
          />
          <NodeRunDrawer v-if="sidebar === 'node' && selectedNodeId" class="run-drawer" :detail="detail" :node-id="selectedNodeId" @close="selectNode(null)" />
          <aside v-else class="run-overview">
            <AgentSessionPane :session-id="detail.run.overviewSessionId" :sticky-user-turn="false" />
          </aside>
        </div>
        <div class="run-tool-host" :class="{ open: stageOpen }" :style="stageOpen ? { width: `${stageWidth}px` } : undefined">
          <InboxToolRail
            kind="workflow"
            log-label="工作流"
            :workspaces="toolWorkspaces"
            workspace-id=""
            :session-id="detail.run.overviewSessionId"
            :extra-session-ids="nodeSessionIds"
            :run-id="detail.run.id"
            :workflow-id="detail.run.workflowId"
            float-host-selector=".run-page"
            :task-id="null"
            @layout="onToolLayout"
          />
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.run-page {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.run-error {
  margin: var(--spacing-6);
}
.run-body {
  display: flex;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}
.run-tool-host {
  display: flex;
  flex: none;
  width: 0;
  min-width: 0;
  min-height: 0;
  height: 100%;
  overflow: hidden;
}
.run-tool-host.open {
  width: var(--inbox-stage-width, 420px);
}
.run-tool-host :deep(.inbox-stage) {
  position: relative;
  top: auto;
  right: auto;
  bottom: auto;
  flex: 1;
  width: 100%;
  height: 100%;
  box-shadow: none;
}
.run-main {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}
.run-toolbar {
  display: flex;
  align-items: center;
  gap: var(--spacing-3);
  padding: var(--spacing-2) var(--spacing-4);
  border-bottom: 1px solid var(--color-border-light);
}
.edge-info {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  min-width: 0;
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}
.edge-value {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-family-mono);
}
.run-stage {
  position: relative;
  flex: 1;
  min-height: 0;
}
.run-stage > :deep(*) {
  height: 100%;
}
.run-side {
  display: flex;
  flex: none;
  min-width: 0;
  min-height: 0;
}
.run-side.outcome-side { overflow: hidden; }
.run-outcome { display: flex; flex: 1; min-height: 0; padding: var(--spacing-3); }
.run-drawer,
.run-overview {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  height: 100%;
}
</style>
