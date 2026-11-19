<script setup lang="ts">
import WorkflowRunBar from "../components/workflow/WorkflowRunBar.vue";
import { isWorkflowRunSession } from "../domain/workflow";
import { computed, nextTick, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ArrowLeft, ListTodo, PanelRight, Trash2 } from "@lucide/vue";
import type { EditorSelectionContext } from "../domain/types";
import ChatComposer from "../components/chat/ChatComposer.vue";
import {
  isFileChip,
  parseUserPromptNodes,
  type ComposerNode,
} from "../components/chat/composer-content";
import { useAppStore } from "../stores/app";
import TaskActivity from "./TaskActivity.vue";
import TaskIssueSide from "./TaskIssueSide.vue";
import { t } from "../i18n";

const store = useAppStore();
const route = useRoute();
const router = useRouter();
const sideOpen = ref(true);
const error = ref("");
const titleDraft = ref("");
const titleEl = ref<HTMLTextAreaElement | null>(null);
const contexts = ref<EditorSelectionContext[]>([]);
const descriptionNodes = ref<ComposerNode[]>([{ type: "text", value: "" }]);
const descriptionKey = ref(0);

const task = computed(() => store.data?.tasks.find((item) => item.id === route.params.taskId));
const hasDescription = computed(() =>
  descriptionNodes.value.some((node) =>
    node.type === "chip" || (node.type === "text" && node.value.replace(/\u00a0/g, " ").trim()),
  ),
);
/** 创建任务时描述会原样变成第一条用户消息，动态里已经有，标题下不再画一遍。 */
const showDescription = computed(() => {
  if (!hasDescription.value || !task.value) return false;
  const list = store.messages[task.value.sessionId];
  if (!list) return false;
  const opening = list.find((message) => message.role === "user");
  if (!opening) return true;
  const description = task.value.description.replace(/\u00a0/g, " ").trim();
  const content = opening.content.replace(/\u00a0/g, " ").trim();
  return description !== content;
});

function fitTitle() {
  const el = titleEl.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = `${el.scrollHeight}px`;
}

function syncDescriptionFromTask(description: string) {
  const nodes = parseUserPromptNodes(description);
  descriptionNodes.value = nodes.length ? nodes : [{ type: "text", value: "" }];
  contexts.value = nodes.filter(isFileChip).map((node) => ({
    id: node.id,
    workspaceId: "",
    path: node.path,
    startLine: node.startLine,
    endLine: node.endLine,
    text: node.text,
    isDirectory: node.isDirectory,
  }));
  descriptionKey.value += 1;
}

watch(task, (value) => {
  if (!value) return;
  if (document.activeElement !== titleEl.value) titleDraft.value = value.title;
  syncDescriptionFromTask(value.description);
  void nextTick(fitTitle);
}, { immediate: true });

watch(titleDraft, () => {
  void nextTick(fitTitle);
});

async function saveTitle() {
  if (!task.value || titleDraft.value.trim() === task.value.title) return;
  try {
    await store.updateTask({ taskId: task.value.id, title: titleDraft.value.trim() });
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
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
</script>

<template>
  <div v-if="task" class="task-issue">
    <header class="task-issue-bar">
      <span class="task-issue-crumb" />
      <UiButton variant="ghost" size="icon" class="task-issue-delete" :tip="t('tasks.delete')" :aria-label="t('tasks.delete')" @click="removeTask">
        <Trash2 :size="15" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :class="{ active: sideOpen }"
        :tip="t('tasks.propertiesPanel')"
        :aria-label="t('tasks.propertiesPanel')"
        :aria-pressed="sideOpen"
        @click="sideOpen = !sideOpen"
      >
        <PanelRight :size="16" />
      </UiButton>
    </header>
    <div class="task-issue-body" :class="{ 'side-closed': !sideOpen }">
      <main class="task-issue-main">
        <div class="task-issue-hero">
          <div class="task-issue-content">
            <WorkflowRunBar v-if="isWorkflowRunSession(task.sessionId)" :session-id="task.sessionId" />
            <textarea
              ref="titleEl"
              v-model="titleDraft"
              class="task-issue-title"
              rows="1"
              :aria-label="t('tasks.titleField')"
              @input="fitTitle"
              @blur="saveTitle"
            />
            <ChatComposer
              v-if="showDescription"
              :key="descriptionKey"
              class="task-issue-description is-view-only"
              embedded
              readonly
              placeholder=""
              :initial-nodes="descriptionNodes"
              :contexts="contexts"
              :show-context-usage="false"
              :allow-target-change="false"
              :can-send="false"
              target-label=""
              target-avatar=""
            />
          </div>
        </div>
        <section class="task-feed task-feed-fill">
          <header>{{ t('tasks.activity') }}</header>
          <TaskActivity docked :session-id="task.sessionId" :agent-id="task.agentId" />
        </section>
        <p v-if="error" class="task-board-error">{{ error }}</p>
      </main>
      <TaskIssueSide v-if="sideOpen" :task-id="task.id" />
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
</template>
