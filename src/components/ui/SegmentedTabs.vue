<script setup lang="ts">
export type SegmentedTab = {
  id: string;
  label: string;
  count?: number;
  /** 一直显示数字。其余项只在选中时显示。 */
  pinCount?: boolean;
};

const props = defineProps<{
  modelValue: string;
  items: SegmentedTab[];
  label: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
}>();

function showCount(item: SegmentedTab) {
  if (item.count == null) return false;
  return Boolean(item.pinCount) || props.modelValue === item.id;
}
</script>

<template>
  <nav class="ds-segmented" :aria-label="label">
    <button
      v-for="item in items"
      :key="item.id"
      type="button"
      role="tab"
      :aria-selected="modelValue === item.id"
      :class="{ active: modelValue === item.id }"
      @click="emit('update:modelValue', item.id)"
    >
      <slot name="leading" :item="item" />
      {{ item.label }}<em v-if="showCount(item)">{{ item.count }}</em>
    </button>
  </nav>
</template>
