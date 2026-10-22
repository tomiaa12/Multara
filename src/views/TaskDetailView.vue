<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ArrowLeft, ListTodo, PanelRight, ScrollText, Trash2, User } from "@lucide/vue";
import type { TaskRunRecord, TaskStatus } from "../domain/types";
import {
  elapsedFromMessages,
  failureFromMessages,
  openRunLog,
  runStatusLabel,
  runStatusTone,
  timelineFromMessages,
} from "../components/run-log";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";
import { relativeTime, TASK_COLUMNS, taskStatusLabel } from "./task-board";
import TagManageDialog from "../components/ui/TagManageDialog.vue";
import TagPicker from "../components/ui/TagPicker.vue";
import {
  addRequirementCategory,
  loadRequirementCategories,
  recolorRequirementCategory,
  removeRequirementCategory,
  renameRequirementCategory,
  saveRequirementCategories,
  type RequirementCategoryState,
} from "./dashboard/requirement-categories";
import { PRIORITY_OPTIONS, readTaskMeta, writeTaskMeta, type TaskPriority } from "./task-meta";
import TaskActivity from "./TaskActivity.vue";
import { t } from "../i18n";

const PRIORITY_KEYS: Record<TaskPriority, string> = {
  none: "tasks.priority.none",
  urgent: "tasks.priority.urgent",
  high: "tasks.priority.high",
  medium: "tasks.priority.medium",
  low: "tasks.priority.low",
};

const store = useAppStore();
const route = useRoute();
const router = useRouter();
const sideOpen = ref(true);
const error = ref("");
const titleDraft = ref("");
const descriptionDraft = ref("");
const showTagManage = ref(false);
const categories = ref<RequirementCategoryState>(loadRequirementCategories(localStorage));
const runs = ref<Awaited<ReturnType<typeof backend.listTaskRuns>>>([]);
const titleEl = ref<HTMLInputElement | null>(null);
const issueScroll = ref<HTMLElement | null>(null);
const followBottom = ref(true);
const descriptionEl = ref<HTMLTextAreaElement | null>(null);
const metaTick = ref(0);

const task = computed(() => store.data?.tasks.find((item) => item.id === route.params.taskId));
const session = computed(() => store.data?.sessions.find((item) => item.id === task.value?.sessionId));
const agent = computed(() => store.data?.agents.find((item) => item.id === task.value?.agentId));
const project = computed(() => store.data?.projects.find((item) => item.id === session.value?.projectId));
const meta = computed(() => {
  void metaTick.value;
  return task.value ? readTaskMeta(task.value.id) : { priority: "none" as TaskPriority, labels: [] as string[] };
});
const taskLabelIds = computed(() => {
  const known = new Map(categories.value.categories.map((item) => [item.id, item.id]));
  const byName = new Map(categories.value.categories.map((item) => [item.name, item.id]));
  const raw = [...(task.value?.labels ?? []), ...meta.value.labels];
  const ids: string[] = [];
  for (const item of raw) {
    const id = known.get(item) ?? byName.get(item);
    if (id && !ids.includes(id)) ids.push(id);
  }
  return ids;
});

let pinningIssue = false;

function pinIssueToBottom(force = false) {
  const element = issueScroll.value;
  if (!element || (!force && !followBottom.value)) return;
  pinningIssue = true;
  element.scrollTop = element.scrollHeight;
  requestAnimationFrame(() => {
    if (issueScroll.value) issueScroll.value.scrollTop = issueScroll.value.scrollHeight;
    pinningIssue = false;
  });
}

function onIssueScroll() {
  if (pinningIssue) return;
  const element = issueScroll.value;
  if (!element) return;
  followBottom.value = element.scrollHeight - element.scrollTop - element.clientHeight < 80;
}

watch(
  () => task.value?.id,
  () => {
    followBottom.value = true;
    void nextTick(() => pinIssueToBottom(true));
  },
  { immediate: true },
);

watch(
  () => {
    const sessionId = task.value?.sessionId;
    const list = sessionId ? store.messages[sessionId] ?? [] : [];
    const last = list.at(-1);
    return [sessionId, list.length, last?.content.length ?? 0] as const;
  },
  async (current, previous) => {
    const sessionChanged = !previous || current[0] !== previous[0];
    const historyFilled = Number(previous?.[1] ?? 0) === 0 && current[1] > 0;
    if (!sessionChanged && !historyFilled && !followBottom.value) return;
    await nextTick();
    pinIssueToBottom(sessionChanged || historyFilled);
  },
);

const checklistDraft = ref("");
const prDraft = ref("");

/** WebView 对 field-sizing 支持不稳，按内容撑开，避免多行描述被 overflow:hidden 裁掉。 */
function fitDescription() {
  const el = descriptionEl.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = `${el.scrollHeight}px`;
}

watch(task, (value) => {
  if (!value) return;
  if (document.activeElement !== titleEl.value) titleDraft.value = value.title;
  if (document.activeElement !== descriptionEl.value) descriptionDraft.value = value.description;
  prDraft.value = value.prUrl ?? "";
  void nextTick(fitDescription);
  void backend.listTaskRuns(value.id).then((items) => {
    runs.value = items;
  }).catch(() => {
    runs.value = [];
  });
}, { immediate: true });

watch(descriptionDraft, () => {
  void nextTick(fitDescription);
});

function formatDate(iso: string) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return t("tasks.dateMonthDay", { month: date.getMonth() + 1, day: date.getDate() });
}

async function saveTitle() {
  if (!task.value || titleDraft.value.trim() === task.value.title) return;
  try {
    await store.updateTask({ taskId: task.value.id, title: titleDraft.value.trim() });
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function saveDescription() {
  if (!task.value || descriptionDraft.value === task.value.description) return;
  try {
    await store.updateTask({ taskId: task.value.id, description: descriptionDraft.value });
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function changeStatus(status: TaskStatus) {
  if (!task.value || task.value.status === status) return;
  try {
    await store.updateTaskStatus(task.value.id, status);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function changeAgent(agentId: string) {
  if (!task.value || !agentId || task.value.agentId === agentId) return;
  try {
    await store.updateTask({ taskId: task.value.id, agentId });
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function changePriority(priority: TaskPriority) {
  if (!task.value) return;
  writeTaskMeta(task.value.id, { priority });
  metaTick.value += 1;
  await store.updateTaskMeta({ taskId: task.value.id, priority });
}

function persistCategories(next: RequirementCategoryState) {
  categories.value = next;
  saveRequirementCategories(localStorage, next);
}

function createCategory(name: string) {
  const id = crypto.randomUUID();
  const next = addRequirementCategory(categories.value, name, id);
  if (next === categories.value) return "";
  persistCategories(next);
  return id;
}

async function setTaskLabels(labels: string[]) {
  if (!task.value) return;
  writeTaskMeta(task.value.id, { labels });
  metaTick.value += 1;
  await store.updateTaskMeta({ taskId: task.value.id, labels });
}

async function onTagCreate(name: string) {
  const id = createCategory(name);
  if (!id) return;
  await setTaskLabels([...taskLabelIds.value, id]);
}

async function addChecklist() {
  if (!task.value) return;
  const text = checklistDraft.value.trim();
  if (!text) return;
  const checklist = [...(task.value.checklist ?? []), { id: crypto.randomUUID(), text, done: false }];
  checklistDraft.value = "";
  await store.updateTaskMeta({ taskId: task.value.id, checklist });
}

async function toggleChecklist(id: string) {
  if (!task.value) return;
  const checklist = (task.value.checklist ?? []).map((item) =>
    item.id === id ? { ...item, done: !item.done } : item,
  );
  await store.updateTaskMeta({ taskId: task.value.id, checklist });
}

async function savePr() {
  if (!task.value) return;
  await store.updateTaskMeta({ taskId: task.value.id, prUrl: prDraft.value.trim() });
}

const childTasks = computed(() =>
  (store.data?.tasks ?? []).filter((item) => item.parentTaskId === task.value?.id),
);

watch(
  () => childTasks.value.map((item) => item.id).join(","),
  async (current, previous) => {
    if (!task.value || current === previous) return;
    await store.loadMessages(task.value.sessionId);
  },
);
const childDraft = ref("");

async function createChild() {
  if (!task.value) return;
  const title = childDraft.value.trim();
  if (!title) return;
  childDraft.value = "";
  const created = await store.createTask({
    title,
    description: "",
    agentId: task.value.agentId,
    projectId: session.value?.projectId ?? null,
    parentTaskId: task.value.id,
  });
  if (created) await router.push(`/tasks/${created.id}`);
}

async function removeTask() {
  if (!task.value) return;
  try {
    if (!(await store.confirmDeleteTask(task.value.id))) return;
    await router.push("/tasks");
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function ensureSessionMessages() {
  if (!task.value?.sessionId) return;
  await store.loadMessages(task.value.sessionId);
}

function openRun(run: TaskRunRecord) {
  if (!task.value) return;
  void ensureSessionMessages().then(() => {
    if (!task.value) return;
    const messages = store.messages[task.value.sessionId] ?? [];
    const live = store.runStates[run.id];
    openRunLog({
      runId: run.id,
      status: live?.status ?? run.status,
      createdAt: run.createdAt,
      elapsedMs: live?.elapsedMs ?? elapsedFromMessages(messages, run.id),
      agentName: store.data?.agents.find((item) => item.id === run.agentId)?.name
        ?? agent.value?.name,
      error: live?.error ?? failureFromMessages(messages, run.id),
      timeline: timelineFromMessages(messages, run.id),
      title: t("tasks.runLog"),
    });
  });
}


function openLatestRunLog() {
  const latest = [...runs.value].sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
  if (latest) openRun(latest);
}
</script>

<template>
  <div v-if="task" class="task-issue">
    <header class="task-issue-bar">
      <span class="task-issue-crumb" />
      <UiButton variant="ghost" size="icon" class="task-issue-delete" :title="t('tasks.delete')" :aria-label="t('tasks.delete')" @click="removeTask">
        <Trash2 :size="15" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :class="{ active: sideOpen }"
        :title="t('tasks.propertiesPanel')"
        :aria-label="t('tasks.propertiesPanel')"
        :aria-pressed="sideOpen"
        @click="sideOpen = !sideOpen"
      >
        <PanelRight :size="16" />
      </UiButton>
    </header>
    <div class="task-issue-body" :class="{ 'side-closed': !sideOpen }">
      <main class="task-issue-main">
        <div ref="issueScroll" class="task-issue-scroll vs-scroll" @scroll.passive="onIssueScroll">
          <div class="task-issue-content">
            <input
              ref="titleEl"
              v-model="titleDraft"
              class="task-issue-title"
              :aria-label="t('tasks.titleField')"
              @blur="saveTitle"
              @keydown.enter.prevent="titleEl?.blur()"
            />
            <textarea
              ref="descriptionEl"
              v-model="descriptionDraft"
              class="task-issue-description"
              :placeholder="t('tasks.addDescription')"
              rows="1"
              @input="fitDescription"
              @blur="saveDescription"
            />
            <section class="task-feed">
              <header>{{ t('tasks.activity') }}</header>
              <TaskActivity :session-id="task.sessionId" :agent-id="task.agentId" />
            </section>
          </div>
        </div>
        <p v-if="error" class="task-board-error">{{ error }}</p>
      </main>
      <aside v-if="sideOpen" class="task-issue-side vs-scroll">
        <section>
          <h2>{{ t('tasks.properties') }}</h2>
          <div class="task-prop-row">
            <span>{{ t('tasks.field.status') }}</span>
            <span class="task-prop-status">
              <i :data-status="task.status" />
              <UiSelect
                class="task-prop-select"
                variant="ghost"
                size="sm"
                :aria-label="t('tasks.field.status')"
                :model-value="task.status"
                :options="TASK_COLUMNS.map((column) => ({ value: column.id, label: taskStatusLabel(column.id) }))"
                @change="changeStatus($event as TaskStatus)"
              />
            </span>
          </div>
          <div class="task-prop-row">
            <span>{{ t('tasks.assignee') }}</span>
            <span class="task-prop-person">
              <UiSelect
                class="task-prop-select"
                variant="ghost"
                size="sm"
                :aria-label="t('tasks.assignee')"
                :placeholder="t('common.unassigned')"
                :model-value="task.agentId"
                :options="(store.data?.agents ?? []).map((item) => ({ value: item.id, label: item.name, avatar: item.avatar }))"
                @change="changeAgent(String($event))"
              />
            </span>
          </div>
          <div class="task-prop-row">
            <span>{{ t('tasks.project') }}</span>
            <span>{{ project?.name ?? t('tasks.noProject') }}</span>
          </div>
          <div class="task-prop-row">
            <span>{{ t('tasks.priority') }}</span>
            <UiSelect
              class="task-prop-select"
              variant="ghost"
              size="sm"
              :aria-label="t('tasks.priority')"
              :model-value="meta.priority"
              :options="PRIORITY_OPTIONS.map((option) => ({ value: option.id, label: t(PRIORITY_KEYS[option.id]) }))"
              @change="changePriority($event as TaskPriority)"
            />
          </div>
          <div class="task-prop-row">
            <span>{{ t('tasks.tags') }}</span>
            <TagPicker
              :model-value="taskLabelIds"
              :tags="categories.categories"
              :placeholder="t('tasks.addTags')"
              @update:model-value="setTaskLabels"
              @create="onTagCreate"
              @manage="showTagManage = true"
            />
          </div>
        </section>
        <section>
          <h2>{{ t('tasks.checklist') }}</h2>
          <ul v-if="task.checklist?.length" class="task-checklist">
            <li v-for="item in task.checklist" :key="item.id">
              <label>
                <input type="checkbox" :checked="item.done" @change="toggleChecklist(item.id)" />
                {{ item.text }}
              </label>
            </li>
          </ul>
          <input v-model="checklistDraft" class="task-prop-input" :placeholder="t('tasks.addChecklist')" @keydown.enter.prevent="addChecklist" />
        </section>
        <section>
          <h2>{{ t('tasks.subtasks') }}</h2>
          <ul v-if="childTasks.length" class="task-runs">
            <li v-for="child in childTasks" :key="child.id">
              <button type="button" class="task-run-row" @click="router.push(`/tasks/${child.id}`)">
                <span>{{ child.title }}</span>
                <span>{{ taskStatusLabel(child.status) }}</span>
              </button>
            </li>
          </ul>
          <p v-else>{{ t('tasks.noSubtasks') }}</p>
          <input v-model="childDraft" class="task-prop-input" :placeholder="t('tasks.newSubtask')" @keydown.enter.prevent="createChild" />
        </section>
        <section>
          <h2>Pull Request</h2>
          <input v-model="prDraft" class="task-prop-input" placeholder="https://…" @change="savePr" />
        </section>
        <section>
          <div class="task-side-heading">
            <h2>{{ t('tasks.runLog') }}</h2>
            <UiButton
              v-if="runs.length"
              variant="ghost"
              size="small"
              class="task-run-log-btn"
              @click="openLatestRunLog"
            ><ScrollText :size="14" />{{ t('tasks.latestRunLog') }}</UiButton>
          </div>
          <p v-if="!runs.length">{{ t('tasks.noRunHistory') }}</p>
          <ul v-else class="task-runs">
            <li v-for="run in runs" :key="run.id">
              <button type="button" class="task-run-row" @click="openRun(run)">
                <span class="ds-badge" :data-tone="runStatusTone(run.status)">{{ runStatusLabel(run.status) }}</span>
                <span>{{ relativeTime(run.createdAt) }}</span>
              </button>
            </li>
          </ul>
        </section>
        <section>
          <h2>{{ t('tasks.meta') }}</h2>
          <div class="task-prop-row">
            <span>{{ t('tasks.creator') }}</span>
            <span class="task-prop-person"><i class="task-creator-mark" aria-hidden="true"><User :size="10" /></i>{{ t('tasks.creatorMe') }}</span>
          </div>
          <div class="task-prop-row">
            <span>{{ t('tasks.createdAt') }}</span>
            <span>{{ formatDate(task.createdAt) }}</span>
          </div>
          <div class="task-prop-row">
            <span>{{ t('tasks.updatedAtLabel') }}</span>
            <span>{{ formatDate(task.updatedAt) }}</span>
          </div>
        </section>
      </aside>
    </div>
  </div>
  <div v-else class="tasks-page task-issue-missing">
    <div class="ds-empty">
      <span class="ds-empty-icon"><ListTodo :size="20" /></span>
      <h3>{{ t('tasks.notFoundTitle') }}</h3>
      <p>{{ t('tasks.notFoundHint') }}</p>
      <UiButton @click="router.push('/tasks')"><ArrowLeft :size="14" />{{ t('tasks.back') }}</UiButton>
    </div>
  </div>
  <TagManageDialog
    v-if="showTagManage"
    :tags="categories.categories"
    @close="showTagManage = false"
    @create="createCategory"
    @rename="(id, name) => persistCategories(renameRequirementCategory(categories, id, name))"
    @recolor="(id, color) => persistCategories(recolorRequirementCategory(categories, id, color))"
    @remove="async (id) => {
      persistCategories(removeRequirementCategory(categories, id));
      await setTaskLabels(taskLabelIds.filter((item) => item !== id));
    }"
  />
</template>
