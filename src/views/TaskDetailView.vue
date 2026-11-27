<script setup lang="ts">
import WorkflowRunBar from "../components/workflow/WorkflowRunBar.vue";
import { isWorkflowRunSession } from "../domain/workflow";
import { computed, nextTick, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ArrowLeft, ListTodo } from "@lucide/vue";
import type { EditorSelectionContext } from "../domain/types";
import ChatComposer from "../components/chat/ChatComposer.vue";
import {
  isFileChip,
  parseUserPromptNodes,
  type ComposerNode,
} from "../components/chat/composer-content";
import { useAppStore } from "../stores/app";
import TaskActivity from "./TaskActivity.vue";
import InboxToolRail from "./inbox/InboxToolRail.vue";
import { t } from "../i18n";

const store = useAppStore();
const route = useRoute();
const router = useRouter();
const error = ref("");
const titleDraft = ref("");
const titleEl = ref<HTMLTextAreaElement | null>(null);
const contexts = ref<EditorSelectionContext[]>([]);
const descriptionNodes = ref<ComposerNode[]>([{ type: "text", value: "" }]);
const descriptionKey = ref(0);
const stage = ref({ content: false, width: 0, terminal: false });
const rail = ref<{ openWorkspaceFile: (workspaceId: string, path: string, startLine?: number) => void } | null>(null);
const activity = ref<InstanceType<typeof TaskActivity> | null>(null);

const task = computed(() => store.data?.tasks.find((item) => item.id === route.params.taskId));
const workspaces = computed(() => {
  const session = store.data?.sessions.find((item) => item.id === task.value?.sessionId);
  const ids = session?.projectIds?.length
    ? session.projectIds
    : session?.projectId
      ? [session.projectId]
      : [];
  const all = store.data?.workspaces ?? [];
  const linked = all.filter((item) =>
    ids.includes(item.projectId) && (!item.sessionId || item.sessionId === task.value?.sessionId),
  );
  const extraIds = new Set((task.value?.changedFiles ?? []).map((file) => file.workspaceId));
  const extra = all.filter((item) => extraIds.has(item.id) && !linked.some((existing) => existing.id === item.id));
  return [...linked, ...extra];
});
const stageStyle = computed(() =>
  stage.value.content ? { "--inbox-stage-width": `${stage.value.width}px` } : undefined,
);
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

function fileUrl(workspaceId: string, filePath: string) {
  const workspace = workspaces.value.find((item) => item.id === workspaceId)
    ?? store.data?.workspaces.find((item) => item.id === workspaceId);
  const root = workspace?.workPath || workspace?.sourcePath || "";
  if (root) return `file://${root.replace(/\/$/, "")}/${filePath.replace(/^\//, "")}`;
  if (filePath.startsWith("/")) return `file://${filePath}`;
  return "";
}

function onLayout(payload: { content: boolean; files: boolean; width: number; terminal: boolean }) {
  stage.value = { content: payload.content, width: payload.width, terminal: payload.terminal };
}

watch(
  () => [task.value?.id, route.query.review, route.query.preview, route.query.workspace, route.query.at] as const,
  async ([id, review, preview, workspace]) => {
    if (!id) return;
    await nextTick();
    const tool = rail.value;
    if (!tool || !task.value) return;
    if (typeof preview === "string" && preview) {
      const workspaceId = typeof workspace === "string" ? workspace : "";
      const url = /\.html?$/i.test(preview) ? fileUrl(workspaceId, preview) : "";
      if (url) store.revealBrowser(url);
      else tool.openWorkspaceFile(workspaceId, preview);
      return;
    }
    if (review !== "1") return;
    if (task.value.review === "pending") void store.acknowledgeTaskReview(id);
    for (const file of task.value.changedFiles ?? []) {
      tool.openWorkspaceFile(file.workspaceId, file.filePath);
    }
  },
  { immediate: true },
);

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

</script>

<template>
  <div
    v-if="task"
    class="inbox-page task-detail-page"
    :class="{ 'has-stage': stage.content, 'has-terminal': stage.terminal }"
    :style="stageStyle"
  >
    <section class="inbox-detail chat-surface">
      <div class="task-issue">
        <div class="task-issue-body side-closed">
          <main class="task-issue-main vs-scroll">
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
              <TaskActivity ref="activity" docked :session-id="task.sessionId" :agent-id="task.agentId" />
            </section>
            <p v-if="error" class="task-board-error">{{ error }}</p>
          </main>
        </div>
      </div>
    </section>
    <InboxToolRail
      ref="rail"
      :workspaces="workspaces"
      :workspace-id="workspaces[0]?.id ?? ''"
      :session-id="task.sessionId"
      kind="task"
      log-label="任务"
      float-host-selector=".task-detail-page"
      :task-id="task.id"
      @layout="onLayout"
      @add-to-chat="activity?.insertChip($event)"
    />
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
