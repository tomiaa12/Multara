<script setup lang="ts">
import { computed } from "vue";
import { t } from "../../i18n";
import { useRemoteStore } from "../../stores/remote";

withDefaults(defineProps<{
  /** 覆盖默认「停止当前任务 (Ctrl+C)」提示 */
  tip?: string;
}>(), {
  tip: "",
});

const emit = defineEmits<{
  click: [event: MouseEvent];
}>();

const remote = useRemoteStore();
const lockTip = computed(() => remote.lockTip("taskCreate"));

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
    :disabled="Boolean(lockTip)"
    :tip="lockTip || tip || t('chat.stopTaskShortcut')"
    :aria-label="lockTip || tip || t('chat.stopTaskShortcut')"
    @mousedown="onMouseDown"
    @click="onClick"
  >
    <!-- 纯色方块：避免 Lucide Square 的 stroke+fill 在小圆钮里看起来偏心 -->
    <span class="ds-stop-icon" aria-hidden="true" />
  </UiButton>
</template>
