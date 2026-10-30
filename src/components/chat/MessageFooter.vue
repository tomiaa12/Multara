<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Copy, ThumbsDown, ThumbsUp } from "@lucide/vue";
import { formatAbsoluteTime, formatRelativeTime } from "./message-time";
import { useSharedNow } from "./shared-clock";
import { useAppStore } from "../../stores/app";
import { t } from "../../i18n";

const props = defineProps<{
  content: string;
  createdAt?: string | null;
  elapsedMs?: number | null;
  messageId?: string | null;
  feedback?: "up" | "down" | null;
}>();
const store = useAppStore();
const copied = ref(false);
const now = useSharedNow();

const relativeTime = computed(() =>
  props.createdAt ? formatRelativeTime(props.createdAt, now.value) : "",
);
const absoluteTime = computed(() =>
  props.createdAt ? formatAbsoluteTime(props.createdAt) : "",
);

async function copyMessage() {
  await navigator.clipboard.writeText(props.content);
  copied.value = true;
  window.setTimeout(() => (copied.value = false), 1200);
}
</script>

<template>
  <footer class="assistant-message-footer">
    <span v-if="elapsedMs != null" class="message-elapsed">{{ t('chat.elapsedSeconds', { seconds: (elapsedMs / 1000).toFixed(1) }) }}</span>
    <UiButton
      v-if="messageId"
      class="message-copy"
      variant="ghost"
      size="icon"
      :tip="t('chat.feedbackHelpful')"
      :aria-pressed="feedback === 'up'"
      @click="store.setMessageFeedback(messageId, feedback === 'up' ? null : 'up')"
    >
      <ThumbsUp :size="13" />
    </UiButton>
    <UiButton
      v-if="messageId"
      class="message-copy"
      variant="ghost"
      size="icon"
      :tip="t('chat.feedbackUnhelpful')"
      :aria-pressed="feedback === 'down'"
      @click="store.setMessageFeedback(messageId, feedback === 'down' ? null : 'down')"
    >
      <ThumbsDown :size="13" />
    </UiButton>
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
    <UiTooltip v-if="relativeTime" :content="absoluteTime">
      <span class="message-time">{{ relativeTime }}</span>
    </UiTooltip>
  </footer>
</template>
