<script setup lang="ts">
/** 开关：样式沿用 `.ds-switch`。`busy` 时禁用，防止保存未返回前重复切换。 */
defineProps<{
  disabled?: boolean;
  busy?: boolean;
  label?: string;
}>();

const model = defineModel<boolean>({ default: false });
</script>

<template>
  <input
    v-model="model"
    class="ds-switch"
    type="checkbox"
    role="switch"
    :aria-checked="model"
    :aria-label="label"
    :aria-busy="busy || undefined"
    :disabled="disabled || busy"
  />
</template>
