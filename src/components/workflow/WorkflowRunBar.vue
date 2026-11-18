<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { GitBranch } from "@lucide/vue";
import { getWorkflowRun } from "../../services/backend";
import { isWorkflowRunSession, workflowRunIdFromSession, type RunDetail } from "../../domain/workflow";
import { t } from "../../i18n";

/** 总览会话（wf-run:<runId>）上方的运行状态条：状态 + 「打开运行视图」。 */
const props = defineProps<{ sessionId: string }>();
const router = useRouter();

const detail = ref<RunDetail | null>(null);
const runId = computed(() => (isWorkflowRunSession(props.sessionId) ? workflowRunIdFromSession(props.sessionId) : ""));
let unlisten: UnlistenFn | null = null;
let disposed = false;

async function refresh() {
  if (!runId.value) return;
  try {
    detail.value = await getWorkflowRun(runId.value);
  } catch {
    detail.value = null;
  }
}

watch(runId, () => {
  detail.value = null;
  void refresh();
}, { immediate: true });

void listen<{ runId: string }>("workflow-run-changed", (event) => {
  if (event.payload.runId === runId.value) void refresh();
}).then((fn) => {
  if (disposed) fn();
  else unlisten = fn;
}).catch(() => undefined);

onBeforeUnmount(() => {
  disposed = true;
  unlisten?.();
});

const tone = computed(() => {
  switch (detail.value?.run.status) {
    case "succeeded": return "success";
    case "failed": return "error";
    case "waiting": return "warning";
    case "running": return "info";
    default: return undefined;
  }
});

function open() {
  if (!detail.value) return;
  void router.push(`/workflows/${detail.value.run.workflowId}/runs/${detail.value.run.id}`);
}
</script>

<template>
  <div v-if="runId" class="workflow-run-bar">
    <GitBranch :size="14" />
    <strong>{{ detail?.workflow.name ?? t("workflow.link.run") }}</strong>
    <span v-if="detail" class="ds-badge" :data-tone="tone">{{ t(`workflow.link.status.${detail.run.status}`) }}</span>
    <span class="grow" />
    <UiButton size="small" variant="ghost" :disabled="!detail" @click="open">{{ t("workflow.link.openRun") }}</UiButton>
  </div>
</template>

<style scoped>
.workflow-run-bar {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  padding: var(--spacing-1-5) var(--spacing-4);
  border-bottom: 1px solid var(--color-border-light);
  background: var(--color-bg-secondary);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}
.workflow-run-bar .grow {
  display: block;
  flex: 1;
}
.workflow-run-bar strong {
  color: var(--color-text);
  font-weight: var(--font-weight-medium);
}
</style>
