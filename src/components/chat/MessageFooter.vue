<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { Check, Copy, ThumbsDown, ThumbsUp } from "@lucide/vue";
import { formatAbsoluteTime, formatRelativeTime } from "./message-time";
import { useAppStore } from "../../stores/app";

const props = defineProps<{
  content: string;
  createdAt?: string | null;
  elapsedMs?: number | null;
  messageId?: string | null;
  feedback?: "up" | "down" | null;
}>();
const store = useAppStore();
const copied = ref(false);
const now = ref(Date.now());
let timer = 0;

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

onMounted(() => {
  timer = window.setInterval(() => {
    now.value = Date.now();
  }, 30_000);
});
onBeforeUnmount(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <footer class="assistant-message-footer">
    <span v-if="elapsedMs != null" class="message-elapsed">{{ (elapsedMs / 1000).toFixed(1) }} 秒</span>
    <UiButton
      v-if="messageId"
      class="message-copy"
      variant="ghost"
      size="icon"
      title="有帮助"
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
      title="没帮助"
      :aria-pressed="feedback === 'down'"
      @click="store.setMessageFeedback(messageId, feedback === 'down' ? null : 'down')"
    >
      <ThumbsDown :size="13" />
    </UiButton>
    <UiButton
      class="message-copy"
      variant="ghost"
      size="icon"
      :title="copied ? '已复制' : '复制'"
      :aria-label="copied ? '已复制' : '复制消息'"
      @click="copyMessage"
    >
      <Check v-if="copied" :size="13" /><Copy v-else :size="13" />
    </UiButton>
    <span
      v-if="relativeTime"
      class="message-time"
      :title="absoluteTime"
    >{{ relativeTime }}</span>
  </footer>
</template>
