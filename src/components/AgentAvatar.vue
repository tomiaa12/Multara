<script setup lang="ts">
import { computed } from "vue";

/**
 * 全局统一的智能体头像（对应 design-system 的 `.agent-avatar`）。
 * value 可以是 emoji、data:image、http(s)/blob 链接或相对路径（如内置 logo.svg）；
 * 为空时用名字首字兜底。size 传 null 时不写内联尺寸，交给外层 CSS 决定（如下拉选项）。
 */
const props = withDefaults(defineProps<{
  value?: string | null;
  name?: string;
  size?: number | null;
}>(), {
  value: "",
  name: "",
  size: 42,
});

const isImage = computed(() => /^(data:image\/|https?:|blob:|\.{0,2}\/)/.test(props.value ?? ""));
const fallback = computed(() => props.value || props.name.trim().slice(0, 1).toUpperCase() || "🤖");
const sizeStyle = computed(() =>
  props.size == null
    ? undefined
    : { width: `${props.size}px`, height: `${props.size}px`, fontSize: `${Math.max(12, props.size * 0.46)}px` },
);
</script>

<template>
  <UiTooltip :content="name || ''" :disabled="!name">
    <span class="agent-avatar" :style="sizeStyle">
      <img v-if="isImage" draggable="false" referrerpolicy="no-referrer" :src="value ?? ''" :alt="name ? `${name} 的头像` : '智能体头像'" />
      <span v-else class="agent-avatar-glyph">{{ fallback }}</span>
    </span>
  </UiTooltip>
</template>
