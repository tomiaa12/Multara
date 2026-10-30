<script setup lang="ts">
import { computed } from "vue";
import { formatAbsoluteTime, formatRelativeTime } from "./relative-time";
import UiTooltip from "./UiTooltip.vue";

const props = withDefaults(defineProps<{
  /** ISO 字符串、毫秒时间戳或 Date */
  value?: string | number | Date | null;
  /** value 无效或为空时的占位 */
  empty?: string;
}>(), {
  value: null,
  empty: "—",
});

const text = computed(() => formatRelativeTime(props.value));
const title = computed(() => formatAbsoluteTime(props.value));
</script>

<template>
  <UiTooltip v-if="text" :content="title" :disabled="!title">
    <time
      class="ui-relative-time"
      :datetime="typeof value === 'string' ? value : undefined"
    >{{ text }}</time>
  </UiTooltip>
  <span v-else class="ui-relative-time is-empty">{{ empty }}</span>
</template>
