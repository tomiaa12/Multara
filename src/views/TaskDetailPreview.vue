<script setup lang="ts">
import { computed, ref } from "vue";
import { ArrowLeft, ListTodo, PanelRight, ScrollText, Trash2, User } from "@lucide/vue";
import AssistantMessage from "../components/chat/AssistantMessage.vue";
import ChatComposer from "../components/chat/ChatComposer.vue";
import UserPrompt from "../components/chat/UserPrompt.vue";
import {
  isFileChip,
  parseUserPromptNodes,
  type ComposerNode,
} from "../components/chat/composer-content";
import {
  shouldHideAssistantSpeaker,
  speakerForMessage,
  subtaskEmbedForMessage,
} from "../components/chat/message-speaker";
import {
  runStatusLabel,
  runStatusTone,
} from "../components/run-log";
import type { ChatMessage, EditorSelectionContext } from "../domain/types";
import { t } from "../i18n";
import { PRIORITY_OPTIONS, type TaskPriority } from "./task-meta";
import { relativeTime, TASK_COLUMNS, taskStatusLabel } from "./task-board";
import type { TaskDetailPreviewScenario } from "./preview-task-detail";

const PRIORITY_KEYS: Record<TaskPriority, string> = {
  none: "tasks.priority.none",
  urgent: "tasks.priority.urgent",
  high: "tasks.priority.high",
  medium: "tasks.priority.medium",
  low: "tasks.priority.low",
};

const props = defineProps<{
  scenario: TaskDetailPreviewScenario;
}>();

const sideOpen = ref(true);

const task = computed(() => props.scenario.task);
const messages = computed(() => props.scenario.messages);
const agents = computed(() => props.scenario.agents);
const side = computed(() => props.scenario.side);
const agent = computed(() =>
  agents.value.find((item) => item.id === task.value?.agentId) ?? agents.value[0] ?? null,
);

const descriptionNodes = computed((): ComposerNode[] => {
  const nodes = parseUserPromptNodes(task.value?.description ?? "");
  return nodes.length ? nodes : [{ type: "text", value: "" }];
});
const hasDescription = computed(() =>
  descriptionNodes.value.some((node) =>
    node.type === "chip" || (node.type === "text" && node.value.replace(/\u00a0/g, " ").trim()),
  ),
);
const contexts = computed((): EditorSelectionContext[] =>
  descriptionNodes.value.filter(isFileChip).map((node) => ({
    id: node.id,
    workspaceId: "",
    path: node.path,
    startLine: node.startLine,
    endLine: node.endLine,
    text: node.text,
    isDirectory: node.isDirectory,
  })),
);

function speaker(message: ChatMessage) {
  return speakerForMessage(
    message,
    agents.value,
    [],
    task.value?.agentId,
    { messages: messages.value, session: null },
  );
}

function embedSubtask(message: ChatMessage) {
  const index = messages.value.findIndex((item) => item.id === message.id);
  if (index < 0) return null;
  const base = subtaskEmbedForMessage(messages.value, index);
  if (!base) return null;
  const preview = props.scenario.embedPreviewById?.[base.id];
  if (!preview) return base;
  return { ...base, preview };
}

function hideSpeaker(message: ChatMessage, index: number) {
  return shouldHideAssistantSpeaker(messages.value[index - 1], message);
}

function messageRunning(message: ChatMessage) {
  return Boolean(message.runId && !message.content.trim() && !message.failureReason);
}

function formatDate(iso: string) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return t("tasks.dateMonthDay", { month: date.getMonth() + 1, day: date.getDate() });
}
</script>

<template>
  <div v-if="scenario.missing || !task" class="tasks-page task-issue-missing task-detail-preview">
    <div class="ds-empty">
      <span class="ds-empty-icon"><ListTodo :size="20" /></span>
      <h3>{{ t("tasks.notFoundTitle") }}</h3>
      <p>{{ t("tasks.notFoundHint") }}</p>
      <UiButton disabled><ArrowLeft :size="14" />{{ t("tasks.back") }}</UiButton>
    </div>
  </div>
  <div v-else class="task-issue task-detail-preview">
    <header class="task-issue-bar">
      <span class="task-issue-crumb" />
      <UiButton variant="ghost" size="icon" class="task-issue-delete" disabled :tip="t('tasks.delete')" :aria-label="t('tasks.delete')">
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
        <div class="task-issue-scroll vs-scroll">
          <div class="task-issue-content">
            <textarea
              class="task-issue-title"
              rows="1"
              readonly
              :value="task.title"
              :aria-label="t('tasks.titleField')"
            />
            <ChatComposer
              v-if="hasDescription"
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
            <section class="task-feed">
              <header>{{ t("tasks.activity") }}</header>
              <div v-if="messages.length" class="task-activity-log">
                <template v-for="(message, messageIndex) in messages" :key="message.id">
                  <UserPrompt
                    v-if="message.role === 'user'"
                    :message="message"
                    :target-label="agent?.name ?? t('inbox.agentFallback')"
                    :target-avatar="agent?.avatar ?? 'M'"
                    :target-value="agent ? `agent:${agent.id}` : ''"
                    :allow-target-change="false"
                    :enable-replay="false"
                    view-only
                    :running="false"
                  />
                  <AssistantMessage
                    v-else
                    :message="message"
                    :speaker="speaker(message)"
                    :running="messageRunning(message)"
                    :allow-continue="false"
                    default-collapse-subagents
                    :hide-speaker="hideSpeaker(message, messageIndex)"
                    :embed-subtask="embedSubtask(message)"
                  />
                </template>
              </div>
              <p v-else class="task-detail-preview-empty">{{ t("tasks.activity") }} · 暂无消息</p>
            </section>
          </div>
        </div>
      </main>
      <aside v-if="sideOpen" class="task-issue-side vs-scroll">
        <section>
          <h2>{{ t("tasks.properties") }}</h2>
          <div class="task-prop-row">
            <span>{{ t("tasks.field.status") }}</span>
            <span class="task-prop-status">
              <i :data-status="task.status" />
              <UiSelect
                class="task-prop-select"
                variant="ghost"
                size="sm"
                disabled
                :aria-label="t('tasks.field.status')"
                :model-value="task.status"
                :options="TASK_COLUMNS.map((column) => ({ value: column.id, label: taskStatusLabel(column.id) }))"
              />
            </span>
          </div>
          <div class="task-prop-row">
            <span>{{ t("tasks.assignee") }}</span>
            <span class="task-prop-person">
              <UiSelect
                class="task-prop-select"
                variant="ghost"
                size="sm"
                disabled
                :aria-label="t('tasks.assignee')"
                :placeholder="t('common.unassigned')"
                :model-value="task.agentId"
                :options="agents.map((item) => ({ value: item.id, label: item.name, avatar: item.avatar }))"
              />
            </span>
          </div>
          <div class="task-prop-row">
            <span>{{ t("tasks.project") }}</span>
            <span>{{ side.projectName || t("tasks.noProject") }}</span>
          </div>
          <div class="task-prop-row">
            <span>{{ t("tasks.priority") }}</span>
            <UiSelect
              class="task-prop-select"
              variant="ghost"
              size="sm"
              disabled
              :aria-label="t('tasks.priority')"
              :model-value="side.priority"
              :options="PRIORITY_OPTIONS.map((option) => ({ value: option.id, label: t(PRIORITY_KEYS[option.id]) }))"
            />
          </div>
          <div class="task-prop-row">
            <span>{{ t("tasks.tags") }}</span>
            <span>{{ side.labels.length ? side.labels.join("、") : t("tasks.addTags") }}</span>
          </div>
        </section>
        <section>
          <h2>{{ t("tasks.checklist") }}</h2>
          <ul v-if="side.checklist.length" class="task-checklist">
            <li v-for="item in side.checklist" :key="item.id">
              <label>
                <input type="checkbox" :checked="item.done" disabled />
                {{ item.text }}
              </label>
            </li>
          </ul>
          <p v-else>{{ t("tasks.addChecklist") }}</p>
        </section>
        <section>
          <h2>{{ t("tasks.subtasks") }}</h2>
          <ul v-if="side.children.length" class="task-runs">
            <li v-for="child in side.children" :key="child.id">
              <button type="button" class="task-run-row" disabled>
                <span>{{ child.title }}</span>
                <span>{{ taskStatusLabel(child.status) }}</span>
              </button>
            </li>
          </ul>
          <p v-else>{{ t("tasks.noSubtasks") }}</p>
        </section>
        <section>
          <h2>Pull Request</h2>
          <input class="task-prop-input" readonly :value="side.prUrl" placeholder="https://…" />
        </section>
        <section>
          <div class="task-side-heading">
            <h2>{{ t("tasks.runLog") }}</h2>
            <UiButton
              v-if="side.runs.length"
              variant="ghost"
              size="small"
              class="task-run-log-btn"
              disabled
            ><ScrollText :size="14" />{{ t("tasks.latestRunLog") }}</UiButton>
          </div>
          <p v-if="!side.runs.length">{{ t("tasks.noRunHistory") }}</p>
          <ul v-else class="task-runs">
            <li v-for="run in side.runs" :key="run.id">
              <button type="button" class="task-run-row" disabled>
                <span class="ds-badge" :data-tone="runStatusTone(run.status)">{{ runStatusLabel(run.status) }}</span>
                <span>{{ relativeTime(run.createdAt) }}</span>
              </button>
            </li>
          </ul>
        </section>
        <section>
          <h2>{{ t("tasks.meta") }}</h2>
          <div class="task-prop-row">
            <span>{{ t("tasks.creator") }}</span>
            <span class="task-prop-person"><i class="task-creator-mark" aria-hidden="true"><User :size="10" /></i>{{ t("tasks.creatorMe") }}</span>
          </div>
          <div class="task-prop-row">
            <span>{{ t("tasks.createdAt") }}</span>
            <span>{{ formatDate(task.createdAt) }}</span>
          </div>
          <div class="task-prop-row">
            <span>{{ t("tasks.updatedAtLabel") }}</span>
            <span>{{ formatDate(task.updatedAt) }}</span>
          </div>
        </section>
      </aside>
    </div>
  </div>
</template>
