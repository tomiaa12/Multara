<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Copy, GitFork } from "@lucide/vue";
import { formatAbsoluteTime, formatRelativeTime } from "./message-time";
import { useSharedNow } from "./shared-clock";
import { useAppStore } from "../../stores/app";
import { t } from "../../i18n";

const props = withDefaults(defineProps<{
  content: string;
  createdAt?: string | null;
  elapsedMs?: number | null;
  sessionId?: string | null;
  messageId?: string | null;
  /** 仅右下角气泡 / 工作台聊天展示从此消息 fork */
  allowFork?: boolean;
}>(), {
  allowFork: false,
});
const store = useAppStore();
const copied = ref(false);
const forking = ref(false);
const now = useSharedNow();

const relativeTime = computed(() =>
  props.createdAt ? formatRelativeTime(props.createdAt, now.value) : "",
);
const absoluteTime = computed(() =>
  props.createdAt ? formatAbsoluteTime(props.createdAt) : "",
);
const canFork = computed(() =>
  Boolean(props.allowFork && props.sessionId && props.messageId),
);

async function copyMessage() {
  await navigator.clipboard.writeText(props.content);
  copied.value = true;
  window.setTimeout(() => (copied.value = false), 1200);
}

async function forkFromHere() {
  if (!props.sessionId || !props.messageId || forking.value) return;
  forking.value = true;
  try {
    await store.forkSession(props.sessionId, props.messageId);
  } finally {
    forking.value = false;
  }
}
</script>

<template>
  <footer class="assistant-message-footer">
    <span v-if="elapsedMs != null" class="message-elapsed">{{ t('chat.elapsedSeconds', { seconds: (elapsedMs / 1000).toFixed(1) }) }}</span>
    <UiButton
      class="message-copy"
      variant="ghost"
      size="icon"
      :tip="copied ? t('chat.copied') : t('common.copy')"
      :aria-label="copied ? t('chat.copied') : t('chat.copyMessage')"
      @click="copyMessage"
    >
      <Check v-if="copied" :size="13" /><Copy v-else :size="13" />
    </UiButton>
    <UiButton
      v-if="canFork"
      class="message-copy"
      variant="ghost"
      size="icon"
      :tip="t('chat.forkFromMessage')"
      :aria-label="t('chat.forkFromMessage')"
      :disabled="forking"
      @click="forkFromHere"
    >
      <GitFork :size="13" />
    </UiButton>
    <UiTooltip v-if="relativeTime" :content="absoluteTime">
      <span class="message-time">{{ relativeTime }}</span>
    </UiTooltip>
  </footer>
</template>
