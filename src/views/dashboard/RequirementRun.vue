<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Plus, Search } from "@lucide/vue";
import ChatPanel from "../../components/ChatPanel.vue";
import ContextMenu, { type ContextMenuItem } from "../../components/ui/ContextMenu.vue";
import SegmentedTabs from "../../components/ui/SegmentedTabs.vue";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import InboxToolRail from "../inbox/InboxToolRail.vue";
import {
  matchesHomeFilter,
  type HomeFilter,
  type HomeProject,
} from "./project-home";
import { RUN_RAIL_MAX, RUN_RAIL_MIN, readRunRail, writeRunRail } from "./requirement-home-mode";

const props = defineProps<{
  rows: HomeProject[];
  tabs: { id: string; label: string; count?: number; pinCount?: boolean }[];
  error: string;
  mode: string;
}>();

const emit = defineEmits<{
  "switch-mode": [value: string];
  create: [];
  open: [row: HomeProject];
  edit: [id: string, name: string, note: string];
  archive: [id: string, archived: boolean];
  remove: [id: string, name: string];
  "add-workspace": [id: string];
  layout: [payload: { content: boolean; files: boolean; width: number; terminal: boolean }];
  rail: [width: number];
}>();

const store = useAppStore();
const filter = ref<HomeFilter>("all");
const query = ref("");
const selectedId = ref("");
const pickedWorkspaceId = ref("");
const menu = ref<{ x: number; y: number; row: HomeProject } | null>(null);
const railWidth = ref(readRunRail());
const headEl = ref<HTMLElement | null>(null);
const createMeasure = ref<HTMLElement | null>(null);
const createCompact = ref(false);
let createLockWidth = 0;
let headObserver: ResizeObserver | null = null;

const modeTabs = computed(() => [
  { id: "code", label: t("dashboard.mode.code") },
  { id: "run", label: t("dashboard.mode.run") },
]);

const visible = computed(() =>
  props.rows.filter((row) => matchesHomeFilter(row, filter.value, query.value)),
);

const selected = computed(() => props.rows.find((row) => row.id === selectedId.value) ?? null);

const sessionId = computed(() => {
  const projectId = selected.value?.id;
  if (!projectId) return "";
  const taskIds = new Set((store.data?.tasks ?? []).map((task) => task.sessionId));
  const sessions = (store.data?.sessions ?? []).filter(
    (item) => item.projectId === projectId && !taskIds.has(item.id),
  );
  return sessions.find((item) => item.id === store.activeSessionId)?.id ?? sessions[0]?.id ?? "";
});

const toolWorkspaces = computed(() =>
  (store.data?.workspaces ?? []).filter((item) => {
    if (item.projectId !== selected.value?.id) return false;
    return !item.sessionId || item.sessionId === sessionId.value;
  }),
);

const activeWorkspace = computed(() =>
  toolWorkspaces.value.find((item) => item.id === pickedWorkspaceId.value) ?? toolWorkspaces.value[0] ?? null,
);

const menuItems = computed<ContextMenuItem[]>(() => {
  const row = menu.value?.row;
  if (!row) return [];
  const openable = row.workspaces.length > 0 && !store.isProjectPreparing(row.id);
  return [
    { id: "open", label: t("dashboard.menu.openWorkbench"), disabled: !openable },
    { type: "sep" },
    { id: "edit", label: t("dashboard.edit") },
    { id: "add", label: t("dashboard.menu.addProject") },
    { id: "archive", label: row.archived ? t("dashboard.unarchive") : t("dashboard.menu.archive") },
    { type: "sep" },
    { id: "delete", label: t("dashboard.delete") },
  ];
});

watch(visible, (list) => {
  if (list.some((row) => row.id === selectedId.value)) return;
  selectedId.value = list[0]?.id ?? "";
}, { immediate: true });

watch(selectedId, () => {
  pickedWorkspaceId.value = "";
});

function onRowContext(row: HomeProject, event: MouseEvent) {
  event.preventDefault();
  selectedId.value = row.id;
  menu.value = { x: event.clientX, y: event.clientY, row };
}

function onMenu(id: string) {
  const row = menu.value?.row;
  menu.value = null;
  if (!row) return;
  if (id === "open") emit("open", row);
  if (id === "edit") emit("edit", row.id, row.name, row.note);
  if (id === "add") emit("add-workspace", row.id);
  if (id === "archive") emit("archive", row.id, row.archived);
  if (id === "delete") emit("remove", row.id, row.name);
}

function fitCreate() {
  const head = headEl.value;
  const title = head?.querySelector("h1");
  const measure = createMeasure.value;
  if (!head || !(title instanceof HTMLElement) || !measure) return;
  const overflow = title.scrollWidth - title.clientWidth;
  if (!createCompact.value) {
    if (overflow > 1) {
      createCompact.value = true;
      createLockWidth = head.clientWidth;
    }
    return;
  }
  if (head.clientWidth <= createLockWidth) return;
  const button = head.querySelector(".req-run-create");
  const gap = button instanceof HTMLElement ? Number.parseFloat(getComputedStyle(button).columnGap) || 0 : 0;
  if (title.clientWidth - title.scrollWidth < measure.offsetWidth + gap) return;
  createCompact.value = false;
}

function beginRailResize(event: PointerEvent) {
  const handle = event.currentTarget;
  if (!(handle instanceof HTMLElement)) return;
  event.preventDefault();
  handle.setPointerCapture(event.pointerId);
  const startX = event.clientX;
  const startWidth = railWidth.value;
  document.body.classList.add("is-resizing-panels");
  const move = (moveEvent: PointerEvent) => {
    const next = startWidth + (moveEvent.clientX - startX);
    railWidth.value = Math.min(RUN_RAIL_MAX, Math.max(RUN_RAIL_MIN, next));
    emit("rail", railWidth.value);
  };
  const finish = () => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", finish);
    handle.removeEventListener("pointercancel", finish);
    if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId);
    document.body.classList.remove("is-resizing-panels");
    railWidth.value = writeRunRail(railWidth.value);
    emit("rail", railWidth.value);
    fitCreate();
  };
  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", finish);
  handle.addEventListener("pointercancel", finish);
}

onMounted(() => {
  emit("rail", railWidth.value);
  fitCreate();
  const head = headEl.value;
  if (!head) return;
  headObserver = new ResizeObserver(() => fitCreate());
  headObserver.observe(head);
});

onBeforeUnmount(() => {
  headObserver?.disconnect();
  headObserver = null;
});
</script>

<template>
  <aside class="inbox-rail req-run-rail">
    <header>
      <div ref="headEl" class="req-run-head">
        <h1>{{ t("dashboard.title") }}</h1>
        <UiButton
          class="req-run-create"
          variant="primary"
          :size="createCompact ? 'icon' : undefined"
          :tip="createCompact ? t('dashboard.create') : ''"
          @click="emit('create')"
        >
          <Plus :size="15" />
          <span v-if="!createCompact">{{ t("dashboard.create") }}</span>
        </UiButton>
        <span ref="createMeasure" class="req-run-create-measure" aria-hidden="true">{{ t("dashboard.create") }}</span>
      </div>
      <div v-if="error" class="error-card">{{ error }}</div>
      <div class="req-run-filters">
        <div class="req-mode-row">
          <span class="req-mode-label">{{ t("dashboard.modeLabel") }}</span>
          <SegmentedTabs
            :model-value="mode"
            :label="t('dashboard.modeLabel')"
            :items="modeTabs"
            @update:model-value="emit('switch-mode', $event)"
          />
        </div>
        <SegmentedTabs v-model="filter" :label="t('dashboard.filterLabel')" :items="tabs" />
        <label class="ds-search">
          <Search :size="14" />
          <UiInput v-model="query" type="search" :placeholder="t('dashboard.search')" />
        </label>
      </div>
    </header>
    <ul v-if="visible.length" class="inbox-list vs-scroll req-run-list" :aria-label="t('dashboard.run.listLabel')">
      <li v-for="row in visible" :key="row.id">
        <button
          type="button"
          :class="{ selected: row.id === selectedId }"
          @click="selectedId = row.id"
          @contextmenu="onRowContext(row, $event)"
        >
          <span>{{ row.name }}</span>
        </button>
      </li>
    </ul>
    <div v-else class="ds-empty inbox-rail-empty">
      <h3>{{ t("dashboard.noMatch") }}</h3>
      <p>{{ t("dashboard.noMatchHint") }}</p>
    </div>
    <div
      class="panel-resize-handle req-run-rail-resize"
      role="separator"
      :aria-label="t('app.resizeSidebar')"
      aria-orientation="vertical"
      :aria-valuemin="RUN_RAIL_MIN"
      :aria-valuemax="RUN_RAIL_MAX"
      :aria-valuenow="railWidth"
      tabindex="0"
      @pointerdown="beginRailResize"
    />
  </aside>

  <section class="inbox-detail chat-surface">
    <ChatPanel
      v-if="selected"
      :key="selected.id"
      class="inbox-thread"
      :project-id="selected.id"
      compact
      session-commands
    />
    <div v-else class="ds-empty req-run-empty">
      <h3>{{ t("dashboard.run.emptyTitle") }}</h3>
      <p>{{ t("dashboard.run.emptyHint") }}</p>
    </div>
  </section>

  <InboxToolRail
    v-if="selected"
    :key="selected.id"
    :workspaces="toolWorkspaces"
    :workspace-id="activeWorkspace?.id ?? ''"
    :session-id="sessionId"
    kind="project"
    :task-id="null"
    @pick-workspace="pickedWorkspaceId = $event"
    @layout="emit('layout', $event)"
  />

  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="menuItems"
    @select="onMenu"
    @close="menu = null"
  />
</template>
