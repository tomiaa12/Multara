<script lang="ts">
export default { name: "ExplorerTreeList" };
</script>

<script setup lang="ts">
import type { ExplorerRow, ExplorerRowNode } from "./explorer/explorer-rows";

defineProps<{
  nodes: ExplorerRowNode[];
}>();

defineSlots<{
  row(props: { entry: ExplorerRow; sticky: boolean }): unknown;
  edit(props: { entry: ExplorerRow }): unknown;
}>();
</script>

<template>
  <template v-for="node in nodes" :key="node.entry.path">
    <div v-if="node.expanded" class="tree-branch">
      <slot name="row" :entry="node.entry" :sticky="true" />
      <slot name="edit" :entry="node.entry" />
      <ExplorerTreeList v-if="node.children.length" :nodes="node.children">
        <template #row="props">
          <slot name="row" v-bind="props" />
        </template>
        <template #edit="props">
          <slot name="edit" v-bind="props" />
        </template>
      </ExplorerTreeList>
    </div>
    <template v-else>
      <slot name="row" :entry="node.entry" :sticky="false" />
      <slot name="edit" :entry="node.entry" />
    </template>
  </template>
</template>
