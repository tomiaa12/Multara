<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { ScrollText, User } from "@lucide/vue";
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
  recolorRequirementCategory,
  removeRequirementCategory,
  renameRequirementCategory,
  saveRequirementCategories,
  sharedRequirementCategories,
  type RequirementCategoryState,
} from "./dashboard/requirement-categories";
import { PRIORITY_OPTIONS, readTaskMeta, writeTaskMeta, type TaskPriority } from "./task-meta";
import { t } from "../i18n";

const PRIORITY_KEYS: Record<TaskPriority, string> = {
  none: "tasks.priority.none",
  urgent: "tasks.priority.urgent",
  high: "tasks.priority.high",
  medium: "tasks.priority.medium",
  low: "tasks.priority.low",
};

const props = defineProps<{
  taskId: string;
}>();

const store = useAppStore();
const router = useRouter();
const showTagManage = ref(false);
const categories = sharedRequirementCategories();
const runs = ref<Awaited<ReturnType<typeof backend.listTaskRuns>>>([]);
const metaTick = ref(0);
const checklistDraft = ref("");
const childDraft = ref("");
const prDraft = ref("");

const task = computed(() => store.data?.tasks.find((item) => item.id === props.taskId));
const session = computed(() => store.data?.sessions.find((item) => item.id === task.value?.sessionId));
const agent = computed(() => store.data?.agents.find((item) => item.id === task.value?.agentId));
const projectIds = computed(() => {
  if (session.value?.projectIds?.length) return session.value.projectIds;
  return session.value?.projectId ? [session.value.projectId] : [];
});
const projects = computed(() =>
  projectIds.value.flatMap((id) => {
    const project = store.data?.projects.find((item) => item.id === id);
    return project ? [project] : [];
  }),
);
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
const childTasks = computed(() =>
  (store.data?.tasks ?? []).filter((item) => item.parentTaskId === task.value?.id),
);

watch(
  () => task.value?.id,
  (id) => {
    if (!id || !task.value) {
      runs.value = [];
      return;
    }
    prDraft.value = task.value.prUrl ?? "";
    void backend.listTaskRuns(id).then((items) => {
      runs.value = items;
    }).catch(() => {
      runs.value = [];
    });
  },
  { immediate: true },
);

watch(
  () => childTasks.value.map((item) => item.id).join(","),
  async (current, previous) => {
    if (!task.value || current === previous) return;
    await store.loadMessages(task.value.sessionId);
  },
);

function formatDate(iso: string) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return t("tasks.dateMonthDay", { month: date.getMonth() + 1, day: date.getDate() });
}

async function changeStatus(status: TaskStatus) {
  if (!task.value || task.value.status === status) return;
  await store.updateTaskStatus(task.value.id, status);
}

async function changeAgent(agentId: string) {
  if (!task.value || !agentId || task.value.agentId === agentId) return;
  await store.updateTask({ taskId: task.value.id, agentId });
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
    projectIds: [...projectIds.value],
    parentTaskId: task.value.id,
  });
  if (created) await router.push(`/tasks/${created.id}`);
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
  <aside v-if="task" class="task-issue-side vs-scroll">
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
        <span>{{ projects.length ? projects.map((item) => item.name).join('、') : t('tasks.noProject') }}</span>
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
          size="sm"
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
