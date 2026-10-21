<script setup lang="ts">
import { computed } from "vue";
import { AlertTriangle, ChevronRight } from "@lucide/vue";
import type { ChatMessage } from "../../domain/types";

const props = defineProps<{
  message: ChatMessage;
  duplicateCount?: number;
  allowContinue?: boolean;
}>();
const emit = defineEmits<{ continue: [] }>();
const failureText = computed(() => {
  const summary = props.message.content || "这次运行没有成功完成。";
  const count = props.duplicateCount ?? 1;
  return count > 1 ? `${summary}（${count} 个执行者）` : summary;
});
</script>

<template>
  <div class="failure-bubble">
    <div class="failure-summary">
      <AlertTriangle :size="15" />
      <span>{{ failureText }}</span>
    </div>
    <details v-if="message.failureReason" class="failure-technical">
      <summary><ChevronRight :size="13" />技术详情</summary>
      <pre class="failure-details vs-scroll">{{ message.failureReason }}</pre>
    </details>
    <UiButton
      v-if="allowContinue && message.failureReason"
      size="small"
      type="button"
      @click="emit('continue')"
    >继续上次任务</UiButton>
  </div>
</template>
