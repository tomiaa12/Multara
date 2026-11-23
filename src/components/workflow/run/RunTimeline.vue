<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { t } from "../../../i18n";
import { isRunTerminal, nodeRunHistory, type RunDetail } from "../../../domain/workflow";
import { topoOrder } from "../../../domain/workflow-graph";
import { nodeDisplayTitle } from "../node-meta";
import { elapsedMs, formatDuration, nodeStatusLabel, timeMs } from "./run-format";

const props = defineProps<{ detail: RunDetail; selectedNodeId?: string | null }>();
const emit = defineEmits<{ "select-node": [nodeId: string] }>();

const now = ref(Date.now());
const timer = window.setInterval(() => {
  if (!isRunTerminal(props.detail.run.status)) now.value = Date.now();
}, 1000);
onBeforeUnmount(() => window.clearInterval(timer));

interface Bar {
  key: string;
  nodeId: string;
  label: string;
  status: string;
  left: number;
  width: number;
  duration: string;
}
interface Row { nodeId: string; title: string; bars: Bar[] }

const range = computed(() => {
  const starts = props.detail.nodeRuns.map((item) => timeMs(item.startedAt ?? item.createdAt)).filter((v): v is number => v != null);
  const start = Math.min(timeMs(props.detail.run.startedAt ?? props.detail.run.createdAt) ?? Infinity, ...starts);
  const ends = props.detail.nodeRuns.map((item) => timeMs(item.completedAt) ?? now.value);
  const end = Math.max(timeMs(props.detail.run.completedAt) ?? now.value, ...ends, start + 1000);
  return { start: Number.isFinite(start) ? start : now.value, end };
});

const rows = computed<Row[]>(() => {
  const { start, end } = range.value;
  const span = Math.max(1, end - start);
  const order = topoOrder(props.detail.definition);
  const result: Row[] = [];
  for (const nodeId of order) {
    const runs = nodeRunHistory(props.detail.nodeRuns, nodeId);
    if (!runs.length) continue;
    const node = props.detail.definition.nodes.find((item) => item.id === nodeId);
    result.push({
      nodeId,
      title: nodeDisplayTitle(node, nodeId, t),
      bars: runs.map((item) => {
        const from = timeMs(item.startedAt ?? item.createdAt) ?? start;
        const to = timeMs(item.completedAt) ?? (item.startedAt ? now.value : from);
        const label = item.iteration > 0 || item.attempt > 1
          ? t("workflow.timeline.round", { iteration: item.iteration + 1, attempt: item.attempt })
          : "";
        return {
          key: item.id,
          nodeId,
          label,
          status: item.status,
          left: ((from - start) / span) * 100,
          width: Math.max(0.6, ((to - from) / span) * 100),
          duration: formatDuration(elapsedMs(item.startedAt, item.completedAt, now.value)),
        };
      }),
    });
  }
  return result;
});

const ticks = computed(() => {
  const { start, end } = range.value;
  return [0, 0.25, 0.5, 0.75, 1].map((ratio) => ({ ratio, label: formatDuration((end - start) * ratio) }));
});
</script>

<template>
  <div class="timeline">
    <div v-if="!rows.length" class="timeline-empty">{{ t("workflow.timeline.empty") }}</div>
    <template v-else>
      <div class="timeline-axis">
        <span class="timeline-name" />
        <div class="timeline-track axis">
          <span v-for="tick in ticks" :key="tick.ratio" class="tick" :style="{ left: `${tick.ratio * 100}%` }">{{ tick.label }}</span>
        </div>
      </div>
      <button
        v-for="row in rows"
        :key="row.nodeId"
        type="button"
        class="timeline-row"
        :class="{ selected: selectedNodeId === row.nodeId }"
        @click="emit('select-node', row.nodeId)"
      >
        <span class="timeline-name" :title="row.nodeId">{{ row.title }}</span>
        <div class="timeline-track">
          <span
            v-for="bar in row.bars"
            :key="bar.key"
            class="bar"
            :data-status="bar.status"
            :style="{ left: `${bar.left}%`, width: `${bar.width}%` }"
            :title="`${nodeStatusLabel(bar.status)} · ${bar.duration}`"
          ><em v-if="bar.label">{{ bar.label }}</em></span>
        </div>
      </button>
    </template>
  </div>
</template>

<style scoped>
.timeline {
  display: grid;
  align-content: start;
  gap: 2px;
  height: 100%;
  overflow: auto;
  padding: var(--spacing-2) var(--spacing-4);
  font-size: var(--font-size-sm);
}
.timeline-empty {
  color: var(--color-text-tertiary);
}
.timeline-axis,
.timeline-row {
  display: grid;
  grid-template-columns: 160px 1fr;
  align-items: center;
  gap: var(--spacing-3);
}
.timeline-axis {
  position: relative;
  z-index: 1;
}
.timeline-row {
  height: 26px;
  padding: 0;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text);
  text-align: left;
  cursor: pointer;
}
.timeline-row:hover,
.timeline-row.selected {
  background: var(--color-surface-hover);
}
.timeline-name {
  overflow: hidden;
  padding-left: var(--spacing-1);
  text-overflow: ellipsis;
  white-space: nowrap;
}
.timeline-track {
  position: relative;
  height: 16px;
  border-radius: var(--radius-xs);
  background: var(--color-bg-tertiary);
}
.timeline-track.axis {
  height: 18px;
  background: transparent;
  color: var(--color-text-tertiary);
  font-size: var(--font-size-xs);
}
.tick {
  position: absolute;
  top: 0;
  line-height: 18px;
  white-space: nowrap;
  transform: translateX(-50%);
}
.tick:first-child {
  transform: none;
}
.tick:last-child {
  transform: translateX(-100%);
}
.bar {
  position: absolute;
  top: 2px;
  bottom: 2px;
  min-width: 3px;
  overflow: hidden;
  border-radius: var(--radius-xs);
  background: var(--color-text-disabled);
  color: var(--color-on-primary);
  font-size: var(--font-size-xs);
  line-height: 12px;
  white-space: nowrap;
}
.bar em {
  padding: 0 4px;
  font-style: normal;
}
.bar[data-status="succeeded"],
.bar[data-status="reused"] { background: var(--color-success); }
.bar[data-status="failed"] { background: var(--color-error); }
.bar[data-status="running"],
.bar[data-status="waiting_children"] { background: var(--color-info); }
.bar[data-status="ready"] { background: var(--color-info-subtle); }
.bar[data-status="waiting_human"] { background: var(--color-warning); }
.bar[data-status="skipped"] { background: var(--color-surface-emphasis); }
</style>
