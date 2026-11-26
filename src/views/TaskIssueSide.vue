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
import { readTaskMeta, writeTaskMeta } from "./task-meta";
import { t } from "../i18n";

const props = defineProps<{
  taskId: string;
}>();

const store = useAppStore();
const router = useRouter();
const showTagManage = ref(false);
const categories = sharedRequirementCategories();
const runs = ref<Awaited<ReturnType<typeof backend.listTaskRuns>>>([]);
const metaTick = ref(0);

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
  return task.value ? readTaskMeta(task.value.id) : { labels: [] as string[] };
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
watch(
  () => task.value?.id,
  (id) => {
    if (!id || !task.value) {
      runs.value = [];
      return;
    }
    void backend.listTaskRuns(id).then((items) => {
      runs.value = items;
    }).catch(() => {
      runs.value = [];
    });
  },
  { immediate: true },
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

const resultLabel = computed(() => {
  if (task.value?.resultKind === "success") return t("tasks.result.success");
  if (task.value?.resultKind === "failed") return t("tasks.result.failed");
  if (task.value?.resultKind === "stopped") return t("tasks.result.stopped");
  return "";
});

function openChanges() {
  if (!task.value) return;
  void router.replace({ path: `/tasks/${task.value.id}`, query: { review: "1" } });
}

function openRequirement(projectId: string) {
  void router.push(`/projects/${projectId}`);
}

async function markReviewed() {
  if (!task.value) return;
  await store.acknowledgeTaskReview(task.value.id);
}

function openLatestRunLog() {
  const latest = [...runs.value].sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
  if (latest) openRun(latest);
}
</script>

<template>
  <aside v-if="task" class="task-issue-side vs-scroll">
    <section v-if="resultLabel || task.changedFileCount">
      <h2>{{ t('tasks.resultTitle') }}</h2>
      <div v-if="resultLabel" class="task-prop-row">
        <span>{{ t('tasks.resultTitle') }}</span>
        <span>{{ resultLabel }}</span>
      </div>
      <p v-if="task.resultReason && task.resultKind !== 'success'" class="task-result-reason">{{ task.resultReason }}</p>
      <div v-if="task.changedFileCount" class="task-prop-row">
        <span>{{ t('tasks.changedFiles', { n: task.changedFileCount }) }}</span>
        <span v-if="task.review">{{ task.review === 'pending' ? t('tasks.review.pending') : t('tasks.review.seen') }}</span>
      </div>
      <div class="task-result-actions">
        <UiButton v-if="task.changedFileCount" size="small" variant="ghost" @click="openChanges">{{ t('tasks.viewChanges') }}</UiButton>
        <UiButton v-if="task.review === 'pending'" size="small" variant="ghost" @click="markReviewed">{{ t('tasks.markReviewed') }}</UiButton>
      </div>
    </section>
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
        <span>{{ t('tasks.project') }}</span>
        <span v-if="projects.length" class="task-project-links">
          <button
            v-for="project in projects"
            :key="project.id"
            type="button"
            @click="openRequirement(project.id)"
          >{{ project.name }}</button>
        </span>
        <span v-else>{{ t('tasks.noProject') }}</span>
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
