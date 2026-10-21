<script setup lang="ts">
import { computed } from "vue";
import { directoryColumnTemplate, type DirectoryColumn } from "./directory-table";

const props = defineProps<{
  columns: DirectoryColumn[];
}>();

defineEmits<{
  sort: [key: string];
}>();

const columnStyle = computed(() => ({
  "--ui-directory-columns": directoryColumnTemplate(props.columns),
}));
</script>

<template>
  <div class="agent-directory ui-directory-table" :style="columnStyle">
    <div class="agent-directory-head">
      <template v-for="column in columns" :key="column.key">
        <slot :name="`head-${column.key}`" :column="column">
          <button
            v-if="column.sortable"
            type="button"
            @click="$emit('sort', column.key)"
          >{{ column.label }}</button>
          <span v-else :class="{ 'ui-directory-head-actions': column.actions }">{{ column.label }}</span>
        </slot>
      </template>
    </div>
    <slot />
  </div>
</template>
