<script setup lang="ts">
import { onBeforeUnmount, watch } from "vue";
import Codicon from "./Codicon.vue";

const NOTICE_DISMISS_MS = 6000;

const props = defineProps<{
  message: string;
  tone?: "error" | "warning";
  sticky?: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

let timer = 0;

function arm() {
  window.clearTimeout(timer);
  if (!props.message || props.sticky) return;
  timer = window.setTimeout(() => emit("close"), NOTICE_DISMISS_MS);
}

watch(() => props.message, arm, { immediate: true });
onBeforeUnmount(() => window.clearTimeout(timer));
</script>

<template>
  <div class="ui-notice" :data-tone="tone ?? 'warning'" role="status">
    <span>{{ message }}</span>
    <UiTooltip content="关闭">
      <button type="button" @click="emit('close')">
        <Codicon name="close" :size="14" />
      </button>
    </UiTooltip>
  </div>
</template>
