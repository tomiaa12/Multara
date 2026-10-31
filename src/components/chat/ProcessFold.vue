<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronRight } from "@lucide/vue";
import type { TimelineItem } from "../../domain/types";
import { useAppStore } from "../../stores/app";
import ExploreGroup from "./ExploreGroup.vue";
import FileDiffCard from "./FileDiffCard.vue";
import PermissionCard from "./PermissionCard.vue";
import ShellCard from "./ShellCard.vue";
import SubAgentCard from "./SubAgentCard.vue";
import ModeTransitionRow from "./ModeTransitionRow.vue";
import ThinkingLine from "./ThinkingLine.vue";
import TodoProgressCard from "./TodoProgressCard.vue";
import ToolCallRow from "./ToolCallRow.vue";
import { groupTimeline, workFoldLabel, type TimelineBlock } from "./timeline-display";
import { subAgentTallyLabel } from "./subagent-display";

const props = defineProps<{
  items?: TimelineItem[];
  /** 嵌套层直接传父节点已建好的子块，不再从事件重新分组 */
  blocks?: TimelineBlock[];
  running?: boolean;
  elapsedMs?: number | null;
  runId?: string | null;
  /** 嵌套层级；根过程为 0，嵌套层不再套一层「已工作」折叠 */
  depth?: number;
  /** 回看场景（如收件箱）：子代理一律折叠，只显示摘要 */
  defaultCollapseSubagents?: boolean;
}>();

const store = useAppStore();
const blocks = computed(() => props.blocks ?? groupTimeline(props.items ?? []));
const childDepth = computed(() => (props.depth ?? 0) + 1);
// 折叠态默认不渲染几百上千行的工具明细，展开时再挂载。
const opened = ref(false);
const collapsed = computed(() => !props.running && blocks.value.length > 0 && !props.depth);
const foldLabel = computed(() =>
  [
    workFoldLabel(props.elapsedMs, store.data?.settings.language),
    subAgentTallyLabel(blocks.value, props.running, store.data?.settings.language),
  ].filter(Boolean).join(" · "),
);
const expandedSubagentId = computed(() => {
  if (props.defaultCollapseSubagents || !props.running) return "";
  const last = blocks.value.filter((block) => block.type === "subagent" && !block.item.result).at(-1);
  return last?.type === "subagent" ? last.item.use.callId ?? "" : "";
});

function blockKey(block: TimelineBlock, index: number) {
  if (block.type === "thinking" || block.type === "error" || block.type === "permission" || block.type === "mode") {
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
</script>

<template>
  <details
    v-if="collapsed"
    class="process-fold"
    @toggle="opened = ($event.target as HTMLDetailsElement).open"
  >
    <summary>
      <span>{{ foldLabel }}</span>
      <ChevronRight :size="12" />
    </summary>
    <div v-if="opened" class="process-stream">
      <template v-for="(block, index) in blocks" :key="blockKey(block, index)">
        <ThinkingLine v-if="block.type === 'thinking'" :item="block.item" />
        <p v-else-if="block.type === 'error'" class="process-error">{{ errorText(block) }}</p>
        <TodoProgressCard v-else-if="block.type === 'plan'" :todos="block.todos" />
        <ModeTransitionRow v-else-if="block.type === 'mode'" :item="block.item" />
        <PermissionCard v-else-if="block.type === 'permission'" :item="block.item" :run-id="runId" />
        <ExploreGroup v-else-if="block.type === 'explore'" :items="block.items" />
        <SubAgentCard
          v-else-if="block.type === 'subagent'"
          :item="block.item"
          :depth="childDepth"
          :run-id="runId"
          :default-collapse-subagents="defaultCollapseSubagents"
        />
        <template v-else-if="block.type === 'edit'">
          <FileDiffCard :item="block.item" />
          <div v-if="block.item.children?.length" class="process-nested">
            <ProcessFold :blocks="block.item.children" :depth="childDepth" :run-id="runId" :default-collapse-subagents="defaultCollapseSubagents" />
          </div>
        </template>
        <template v-else-if="block.type === 'shell'">
          <ShellCard :item="block.item" />
          <div v-if="block.item.children?.length" class="process-nested">
            <ProcessFold :blocks="block.item.children" :depth="childDepth" :run-id="runId" :default-collapse-subagents="defaultCollapseSubagents" />
          </div>
        </template>
        <template v-else>
          <ToolCallRow :item="block.item.use" :output="block.item.result?.output" />
          <div v-if="block.item.children?.length" class="process-nested">
            <ProcessFold :blocks="block.item.children" :depth="childDepth" :run-id="runId" :default-collapse-subagents="defaultCollapseSubagents" />
          </div>
        </template>
      </template>
    </div>
  </details>
  <div v-else-if="blocks.length" class="process-stream">
    <template v-for="(block, index) in blocks" :key="blockKey(block, index)">
      <ThinkingLine v-if="block.type === 'thinking'" :item="block.item" :running="running && index === blocks.length - 1" />
      <p v-else-if="block.type === 'error'" class="process-error">{{ errorText(block) }}</p>
      <TodoProgressCard v-else-if="block.type === 'plan'" :todos="block.todos" />
      <ModeTransitionRow v-else-if="block.type === 'mode'" :item="block.item" />
      <PermissionCard v-else-if="block.type === 'permission'" :item="block.item" :run-id="runId" />
      <ExploreGroup
        v-else-if="block.type === 'explore'"
        :items="block.items"
        :running="running"
        :running-tail="running && index === blocks.length - 1"
      />
      <SubAgentCard
        v-else-if="block.type === 'subagent'"
        :item="block.item"
        :depth="childDepth"
        :running="running"
        :default-open="block.item.use.callId === expandedSubagentId"
        :run-id="runId"
        :default-collapse-subagents="defaultCollapseSubagents"
      />
      <template v-else-if="block.type === 'edit'">
        <FileDiffCard :item="block.item" />
        <div v-if="block.item.children?.length" class="process-nested">
          <ProcessFold :blocks="block.item.children" :depth="childDepth" :running="running" :run-id="runId" :default-collapse-subagents="defaultCollapseSubagents" />
        </div>
      </template>
      <template v-else-if="block.type === 'shell'">
        <ShellCard :item="block.item" />
        <div v-if="block.item.children?.length" class="process-nested">
          <ProcessFold :blocks="block.item.children" :depth="childDepth" :running="running" :run-id="runId" :default-collapse-subagents="defaultCollapseSubagents" />
        </div>
      </template>
      <template v-else>
        <ToolCallRow
          :item="block.item.use"
          :running="running && !block.item.result"
          :output="block.item.result?.output"
        />
        <div v-if="block.item.children?.length" class="process-nested">
          <ProcessFold :blocks="block.item.children" :depth="childDepth" :running="running" :run-id="runId" :default-collapse-subagents="defaultCollapseSubagents" />
        </div>
      </template>
    </template>
  </div>
</template>
