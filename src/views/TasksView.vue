<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import {
  Ban,
  Circle,
  CircleCheck,
  CircleDashed,
  CircleX,
  Clock3,
  Eye,
  MoreHorizontal,
  Plus,
  Rows3,
  Users,
} from "@lucide/vue";
import type { Task, TaskStatus } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import ContextMenu, { type ContextMenuItem } from "../components/ui/ContextMenu.vue";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import { loadRequirementCategories } from "./dashboard/requirement-categories";
import { moveTask, orderColumn, relativeTime, TASK_COLUMNS, taskCode } from "./task-board";
import { readTaskMeta } from "./task-meta";
import TaskCreateDialog from "./TaskCreateDialog.vue";

const ORDER_KEY = "multara.task-board-order";
const DISPLAY_KEY = "multara.task-board-display";
const store = useAppStore();
const router = useRouter();
const scope = ref<"all" | "agent" | "squad">("all");
const agentFilter = ref("");
const squadFilter = ref("");
const projectFilters = ref<string[]>([]);
const tagFilters = ref<string[]>([]);
const showDone = ref(loadDisplay().showDone);
const showCancelled = ref(loadDisplay().showCancelled);
const displayOpen = ref(false);
const createOpen = ref(false);
const createStatus = ref<TaskStatus>("todo");
const categoryTick = ref(0);
const categories = computed(() => {
  void categoryTick.value;
  return loadRequirementCategories(localStorage).categories;
});
const error = ref("");

watch(createOpen, () => {
  categoryTick.value += 1;
});
const dragId = ref("");
const dropStatus = ref<TaskStatus | "">("");
const dropBeforeId = ref("");
let draggingTaskId = "";
let dragClearTimer = 0;
const cardMenu = ref<{ x: number; y: number; taskId: string } | null>(null);
const columnMenu = ref<{ x: number; y: number; status: TaskStatus } | null>(null);
const order = ref<Record<string, string[]>>(loadOrder());
let skipClick = false;

const icons = {
  backlog: CircleDashed,
  todo: Circle,
  in_progress: Clock3,
  in_review: Eye,
  blocked: Ban,
  done: CircleCheck,
  cancelled: CircleX,
};

const tasks = computed(() => store.data?.tasks ?? []);
const agents = computed(() => store.data?.agents ?? []);
const squads = computed(() => store.data?.squads ?? []);
const projects = computed(() => store.data?.projects ?? []);
const projectFilterOptions = computed(() =>
  projects.value.map((project) => ({ value: project.id, label: project.name })),
);
const tagFilterOptions = computed(() =>
  categories.value.map((tag) => ({ value: tag.id, label: tag.name })),
);
/**
 * 任务本身只记执行的智能体；是否属于某个小队，看它所在会话的目标。
 * 「小队」范围下未选具体小队时，展示所有交给小队的任务。
 */
const filtered = computed(() =>
  tasks.value.filter((task) => {
    const session = store.data?.sessions.find((item) => item.id === task.sessionId);
    if (scope.value === "agent" && agentFilter.value && task.agentId !== agentFilter.value) return false;
    if (scope.value === "squad") {
      if (session?.targetType !== "squad") return false;
      if (squadFilter.value && session.targetId !== squadFilter.value) return false;
    }
    if (projectFilters.value.length) {
      const projectId = session?.projectId ?? "";
      if (!projectFilters.value.includes(projectId)) return false;
    }
    if (tagFilters.value.length && !taskMatchesTags(task, tagFilters.value)) return false;
    return true;
  }),
);

function taskLabelIds(task: Task): string[] {
  const known = new Map(categories.value.map((item) => [item.id, item.id]));
  const byName = new Map(categories.value.map((item) => [item.name, item.id]));
  const raw = [...(task.labels ?? []), ...readTaskMeta(task.id).labels];
  const ids: string[] = [];
  for (const item of raw) {
    const id = known.get(item) ?? byName.get(item);
    if (id && !ids.includes(id)) ids.push(id);
  }
  return ids;
}

function taskMatchesTags(task: Task, tagIds: string[]) {
  const labels = taskLabelIds(task);
  return tagIds.some((id) => labels.includes(id));
}

function setScope(next: "all" | "agent" | "squad") {
  scope.value = next;
  agentFilter.value = "";
  squadFilter.value = "";
}
const columns = computed(() =>
  TASK_COLUMNS.filter((column) => {
    if (column.id === "done") return showDone.value;
    if (column.id === "cancelled") return showCancelled.value;
    return true;
  }),
);
const workingCount = computed(() => new Set(
  tasks.value.filter((task) => task.status === "in_progress").map((task) => task.agentId),
).size);
const menuTask = computed(() => tasks.value.find((task) => task.id === cardMenu.value?.taskId));
const cardMenuItems = computed<ContextMenuItem[]>(() => [
  { id: "open", label: t("tasks.open") },
  { id: "copy", label: t("tasks.copyTitle") },
  { type: "sep" },
  ...TASK_COLUMNS.map((column) => ({
    id: `status:${column.id}`,
    label: column.label,
    checked: menuTask.value?.status === column.id,
  })),
  { type: "sep" },
  { id: "delete", label: t("tasks.delete") },
]);
const columnMenuItems: ContextMenuItem[] = [{ id: "add", label: t("tasks.addTask") }];

function loadOrder() {
  try {
    const raw = localStorage.getItem(ORDER_KEY);
    const parsed = raw ? JSON.parse(raw) as Record<string, string[]> : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function loadDisplay() {
  try {
    const raw = localStorage.getItem(DISPLAY_KEY);
    const parsed = raw ? JSON.parse(raw) as { showDone?: boolean; showCancelled?: boolean } : {};
    return {
      // 已完成默认展示；仅当用户显式关掉时隐藏。已取消仍默认隐藏。
      showDone: parsed.showDone !== false,
      showCancelled: parsed.showCancelled === true,
    };
  } catch {
    return { showDone: true, showCancelled: false };
  }
}

function persistDisplay() {
  localStorage.setItem(DISPLAY_KEY, JSON.stringify({
    showDone: showDone.value,
    showCancelled: showCancelled.value,
  }));
}

function persist(status: TaskStatus, ids: string[]) {
  order.value = { ...order.value, [status]: ids };
  localStorage.setItem(ORDER_KEY, JSON.stringify(order.value));
}

function columnTasks(status: TaskStatus) {
  return orderColumn(filtered.value.filter((task) => task.status === status), order.value[status] ?? []);
}

function agentFor(agentId: string) {
  return agents.value.find((agent) => agent.id === agentId);
}

function openCreate(status: TaskStatus = "todo") {
  createStatus.value = status;
  createOpen.value = true;
  columnMenu.value = null;
}

function openTask(task: Task) {
  if (skipClick) return;
  void router.push(`/tasks/${task.id}`);
}

function clearDragState() {
  window.clearTimeout(dragClearTimer);
  dragClearTimer = 0;
  draggingTaskId = "";
  dragId.value = "";
  dropStatus.value = "";
  dropBeforeId.value = "";
}

function onDragStart(event: DragEvent, task: Task) {
  window.clearTimeout(dragClearTimer);
  dragClearTimer = 0;
  draggingTaskId = task.id;
  dragId.value = task.id;
  dropStatus.value = "";
  dropBeforeId.value = "";
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", task.id);
  }
}

function allowDrop(event: DragEvent) {
  if (!draggingTaskId) return false;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  return true;
}

function onCardOver(event: DragEvent, status: TaskStatus, taskId: string) {
  if (!allowDrop(event)) return;
  dropStatus.value = status;
  if (taskId === draggingTaskId) return;
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  const after = event.clientY > rect.top + rect.height / 2;
  if (!after) {
    dropBeforeId.value = taskId;
    return;
  }
  const ids = columnTasks(status).map((item) => item.id);
  const index = ids.indexOf(taskId);
  dropBeforeId.value = ids[index + 1] ?? "";
}

function onColumnOver(event: DragEvent, status: TaskStatus) {
  if (!allowDrop(event)) return;
  dropStatus.value = status;
  const card = (event.target as HTMLElement | null)?.closest("article.task-card");
  if (!card) dropBeforeId.value = "";
}

function onDragEnd() {
  skipClick = true;
  // macOS/WKWebView 可能在 drop 之后才清状态；保留短暂宽限避免丢单。
  window.clearTimeout(dragClearTimer);
  dragClearTimer = window.setTimeout(() => {
    clearDragState();
    skipClick = false;
  }, 50);
}

async function finishDrop(event: DragEvent, status: TaskStatus) {
  event.preventDefault();
  const taskId = draggingTaskId || event.dataTransfer?.getData("text/plain") || "";
  const beforeId = dropStatus.value === status ? dropBeforeId.value : "";
  clearDragState();
  skipClick = true;
  window.setTimeout(() => {
    skipClick = false;
  }, 0);
  const task = tasks.value.find((item) => item.id === taskId);
  if (!task || !taskId) return;
  const source = task.status;
  try {
    if (source !== status) await store.updateTaskStatus(taskId, status);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    return;
  }
  const destination = columnTasks(status).map((item) => item.id);
  persist(status, moveTask(destination, taskId, beforeId === taskId ? "" : beforeId));
  if (source !== status) persist(source, (order.value[source] ?? []).filter((id) => id !== taskId));
}

function openCardMenu(event: MouseEvent, task: Task) {
  displayOpen.value = false;
  columnMenu.value = null;
  cardMenu.value = { x: event.clientX, y: event.clientY, taskId: task.id };
}

function openColumnMenu(event: MouseEvent, status: TaskStatus) {
  cardMenu.value = null;
  columnMenu.value = { x: event.clientX, y: event.clientY, status };
}

async function onCardMenu(id: string) {
  const task = menuTask.value;
  cardMenu.value = null;
  if (!task) return;
  if (id === "open") {
    void router.push(`/tasks/${task.id}`);
    return;
  }
  if (id === "copy") {
    await navigator.clipboard.writeText(task.title);
    return;
  }
  if (id === "delete") {
    try {
      await store.confirmDeleteTask(task.id);
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause);
    }
    return;
  }
  if (id.startsWith("status:")) {
    const status = id.slice("status:".length) as TaskStatus;
    if (status === task.status) return;
    try {
      await store.updateTaskStatus(task.id, status);
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause);
    }
  }
}

function onColumnMenu(id: string) {
  const status = columnMenu.value?.status ?? "todo";
  columnMenu.value = null;
  if (id === "add") openCreate(status);
}
</script>

<template>
  <div class="tasks-page">
    <header class="task-toolbar">
      <div class="task-toolbar-title">
        <h1>{{ t("tasks.title") }}</h1>
        <nav class="ds-segmented" :aria-label="t('tasks.scope')">
          <button type="button" :class="{ active: scope === 'all' }" @click="setScope('all')">{{ t("tasks.scope.all") }}</button>
          <button type="button" :class="{ active: scope === 'agent' }" @click="setScope('agent')">{{ t("tasks.scope.agent") }}</button>
          <button type="button" :class="{ active: scope === 'squad' }" @click="setScope('squad')">{{ t("tasks.scope.squad") }}</button>
        </nav>
        <div class="task-toolbar-filters">
          <UiSelect
            v-model="projectFilters"
            multiple
            clearable
            size="sm"
            :placeholder="t('tasks.filterProject')"
            :aria-label="t('tasks.filterProject')"
            :options="projectFilterOptions"
            :empty-text="t('tasks.noProjects')"
          />
          <UiSelect
            v-model="tagFilters"
            multiple
            clearable
            size="sm"
            :placeholder="t('tasks.filterTags')"
            :aria-label="t('tasks.filterTags')"
            :options="tagFilterOptions"
            :empty-text="t('tasks.noTags')"
          />
          <div class="task-toolbar-display">
            <button type="button" :class="{ active: displayOpen }" @click="displayOpen = !displayOpen">
              <Rows3 :size="14" />{{ t("tasks.display") }}
            </button>
            <div v-if="displayOpen" class="task-popover task-popover-start">
              <label><input v-model="showDone" type="checkbox" @change="persistDisplay" />{{ t("tasks.showDone") }}</label>
              <label><input v-model="showCancelled" type="checkbox" @change="persistDisplay" />{{ t("tasks.showCancelled") }}</label>
            </div>
          </div>
        </div>
      </div>
      <div class="task-toolbar-actions">
        <span class="task-working">
          <i class="ds-dot" :data-tone="workingCount ? 'success' : undefined" />
          {{ t("tasks.working", { n: workingCount }) }}
        </span>
        <UiButton variant="primary" size="small" @click="openCreate()"><Plus :size="14" /> {{ t("tasks.create") }}</UiButton>
      </div>
    </header>
    <div v-if="scope === 'agent'" class="task-agent-scopes">
      <button type="button" :class="{ active: !agentFilter }" @click="agentFilter = ''">{{ t("tasks.allAgents") }}</button>
      <button
        v-for="agent in agents"
        :key="agent.id"
        type="button"
        :class="{ active: agentFilter === agent.id }"
        @click="agentFilter = agent.id"
      >
        <AgentAvatar :value="agent.avatar" :name="agent.name" :size="16" />
        {{ agent.name }}
      </button>
    </div>
    <div v-if="scope === 'squad'" class="task-agent-scopes">
      <button type="button" :class="{ active: !squadFilter }" @click="squadFilter = ''">{{ t("tasks.allSquads") }}</button>
      <button
        v-for="squad in squads"
        :key="squad.id"
        type="button"
        :class="{ active: squadFilter === squad.id }"
        @click="squadFilter = squad.id"
      >
        <Users :size="14" />
        {{ squad.name }}
      </button>
      <span v-if="!squads.length" class="task-scope-note">{{ t("tasks.noSquads") }}</span>
    </div>
    <p v-if="error" class="task-board-error">{{ error }}</p>
    <div class="task-board vs-scroll">
      <section
        v-for="column in columns"
        :key="column.id"
        class="task-column"
        :class="{ 'is-drop-target': dropStatus === column.id && !!dragId }"
        @dragover.prevent="onColumnOver($event, column.id)"
        @drop.prevent="finishDrop($event, column.id)"
      >
        <header>
          <component :is="icons[column.id]" :size="15" />
          <strong>{{ column.label }}</strong>
          <span>{{ columnTasks(column.id).length }}</span>
          <button type="button" class="task-icon-button" :title="t('tasks.columnMenu')" @click="openColumnMenu($event, column.id)">
            <MoreHorizontal :size="15" />
          </button>
          <button type="button" class="task-icon-button" :title="t('tasks.addTask')" @click="openCreate(column.id)">
            <Plus :size="15" />
          </button>
        </header>
        <div class="task-column-list vs-scroll">
          <article
            v-for="task in columnTasks(column.id)"
            :key="task.id"
            class="task-card"
            draggable="true"
            :class="{
              'is-dragging': dragId === task.id,
              'is-drop-before': dropStatus === column.id && dropBeforeId === task.id && dragId !== task.id,
            }"
            @dragstart="onDragStart($event, task)"
            @dragover.prevent="onCardOver($event, column.id, task.id)"
            @drop.prevent="finishDrop($event, column.id)"
            @dragend="onDragEnd"
            @click="openTask(task)"
            @contextmenu.prevent="openCardMenu($event, task)"
          >
            <span class="task-code">{{ taskCode(task, tasks) }}</span>
            <h3>{{ task.title }}</h3>
            <footer>
              <AgentAvatar :value="agentFor(task.agentId)?.avatar ?? 'M'" :name="agentFor(task.agentId)?.name ?? t('common.unassigned')" :size="18" />
              <span>{{ agentFor(task.agentId)?.name ?? t("common.unassigned") }}</span>
              <small>{{ t("tasks.updatedAt", { time: relativeTime(task.updatedAt) }) }}</small>
            </footer>
          </article>
          <p v-if="!columnTasks(column.id).length" class="task-column-empty">{{ t("tasks.columnEmpty") }}</p>
        </div>
      </section>
    </div>
    <TaskCreateDialog v-if="createOpen" :initial-status="createStatus" @close="createOpen = false" />
    <ContextMenu
      v-if="cardMenu"
      :x="cardMenu.x"
      :y="cardMenu.y"
      :items="cardMenuItems"
      @select="onCardMenu"
      @close="cardMenu = null"
    />
    <ContextMenu
      v-if="columnMenu"
      :x="columnMenu.x"
      :y="columnMenu.y"
      :items="columnMenuItems"
      @select="onColumnMenu"
      @close="columnMenu = null"
    />
  </div>
</template>
