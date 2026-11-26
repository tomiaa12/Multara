<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { vMotion } from "../ui/motion";
import { useRouter } from "vue-router";
import {
  Ban,
  Circle,
  CircleCheck,
  CircleDashed,
  CircleX,
  Clock3,
  MoreHorizontal,
  Plus,
  Rows3,
  Users,
} from "@lucide/vue";
import type { Task, TaskChangedFile, TaskStatus } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import AgentLiveStatus from "../components/AgentLiveStatus.vue";
import ContextMenu, { type ContextMenuItem } from "../components/ui/ContextMenu.vue";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import { readStorage, writeStorage } from "../utils/safe-storage";
import { vDragPan } from "../ui/drag-pan";
import { sharedRequirementCategories } from "./dashboard/requirement-categories";
import { moveTask, orderColumn, relativeTime, TASK_COLUMNS, taskCodeMap } from "./task-board";
import { readTaskMeta } from "./task-meta";
import { beginTaskCardDrag, cancelTaskCardDrag, type TaskDropTarget } from "./task-card-drag";
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
const requirementCategories = sharedRequirementCategories();
const categories = computed(() => requirementCategories.value.categories);
const error = ref("");
const dragId = ref("");
const dropStatus = ref<TaskStatus | "">("");
const dropBeforeId = ref("");
const boardRef = ref<HTMLElement | null>(null);
const cardMenu = ref<{ x: number; y: number; taskId: string } | null>(null);
const columnMenu = ref<{ x: number; y: number; status: TaskStatus } | null>(null);
const order = ref<Record<string, string[]>>(loadOrder());
let skipClick = false;

const icons = {
  backlog: CircleDashed,
  todo: Circle,
  in_progress: Clock3,
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
const sessionsById = computed(() => new Map((store.data?.sessions ?? []).map((item) => [item.id, item])));
const filtered = computed(() =>
  tasks.value.filter((task) => {
    const session = sessionsById.value.get(task.sessionId);
    if (scope.value === "agent" && agentFilter.value && task.agentId !== agentFilter.value) return false;
    if (scope.value === "squad") {
      if (session?.targetType !== "squad") return false;
      if (squadFilter.value && session.targetId !== squadFilter.value) return false;
    }
    if (projectFilters.value.length) {
      let projectIds = session?.projectIds ?? [];
      if (!projectIds.length && session?.projectId) projectIds = [session.projectId];
      if (!projectIds.some((id) => projectFilters.value.includes(id))) return false;
    }
    if (tagFilters.value.length && !taskMatchesTags(task, tagFilters.value)) return false;
    return true;
  }),
);

const categoryIdLookup = computed(() => new Map([
  ...categories.value.map((item) => [item.name, item.id] as const),
  ...categories.value.map((item) => [item.id, item.id] as const),
]));

function taskLabelIds(task: Task): string[] {
  const raw = [...(task.labels ?? []), ...readTaskMeta(task.id).labels];
  const ids: string[] = [];
  for (const item of raw) {
    const id = categoryIdLookup.value.get(item);
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
const workingCount = computed(() => {
  const activeSessions = new Set(
    Object.values(store.runStates)
      .filter((event) => (event.status === "queued" || event.status === "running") && !event.runId.startsWith("pending:"))
      .map((event) => event.sessionId),
  );
  return new Set(
    tasks.value.filter((task) => activeSessions.has(task.sessionId)).map((task) => task.agentId),
  ).size;
});
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
/** 仅待规划 / 待办列可直接新建任务 */
const CREATE_COLUMN_STATUSES: TaskStatus[] = ["backlog", "todo"];
function canAddInColumn(status: TaskStatus) {
  return CREATE_COLUMN_STATUSES.includes(status);
}
const columnMenuItems: ContextMenuItem[] = [{ id: "add", label: t("tasks.addTask") }];

function loadOrder() {
  try {
    const raw = readStorage(ORDER_KEY);
    const parsed = raw ? JSON.parse(raw) as Record<string, string[]> : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function loadDisplay() {
  try {
    const raw = readStorage(DISPLAY_KEY);
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
  writeStorage(DISPLAY_KEY, JSON.stringify({
    showDone: showDone.value,
    showCancelled: showCancelled.value,
  }));
}

function persist(status: TaskStatus, ids: string[]) {
  order.value = { ...order.value, [status]: ids };
  writeStorage(ORDER_KEY, JSON.stringify(order.value));
}

const tasksByColumn = computed(() => new Map(TASK_COLUMNS.map((column) => [
  column.id,
  orderColumn(filtered.value.filter((task) => task.status === column.id), order.value[column.id] ?? []),
])));
const taskCodes = computed(() => taskCodeMap(tasks.value));
const agentsById = computed(() => new Map(agents.value.map((agent) => [agent.id, agent])));

function columnTasks(status: TaskStatus) {
  return tasksByColumn.value.get(status) ?? [];
}

function taskAgentItem(agentId: string) {
  const agent = agentFor(agentId);
  return {
    key: agentId || "unassigned",
    name: agent?.name ?? t("common.unassigned"),
    image: agent?.avatar ?? "M",
  };
}

function agentFor(agentId: string) {
  return agentsById.value.get(agentId);
}

function openCreate(status: TaskStatus = "todo", lockStatus = false) {
  columnMenu.value = null;
  store.openTaskCreate(status, { lockStatus });
}

function openTask(task: Task) {
  if (skipClick) return;
  if (task.status === "backlog") {
    store.openTaskCreate("backlog", { lockStatus: true, taskId: task.id });
    return;
  }
  void router.push(`/tasks/${task.id}`);
}

function taskProjects(task: Task) {
  const session = sessionsById.value.get(task.sessionId);
  let ids = session?.projectIds ?? [];
  if (!ids.length && session?.projectId) ids = [session.projectId];
  return ids.flatMap((id) => {
    const project = projects.value.find((item) => item.id === id);
    return project ? [project] : [];
  });
}

function taskFiles(task: Task): TaskChangedFile[] {
  return task.changedFiles ?? [];
}

function htmlFile(task: Task): TaskChangedFile | undefined {
  return taskFiles(task).find((file) => /\.html?$/i.test(file.filePath));
}

function resultLabel(task: Task) {
  if (task.resultKind === "success") return t("tasks.result.success");
  if (task.resultKind === "failed") return t("tasks.result.failed");
  if (task.resultKind === "stopped") return t("tasks.result.stopped");
  return "";
}

function openChanges(task: Task) {
  void router.push({
    path: `/tasks/${task.id}`,
    query: { review: "1", at: String(Date.now()) },
  });
}

async function openHtml(task: Task, file: TaskChangedFile) {
  await router.push({
    path: `/tasks/${task.id}`,
    query: { preview: file.filePath, workspace: file.workspaceId, at: String(Date.now()) },
  });
}

function openRequirement(projectId: string) {
  void router.push(`/projects/${projectId}`);
}

function clearDragState() {
  dragId.value = "";
  dropStatus.value = "";
  dropBeforeId.value = "";
}

/** 拖完松手时浏览器还会补一个 click，吞掉它免得打开卡片。 */
function suppressNextClick() {
  skipClick = true;
  window.setTimeout(() => {
    skipClick = false;
  }, 0);
}

function onCardPointerDown(event: PointerEvent, task: Task) {
  if (event.pointerType === "mouse" && event.button !== 0) return;
  beginTaskCardDrag(event, task.id, boardRef.value, {
    onStart() {
      cardMenu.value = null;
      columnMenu.value = null;
      dragId.value = task.id;
    },
    onOver(target) {
      dropStatus.value = (target?.status as TaskStatus | undefined) ?? "";
      dropBeforeId.value = target?.beforeId ?? "";
    },
    onDrop(target) {
      clearDragState();
      suppressNextClick();
      if (target) void dropTask(task.id, target);
    },
    onCancel() {
      clearDragState();
      suppressNextClick();
    },
  });
}

async function dropTask(taskId: string, target: TaskDropTarget) {
  const status = target.status as TaskStatus;
  if (!TASK_COLUMNS.some((column) => column.id === status)) return;
  const task = tasks.value.find((item) => item.id === taskId);
  if (!task) return;
  const source = task.status;
  error.value = "";
  try {
    if (source !== status) await store.updateTaskStatus(taskId, status);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    return;
  }
  const destination = columnTasks(status).map((item) => item.id);
  persist(status, moveTask(destination, taskId, target.beforeId === taskId ? "" : target.beforeId));
  if (source !== status) persist(source, (order.value[source] ?? []).filter((id) => id !== taskId));
}

onBeforeUnmount(cancelTaskCardDrag);

function openCardMenu(event: MouseEvent, task: Task) {
  displayOpen.value = false;
  columnMenu.value = null;
  cardMenu.value = { x: event.clientX, y: event.clientY, taskId: task.id };
}

function openColumnMenu(event: MouseEvent, status: TaskStatus) {
  if (!canAddInColumn(status)) return;
  cardMenu.value = null;
  columnMenu.value = { x: event.clientX, y: event.clientY, status };
}

async function onCardMenu(id: string) {
  const task = menuTask.value;
  cardMenu.value = null;
  if (!task) return;
  if (id === "open") {
    openTask(task);
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
  if (id === "add") openCreate(status, true);
}
</script>

<template>
  <div class="tasks-page">
    <header class="task-toolbar">
      <div class="task-toolbar-title">
        <h1>{{ t("tasks.title") }}</h1>
        <UiButton variant="primary" size="small" :tip="t('tasks.createShortcut')" @click="openCreate()">
          <Plus :size="14" /> {{ t("tasks.create") }}
        </UiButton>
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
            <div v-motion="{ leave: 'dropdown' }" v-if="displayOpen" class="task-popover task-popover-start">
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
    <div ref="boardRef" v-drag-pan class="task-board vs-scroll">
      <section
        v-for="column in columns"
        :key="column.id"
        class="task-column"
        :class="{ 'is-drop-target': dropStatus === column.id && !!dragId }"
        :data-status="column.id"
      >
        <header>
          <component :is="icons[column.id]" :size="15" />
          <strong>{{ column.label }}</strong>
          <span>{{ columnTasks(column.id).length }}</span>
          <div v-if="canAddInColumn(column.id)" class="task-column-actions">
            <UiButton
              variant="ghost"
              size="icon"
              class="task-column-action"
              :tip="t('tasks.columnMenu')"
              @click="openColumnMenu($event, column.id)"
            >
              <MoreHorizontal :size="15" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon"
              class="task-column-action"
              :tip="t('tasks.addTask')"
              @click="openCreate(column.id, true)"
            >
              <Plus :size="15" />
            </UiButton>
          </div>
        </header>
        <TransitionGroup tag="div" name="ds-reorder" class="task-column-list vs-scroll" data-task-list>
          <article
            v-for="task in columnTasks(column.id)"
            :key="task.id"
            class="task-card"
            :data-task-id="task.id"
            data-pan-ignore
            :class="{
              'is-dragging': dragId === task.id,
              'is-drop-before': dropStatus === column.id && dropBeforeId === task.id && dragId !== task.id,
            }"
            @pointerdown="onCardPointerDown($event, task)"
            @click="openTask(task)"
            @contextmenu.prevent="openCardMenu($event, task)"
          >
            <span class="task-code">{{ taskCodes.get(task.id) }}</span>
            <h3>{{ task.title }}</h3>
            <p v-if="resultLabel(task)" class="task-card-meta">
              <span class="ds-badge" :data-tone="task.resultKind === 'failed' ? 'error' : task.resultKind === 'success' ? 'success' : undefined">{{ resultLabel(task) }}</span>
              <span v-if="task.resultReason && task.resultKind !== 'success'">{{ task.resultReason }}</span>
            </p>
            <p v-if="task.changedFileCount" class="task-card-meta">
              <span v-if="task.review" class="ds-badge" :data-tone="task.review === 'pending' ? 'warning' : undefined">
                {{ task.review === 'pending' ? t('tasks.review.pending') : t('tasks.review.seen') }}
              </span>
              {{ t('tasks.changedFiles', { n: task.changedFileCount }) }}
            </p>
            <div
              v-if="taskProjects(task).length"
              class="task-card-meta task-card-projects"
              @pointerdown.stop
              @click.stop
            >
              <button
                v-for="project in taskProjects(task)"
                :key="project.id"
                type="button"
                @click="openRequirement(project.id)"
              >{{ project.name }}</button>
            </div>
            <div
              v-if="taskFiles(task).length"
              class="task-card-actions"
              @pointerdown.stop
              @click.stop
            >
              <UiButton
                size="small"
                variant="ghost"
                @click="openChanges(task)"
              >{{ t('tasks.viewChanges') }}</UiButton>
              <UiButton
                v-if="htmlFile(task)"
                size="small"
                variant="ghost"
                @click="openHtml(task, htmlFile(task)!)"
              >{{ t('tasks.previewHtml') }}</UiButton>
            </div>
            <footer>
              <AgentLiveStatus
                :session-ids="[task.sessionId]"
                :fallback="[taskAgentItem(task.agentId)]"
                show-name
                :size="18"
              />
              <small>{{ t("tasks.updatedAt", { time: relativeTime(task.updatedAt) }) }}</small>
            </footer>
          </article>
          <p v-if="!columnTasks(column.id).length" key="__empty" class="task-column-empty">{{ t("tasks.columnEmpty") }}</p>
        </TransitionGroup>
      </section>
    </div>
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
