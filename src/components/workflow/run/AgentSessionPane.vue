<script setup lang="ts">
import { computed, onMounted, watch } from "vue";
import { t } from "../../../i18n";
import { useAppStore } from "../../../stores/app";
import ChatPanel from "../../ChatPanel.vue";
import ChatMessageItem from "../../ChatMessageItem.vue";

const props = defineProps<{
  /** `wf-node:<nodeRunId>` 或 `wf-run:<runId>` */
  sessionId: string;
}>();

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
    <ChatPanel v-if="known" :session-id="sessionId" embedded :allow-target-change="false" />
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
.agent-session :deep(.chat-panel) {
  height: 100%;
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
