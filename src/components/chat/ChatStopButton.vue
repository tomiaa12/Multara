<script setup lang="ts">
import { Square } from "@lucide/vue";
import { t } from "../../i18n";

withDefaults(defineProps<{
  /** 覆盖默认「停止当前任务 (Ctrl+C)」提示 */
  tip?: string;
}>(), {
  tip: "",
});

const emit = defineEmits<{
  click: [event: MouseEvent];
}>();

function onMouseDown(event: MouseEvent) {
  // 阻止抢焦点：footer-on-focus 下若先失焦，页脚会把停止钮藏掉，click 落空
  event.preventDefault();
}

function onClick(event: MouseEvent) {
  emit("click", event);
}
</script>

<template>
  <UiButton
    type="button"
    class="ds-stop-button"
    :tip="tip || t('chat.stopTaskShortcut')"
    :aria-label="tip || t('chat.stopTaskShortcut')"
    @mousedown="onMouseDown"
    @click="onClick"
  >
    <Square :size="14" />
  </UiButton>
</template>
