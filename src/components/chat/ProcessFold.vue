<script setup lang="ts">
import { computed } from "vue";
import { ChevronRight } from "@lucide/vue";
import type { TimelineItem } from "../../domain/types";
import { useAppStore } from "../../stores/app";
import ExploreGroup from "./ExploreGroup.vue";
import FileDiffCard from "./FileDiffCard.vue";
import PermissionCard from "./PermissionCard.vue";
import PlanCard from "./PlanCard.vue";
import ShellCard from "./ShellCard.vue";
import ThinkingLine from "./ThinkingLine.vue";
import ToolCallRow from "./ToolCallRow.vue";
import { groupTimeline, workFoldLabel, type TimelineBlock } from "./timeline-display";

const props = defineProps<{
  items: TimelineItem[];
  running?: boolean;
  elapsedMs?: number | null;
}>();

const store = useAppStore();
const blocks = computed(() => groupTimeline(props.items));
const collapsed = computed(() => !props.running && blocks.value.length > 0);
const foldLabel = computed(() =>
  workFoldLabel(props.elapsedMs, store.data?.settings.language),
);

function blockKey(block: TimelineBlock, index: number) {
  if (block.type === "thinking" || block.type === "error" || block.type === "permission") {
    return `${block.type}-${block.item.seq}`;
  }
  if (block.type === "plan") return `plan-${index}`;
  if (block.type === "explore") {
    const first = block.items[0];
    if (!first) return `explore-${index}`;
    if (first.kind === "thinking") return `explore-think-${first.item.seq}`;
    return `explore-${first.item.use.callId || first.item.use.seq || index}`;
  }
  return `${block.type}-${block.item.use.callId || block.item.use.seq || index}`;
}

function errorText(block: TimelineBlock) {
  if (block.type !== "error") return "";
  return (block.item.content || block.item.output || "").trim() || "工具出错";
}

function nestedItems(children: TimelineBlock[]) {
  return children.flatMap((block) => {
    if (block.type === "thinking" || block.type === "error" || block.type === "permission") return [block.item];
    if (block.type === "explore") {
      return block.items.flatMap((entry) => {
        if (entry.kind === "thinking") return [entry.item];
        return [entry.item.use, ...(entry.item.result ? [entry.item.result] : [])];
      });
    }
    if (block.type === "plan") return [];
    return [block.item.use, ...(block.item.result ? [block.item.result] : [])];
  });
}
</script>

<template>
  <details v-if="collapsed" class="process-fold">
    <summary>
      <span>{{ foldLabel }}</span>
      <ChevronRight :size="12" />
    </summary>
    <div class="process-stream">
      <template v-for="(block, index) in blocks" :key="blockKey(block, index)">
        <ThinkingLine v-if="block.type === 'thinking'" :item="block.item" />
        <p v-else-if="block.type === 'error'" class="process-error">{{ errorText(block) }}</p>
        <PlanCard v-else-if="block.type === 'plan'" :todos="block.todos" />
        <PermissionCard v-else-if="block.type === 'permission'" :item="block.item" />
        <ExploreGroup v-else-if="block.type === 'explore'" :items="block.items" />
        <template v-else-if="block.type === 'edit'">
          <FileDiffCard :item="block.item" />
          <div v-if="block.item.children?.length" class="process-nested">
            <ProcessFold :items="nestedItems(block.item.children)" />
          </div>
        </template>
        <template v-else-if="block.type === 'shell'">
          <ShellCard :item="block.item" />
          <div v-if="block.item.children?.length" class="process-nested">
            <ProcessFold :items="nestedItems(block.item.children)" />
          </div>
        </template>
        <template v-else>
          <ToolCallRow :item="block.item.use" :output="block.item.result?.output" />
          <div v-if="block.item.children?.length" class="process-nested">
            <ProcessFold :items="nestedItems(block.item.children)" />
          </div>
        </template>
      </template>
    </div>
  </details>
  <div v-else-if="blocks.length" class="process-stream">
    <template v-for="(block, index) in blocks" :key="blockKey(block, index)">
      <ThinkingLine v-if="block.type === 'thinking'" :item="block.item" :running="running && index === blocks.length - 1" />
      <p v-else-if="block.type === 'error'" class="process-error">{{ errorText(block) }}</p>
      <PlanCard v-else-if="block.type === 'plan'" :todos="block.todos" />
      <PermissionCard v-else-if="block.type === 'permission'" :item="block.item" />
      <ExploreGroup
        v-else-if="block.type === 'explore'"
        :items="block.items"
        :running="running"
        :running-tail="running && index === blocks.length - 1"
      />
      <template v-else-if="block.type === 'edit'">
        <FileDiffCard :item="block.item" />
        <div v-if="block.item.children?.length" class="process-nested">
          <ProcessFold :items="nestedItems(block.item.children)" :running="running" />
        </div>
      </template>
      <template v-else-if="block.type === 'shell'">
        <ShellCard :item="block.item" />
        <div v-if="block.item.children?.length" class="process-nested">
          <ProcessFold :items="nestedItems(block.item.children)" :running="running" />
        </div>
      </template>
      <template v-else>
        <ToolCallRow
          :item="block.item.use"
          :running="running && !block.item.result"
          :output="block.item.result?.output"
        />
        <div v-if="block.item.children?.length" class="process-nested">
          <ProcessFold :items="nestedItems(block.item.children)" :running="running" />
        </div>
      </template>
    </template>
  </div>
</template>
