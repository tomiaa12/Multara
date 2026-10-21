<script setup lang="ts">
import { computed } from "vue";
import { countBadgeDigitsAttr, formatCountBadge, resolveCountBadgeLabel } from "../../ui/count-badge";

const props = withDefaults(defineProps<{
  /** 原始计数；大于 99 显示 99+ */
  count?: number | null;
  /** 已有文案或数字；与 count 同时存在时优先 label */
  label?: string | number | null;
  tone?: "default" | "primary";
}>(), {
  count: null,
  label: null,
  tone: "default",
});

const text = computed(() => {
  if (props.label != null && props.label !== "") {
    return resolveCountBadgeLabel(props.label);
  }
  if (props.count == null) return "";
  return formatCountBadge(props.count);
});

const digits = computed(() => countBadgeDigitsAttr(text.value));
</script>

<template>
  <span
    v-if="text"
    class="ds-count"
    :data-tone="tone === 'primary' ? 'primary' : undefined"
    :data-digits="digits"
  >{{ text }}</span>
</template>
