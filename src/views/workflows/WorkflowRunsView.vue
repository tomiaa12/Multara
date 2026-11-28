<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { onAppEvent } from "../../dev/dev-events";
import { useRemoteStore } from "../../stores/remote";
import { GitBranch, Trash2 } from "@lucide/vue";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import { confirmDialog } from "../../components/ui/confirm-dialog";
import SegmentedTabs from "../../components/ui/SegmentedTabs.vue";
import { isRunTerminal, type Workflow, type WorkflowRun } from "../../domain/workflow";
import {
  elapsedMs,
  formatDuration,
  runStatusLabel,
  runStatusTone,
  summarizeInputs,
  triggerSourceLabel,
} from "../../components/workflow/run/run-format";
import { useWorkflowsStore } from "../../stores/workflows";

const route = useRoute();
const router = useRouter();
const store = useWorkflowsStore();
const remote = useRemoteStore();

const workflowId = computed(() => String(route.params.workflowId ?? ""));
const workflow = ref<Workflow | null>(null);
const runs = ref<WorkflowRun[]>([]);
const status = ref("all");
const loading = ref(true);
const error = ref("");
const now = ref(Date.now());
let unlisten: UnlistenFn | null = null;
let tick = 0;
let disposed = false;

const filters = computed(() => [
  { id: "all", label: t("workflow.runs.filterAll") },
  { id: "running", label: t("workflow.runs.filterRunning") },
  { id: "waiting", label: t("workflow.runs.filterWaiting") },
  { id: "succeeded", label: t("workflow.runs.filterSucceeded") },
  { id: "failed", label: t("workflow.runs.filterFailed") },
  { id: "cancelled", label: t("workflow.runs.filterCancelled") },
]);

async function load() {
  if (!workflowId.value) return;
  try {
    const [runList, detail] = await Promise.all([
      backend.listWorkflowRuns({ workflowId: workflowId.value, status: status.value === "all" ? null : status.value }),
      workflow.value ? Promise.resolve(null) : backend.getWorkflow(workflowId.value).catch(() => null),
    ]);
    runs.value = runList;
    if (detail) workflow.value = detail.workflow;
    error.value = "";
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading.value = false;
  }
}

function open(run: WorkflowRun) {
  void router.push(`/workflows/${run.workflowId}/runs/${run.id}`);
}

async function remove(run: WorkflowRun) {
  if (!isRunTerminal(run.status)) return;
  if (!(await confirmDialog(t("workflow.runs.deleteConfirm"), { danger: true, detail: t("workflow.runs.deleteDetail") }))) return;
  const removeWorktrees = await confirmDialog(t("workflow.runs.removeWorktrees"), {
    primary: t("workflow.runs.removeWorktreesYes"),
    cancel: t("workflow.runs.removeWorktreesNo"),
  });
  try {
    await backend.deleteWorkflowRun(run.id, removeWorktrees);
    await load();
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

onMounted(async () => {
  void store.subscribe();
  void load();
  tick = window.setInterval(() => (now.value = Date.now()), 1000);
  try {
    const handler = await onAppEvent("workflow-run-changed", () => void load());
    if (disposed) handler();
    else unlisten = handler;
  } catch {
    // 浏览器预览没有事件总线
  }
});
onBeforeUnmount(() => {
  disposed = true;
  store.unsubscribe();
  window.clearInterval(tick);
  unlisten?.();
});
watch([workflowId, status], () => void load());
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>{{ workflow?.name ?? t("route.workflowRuns") }}</h1>
        <p>{{ t("workflow.runs.subtitle") }}</p>
      </div>
    </div>

    <SegmentedTabs v-model="status" :items="filters" :label="t('workflow.runs.filter')" />

    <p v-if="error" class="page-error">{{ error }}</p>
    <section v-else-if="!loading && !runs.length" class="panel placeholder">
      <span class="ds-empty-icon"><GitBranch :size="18" /></span>
      <h2>{{ t("workflow.runs.empty") }}</h2>
      <p>{{ t("workflow.runs.emptyHint") }}</p>
    </section>
    <div v-else class="card-list">
      <article v-for="run in runs" :key="run.id" class="list-card run-row" tabindex="0" @click="open(run)" @keydown.enter="open(run)">
        <div class="grow">
          <div class="run-line">
            <span class="ds-badge" :data-tone="runStatusTone(run.status)">{{ runStatusLabel(run.status) }}</span>
            <span v-if="run.version === 0" class="ds-badge" data-tone="purple">{{ t("workflow.run.draftRun") }}</span>
            <span v-else class="ds-badge">v{{ run.version }}</span>
            <span class="ds-badge">{{ triggerSourceLabel(run.triggerSource) }}</span>
            <small>{{ new Date(run.createdAt).toLocaleString() }}</small>
            <small>{{ t("workflow.run.duration", { time: formatDuration(elapsedMs(run.startedAt ?? run.createdAt, run.completedAt, now)) }) }}</small>
          </div>
          <p class="run-inputs">{{ summarizeInputs(run.inputs, 140) }}</p>
          <small v-if="run.error" class="run-err">{{ run.errorCode ? `${run.errorCode}: ` : "" }}{{ run.error }}</small>
        </div>
        <UiButton v-if="isRunTerminal(run.status) && !remote.isRemote" variant="ghost" size="icon" :tip="t('common.delete')" @click.stop="remove(run)"><Trash2 :size="14" /></UiButton>
      </article>
    </div>
  </div>
</template>

<style scoped>
.run-row {
  cursor: pointer;
}
.run-row:hover {
  background: var(--color-surface-hover);
}
.run-line {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--spacing-2);
}
.run-line small {
  color: var(--color-text-tertiary);
}
.run-inputs {
  margin: 0;
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  text-overflow: ellipsis;
  white-space: nowrap;
}
.run-err {
  color: var(--color-error);
}
.page-error { margin: 0 0 var(--spacing-3); color: var(--color-error); font-size: var(--font-size-sm); }
</style>
