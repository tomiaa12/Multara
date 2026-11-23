<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { useRouter } from "vue-router";
import { FolderOpen, MessageSquare, Square } from "@lucide/vue";
import { t } from "../../../i18n";
import * as backend from "../../../services/backend";
import { confirmDialog } from "../../ui/confirm-dialog";
import { isRunTerminal, type RunDetail } from "../../../domain/workflow";
import { elapsedMs, formatDuration, runStatusLabel, runStatusTone, summarizeInputs, triggerSourceLabel } from "./run-format";

const props = defineProps<{
  detail: RunDetail;
  overviewOpen?: boolean;
  hideOverview?: boolean;
}>();
const emit = defineEmits<{ "toggle-overview": [] }>();

const router = useRouter();
const now = ref(Date.now());
const error = ref("");
const timer = window.setInterval(() => (now.value = Date.now()), 1000);
onBeforeUnmount(() => window.clearInterval(timer));

const run = computed(() => props.detail.run);
const live = computed(() => !isRunTerminal(run.value.status));
const duration = computed(() => formatDuration(elapsedMs(run.value.startedAt ?? run.value.createdAt, run.value.completedAt, now.value)));
const usage = computed(() => {
  const items = Object.values(props.detail.usage ?? {});
  return {
    agentRuns: items.reduce((sum, item) => sum + item.agentRuns, 0),
    elapsedMs: items.reduce((sum, item) => sum + item.elapsedMs, 0),
  };
});

async function cancel() {
  if (!(await confirmDialog(t("workflow.run.cancelConfirm"), { danger: true, primary: t("workflow.run.cancel") }))) return;
  error.value = "";
  try {
    await backend.cancelWorkflowRun(run.value.id);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function openWorkbench(projectId: string) {
  void router.push(`/projects/${projectId}`);
}
</script>

<template>
  <header class="run-header">
    <div class="run-title">
      <strong>{{ detail.workflow.name }}</strong>
      <span class="ds-badge" :data-tone="runStatusTone(run.status)">{{ runStatusLabel(run.status) }}</span>
      <span v-if="run.version === 0" class="ds-badge" data-tone="purple">{{ t("workflow.run.draftRun") }}</span>
      <span v-else class="ds-badge">v{{ run.version }}</span>
    </div>
    <div class="run-meta">
      <span>{{ triggerSourceLabel(run.triggerSource) }}</span>
      <span>{{ t("workflow.run.duration", { time: duration }) }}</span>
      <span v-if="usage.agentRuns">{{ t("workflow.run.usage", { runs: usage.agentRuns, time: formatDuration(usage.elapsedMs) }) }}</span>
      <span class="run-inputs">{{ summarizeInputs(run.inputs, 100) }}</span>
      <span v-if="run.error" class="run-error">{{ run.errorCode ? `${run.errorCode}: ` : "" }}{{ run.error }}</span>
      <span v-if="error" class="run-error">{{ error }}</span>
    </div>
    <div class="run-actions">
      <UiButton v-for="space in detail.workspaces" :key="space.slotKey" size="small" :tip="space.path" @click="openWorkbench(space.projectId)">
        <FolderOpen :size="14" /> {{ t("workflow.run.openWorkbench", { slot: space.slotKey }) }}
      </UiButton>
      <UiButton v-if="!hideOverview" size="small" :class="{ active: overviewOpen }" @click="emit('toggle-overview')"><MessageSquare :size="14" /> {{ t("workflow.run.overview") }}</UiButton>
      <UiButton v-if="live" size="small" variant="danger" @click="cancel"><Square :size="13" /> {{ t("workflow.run.cancel") }}</UiButton>
      <slot name="actions" />
    </div>
  </header>
</template>

<style scoped>
.run-header {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--spacing-2) var(--spacing-4);
  padding: var(--spacing-2) var(--spacing-4);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg);
}
.run-title,
.run-actions,
.run-meta {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  min-width: 0;
}
.run-meta {
  flex: 1;
  flex-wrap: wrap;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}
.run-inputs {
  overflow: hidden;
  max-width: 420px;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--color-text-tertiary);
}
.run-error {
  color: var(--color-error);
}
</style>
