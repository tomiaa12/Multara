<script setup lang="ts">
import { ChevronRight } from "@lucide/vue";
import type { OutlineItem } from "../../editor/outline";

defineProps<{
  parts: string[];
  symbol: OutlineItem | null;
}>();

const emit = defineEmits<{
  reveal: [line: number];
}>();
</script>

<template>
  <nav v-if="parts.length" class="editor-breadcrumbs" aria-label="面包屑">
    <template v-for="(part, index) in parts" :key="`${index}-${part}`">
      <ChevronRight v-if="index" :size="10" />
      <span>{{ part }}</span>
    </template>
    <template v-if="symbol">
      <ChevronRight :size="10" />
      <button type="button" class="editor-breadcrumb-symbol" @click="emit('reveal', symbol.line)">
        {{ symbol.label }}
      </button>
    </template>
  </nav>
</template>
