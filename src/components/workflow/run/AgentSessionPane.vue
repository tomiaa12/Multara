<script setup lang="ts">
import { computed, onMounted, watch } from "vue";
import { t } from "../../../i18n";
import { useAppStore } from "../../../stores/app";
import ChatPanel from "../../ChatPanel.vue";
import ChatMessageItem from "../../ChatMessageItem.vue";

const props = withDefaults(defineProps<{
  /** `wf-node:<nodeRunId>` 或 `wf-run:<runId>` */
  sessionId: string;
  /** 打开输入框，提问卡片会出现在输入框上方 */
  reply?: boolean;
  /** 末轮撑满一屏。总览侧栏只有几条系统消息时关掉，避免上下大片空白 */
  stickyUserTurn?: boolean;
}>(), {
  stickyUserTurn: true,
});

const store = useAppStore();
// ChatPanel 只渲染 store.data.sessions 里存在的会话；拿不到会话时退化成只读消息列表。
const known = computed(() => Boolean(store.data?.sessions.some((item) => item.id === props.sessionId)));
const messages = computed(() => store.messages[props.sessionId] ?? []);

async function load() {
  try {
    // wf-* 会话不在聊天列表里：先把会话本身和消息拉进 store，ChatPanel 才找得到
    await store.ensureSessionLoaded(props.sessionId);
  } catch {
    // 预览模式没有会话
  }
}

onMounted(load);
watch(() => props.sessionId, load);
</script>

<template>
  <div class="agent-session">
    <ChatPanel v-if="known" class="workflow-chat" :session-id="sessionId" embedded :hide-composer="!reply" :sticky-user-turn="stickyUserTurn" lock-sent-prompts :allow-target-change="false">
      <template #above-composer><slot name="above-composer" /></template>
    </ChatPanel>
    <div v-else class="session-fallback">
      <p v-if="!messages.length" class="session-empty">{{ t("workflow.drawer.sessionEmpty") }}</p>
      <ChatMessageItem v-for="message in messages" :key="message.id" :message="message" view-only :enable-replay="false" />
    </div>
  </div>
</template>

<style scoped>
.agent-session {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  overflow: hidden;
}
.agent-session :deep(.chat-panel),
.agent-session :deep(.shared-chat-panel),
.agent-session :deep(.chat-surface) {
  height: 100%;
  min-height: 0;
}
.agent-session :deep(.chat-surface-header) {
  min-height: 40px;
  padding: var(--spacing-2) var(--spacing-3);
}
.agent-session :deep(.message-list) {
  padding: var(--spacing-3);
}
/* 收件箱会按整页会话的左右留白收窄输入框；这里消息区只有 spacing-3，输入框跟回答同宽 */
.agent-session :deep(.chat-surface > .chat-editing-dock),
.agent-session :deep(.chat-surface > .chat-composer) {
  box-sizing: border-box;
  width: calc(100% - 2 * var(--spacing-3));
  max-width: none;
  margin: 0 var(--spacing-3) var(--spacing-3);
}
.agent-session :deep(.shared-chat-panel.is-sticky-user-turn .message-exchange:last-child) {
  min-height: 0;
}
.session-fallback {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-2);
  overflow: auto;
  padding: var(--spacing-2);
}
.session-empty {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: var(--font-size-sm);
}
</style>
