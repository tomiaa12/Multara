<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { useAppStore } from "../../stores/app";
import { t } from "../../i18n";
import AgentLiveStatus from "../AgentLiveStatus.vue";
import type { AssignmentSubtaskRef } from "./message-speaker";

const props = defineProps<{
  subtask: AssignmentSubtaskRef;
}>();

const store = useAppStore();
const preview = computed(() => props.subtask.preview ?? null);
const task = computed(() => {
  if (preview.value) return null;
  return store.data?.tasks.find((item) => item.id === props.subtask.id) ?? null;
});
const agent = computed(() => {
  if (preview.value?.agent) return preview.value.agent;
  const id = task.value?.agentId;
  if (!id) return null;
  return store.data?.agents.find((item) => item.id === id) ?? null;
});
const sessionId = computed(() => (preview.value ? "" : task.value?.sessionId ?? ""));
const fallback = computed(() => {
  const item = agent.value;
  if (!item) return [];
  return [{ key: item.id, name: item.name, image: item.avatar }];
});
/** 只展示「处理中」的过程卡片；交付后由被指派方 @ 回来的消息承载回答，这里不再重复。 */
const active = computed(() => {
  if (preview.value) return Boolean(preview.value.pending);
  const status = task.value?.status;
  return !status || status === "backlog" || status === "todo" || status === "in_progress";
});
const statusLabel = computed(() => {
  const status = preview.value?.status ?? task.value?.status;
  if (!status) return "";
  return t(`tasks.status.${status}`);
});
</script>

<template>
  <section v-if="active" class="subtask-reply-embed">
    <header class="subtask-reply-embed-head">
      <AgentLiveStatus
        :session-ids="sessionId ? [sessionId] : []"
        :fallback="fallback"
        show-name
        :size="18"
        :dialog-title="subtask.title"
      />
      <span v-if="statusLabel" class="subtask-reply-status">{{ statusLabel }}</span>
    </header>
    <RouterLink class="subtask-reply-title" :to="`/tasks/${subtask.id}`">
      {{ subtask.title }}
    </RouterLink>
    <p class="subtask-reply-empty">{{ t("chat.subtaskReplyPending") }}</p>
  </section>
</template>
