<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Copy } from "@lucide/vue";
import { formatAbsoluteTime, formatRelativeTime } from "./message-time";
import { useSharedNow } from "./shared-clock";
import { t } from "../../i18n";

const props = defineProps<{
  content: string;
  createdAt?: string | null;
  elapsedMs?: number | null;
}>();
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
