<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RotateCcw, SkipForward, X } from "@lucide/vue";
import { t } from "../../../i18n";
import type { ChatMessage } from "../../../domain/types";
import * as backend from "../../../services/backend";
import ChatMessageItem from "../../ChatMessageItem.vue";
import SegmentedTabs, { type SegmentedTab } from "../../ui/SegmentedTabs.vue";
import {
  currentNodeRun,
  nodeRunHistory,
  type NodeRun,
  type RunDetail,
} from "../../../domain/workflow";
import { kindLabelKey, nodeDisplayTitle } from "../node-meta";
import AgentSessionPane from "./AgentSessionPane.vue";
import { elapsedMs, formatDuration, nodeStatusLabel, nodeStatusTone, summarizeValue } from "./run-format";

const props = defineProps<{ detail: RunDetail; nodeId: string }>();
const emit = defineEmits<{ close: [] }>();

const tab = ref("overview");
const pickedId = ref<string | null>(null);
const retryOpen = ref(false);
const skipOpen = ref(false);
const skipText = ref("{}");
const busy = ref(false);
const error = ref("");

const node = computed(() => props.detail.definition.nodes.find((item) => item.id === props.nodeId));
const displayTitle = computed(() => nodeDisplayTitle(node.value, props.nodeId, t));
const history = computed(() => nodeRunHistory(props.detail.nodeRuns, props.nodeId));
const current = computed(() => currentNodeRun(props.detail.nodeRuns, props.nodeId));
const selected = computed<NodeRun | undefined>(() => history.value.find((item) => item.id === pickedId.value) ?? current.value);
const isCurrent = computed(() => selected.value?.id === current.value?.id);
const usage = computed(() => props.detail.usage?.[props.nodeId]);
const blockedAgent = computed(() => node.value?.kind === "agent" && selected.value?.status === "waiting_human");
const confirmTargets = computed(() => {
  if (!blockedAgent.value || !isCurrent.value) return [];
  return props.detail.definition.edges
    .filter((edge) => edge.source === props.nodeId)
    .map((edge) => ({
      id: edge.target,
      title: nodeDisplayTitle(props.detail.definition.nodes.find((item) => item.id === edge.target), edge.target, t),
    }));
});
const chatSession = computed(() => (blockedAgent.value ? selected.value?.sessionId ?? "" : ""));
const answer = computed<ChatMessage | null>(() => {
  const text = selected.value?.content?.trim();
  if (!text || !selected.value) return null;
  const target = (node.value?.config as { target?: { type?: string; id?: string } } | undefined)?.target;
  return {
    id: `step:${selected.value.id}`,
    sessionId: selected.value.sessionId ?? "",
    role: "assistant",
    content: text,
    runId: null,
    agentId: target?.type === "agent" ? target.id : null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: selected.value.completedAt ?? selected.value.createdAt,
  };
});

watch(() => props.nodeId, () => {
  pickedId.value = null;
  tab.value = "overview";
  error.value = "";
});

const tabs = computed<SegmentedTab[]>(() => {
  const list: SegmentedTab[] = [
    { id: "overview", label: t("workflow.drawer.overview") },
  ];
  if (node.value?.kind === "agent" && selected.value?.sessionId && !chatSession.value) list.push({ id: "session", label: t("workflow.drawer.session") });
  if (history.value.length > 1) list.push({ id: "attempts", label: t("workflow.drawer.attempts"), count: history.value.length });
  return list;
});

watch(tabs, (list) => {
  if (!list.some((item) => item.id === tab.value)) tab.value = "overview";
});

function runLabel(item: NodeRun) {
  return t("workflow.drawer.roundLabel", { iteration: item.iteration + 1, attempt: item.attempt });
}

const canRetry = computed(() => isCurrent.value && ["failed", "cancelled"].includes(selected.value?.status ?? ""));
const canSkip = computed(() => isCurrent.value && selected.value?.status === "failed");

function openSkip() {
  skipText.value = selected.value?.content ?? "";
  error.value = "";
  skipOpen.value = true;
}

async function doRetry() {
  busy.value = true;
  error.value = "";
  try {
    await backend.retryWorkflowNode(props.detail.run.id, props.nodeId, false);
    retryOpen.value = false;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

async function confirmNext(target: string) {
  if (!selected.value) return;
  busy.value = true;
  error.value = "";
  try {
    await backend.confirmWorkflowNode(selected.value.id, target);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

async function doSkip() {
  busy.value = true;
  error.value = "";
  try {
    await backend.skipWorkflowNode(props.detail.run.id, props.nodeId, skipText.value);
    skipOpen.value = false;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <aside class="node-drawer">
    <header class="drawer-head">
      <div class="drawer-title">
        <strong>{{ displayTitle }}</strong>
        <small>{{ node ? t(kindLabelKey(node.kind)) : nodeId }}</small>
      </div>
      <UiSelect
        v-if="history.length > 1"
        class="drawer-run-select"
        size="sm"
        :model-value="selected?.id"
        :options="history.map((item) => ({ value: item.id, label: runLabel(item), meta: nodeStatusLabel(item.status) }))"
        @update:model-value="pickedId = String($event)"
      />
      <UiButton variant="ghost" size="icon" :tip="t('common.close')" @click="emit('close')"><X :size="15" /></UiButton>
    </header>

    <div v-if="!selected" class="drawer-empty">{{ t("workflow.drawer.notRun") }}</div>
    <template v-else>
      <SegmentedTabs v-model="tab" class="drawer-tabs" :items="tabs" :label="t('workflow.drawer.tabs')" />
      <div class="drawer-body" :class="{ fill: tab === 'session' || Boolean(chatSession) }">
        <template v-if="tab === 'overview' && chatSession">
          <AgentSessionPane class="drawer-chat" :session-id="chatSession" reply :sticky-user-turn="false">
            <template #above-composer>
              <div v-if="confirmTargets.length" class="confirm-bar">
                <UiButton
                  v-for="target in confirmTargets"
                  :key="target.id"
                  size="small"
                  :disabled="busy"
                  @click="confirmNext(target.id)"
                >{{ confirmTargets.length === 1 ? t("workflow.drawer.advance") : t("workflow.drawer.confirmTo", { title: target.title }) }}</UiButton>
                <p v-if="error" class="drawer-error">{{ error }}</p>
              </div>
            </template>
          </AgentSessionPane>
        </template>
        <template v-else-if="tab === 'overview'">
          <div class="kv">
            <span>{{ t("workflow.drawer.status") }}</span>
            <span class="ds-badge" :data-tone="nodeStatusTone(selected.status)">{{ nodeStatusLabel(selected.status) }}</span>
            <span>{{ t("workflow.drawer.round") }}</span><span>{{ runLabel(selected) }}</span>
            <span>{{ t("workflow.drawer.started") }}</span><span>{{ selected.startedAt ? new Date(selected.startedAt).toLocaleString() : "—" }}</span>
            <span>{{ t("workflow.drawer.elapsed") }}</span><span>{{ selected.status === "waiting_human" ? t("workflow.gate.waiting") : formatDuration(elapsedMs(selected.startedAt, selected.completedAt)) }}</span>
            <template v-if="usage"><span>{{ t("workflow.drawer.usage") }}</span><span>{{ t("workflow.run.usage", { runs: usage.agentRuns, time: formatDuration(usage.elapsedMs) }) }}</span></template>
            <template v-if="selected.repairCount"><span>{{ t("workflow.drawer.repairs") }}</span><span>{{ selected.repairCount }}</span></template>
            <template v-if="selected.skipReason"><span>{{ t("workflow.drawer.skipReason") }}</span><span>{{ selected.skipReason }}</span></template>
            <template v-if="selected.retryAt"><span>{{ t("workflow.drawer.retryAt") }}</span><span>{{ new Date(selected.retryAt).toLocaleString() }}</span></template>
          </div>
          <div v-if="selected.error || selected.errorCode" class="drawer-error">
            <strong v-if="selected.errorCode">{{ selected.errorCode }}</strong>
            <span>{{ selected.error }}</span>
          </div>
          <ChatMessageItem
            v-if="answer"
            class="step-answer"
            :message="answer"
            view-only
            compact
            :enable-replay="false"
            :allow-continue="false"
            :allow-target-change="false"
            :ui-interactive="false"
          />
          <div v-if="canRetry || canSkip" class="drawer-actions">
            <UiButton v-if="canRetry" size="small" @click="retryOpen = true; error = ''"><RotateCcw :size="14" /> {{ t("workflow.drawer.retry") }}</UiButton>
            <UiButton v-if="canSkip" size="small" @click="openSkip"><SkipForward :size="14" /> {{ t("workflow.drawer.skip") }}</UiButton>
          </div>
        </template>

        <AgentSessionPane v-else-if="tab === 'session' && selected.sessionId" :session-id="selected.sessionId" reply />

        <template v-else-if="tab === 'attempts'">
          <button v-for="item in history" :key="item.id" type="button" class="attempt-row" :class="{ active: item.id === selected.id }" @click="pickedId = item.id; tab = 'overview'">
            <span class="ds-badge" :data-tone="nodeStatusTone(item.status)">{{ nodeStatusLabel(item.status) }}</span>
            <strong>{{ runLabel(item) }}</strong>
            <span>{{ formatDuration(elapsedMs(item.startedAt, item.completedAt)) }}</span>
            <small v-if="item.error">{{ summarizeValue(item.error, 60) }}</small>
          </button>
        </template>
      </div>
    </template>

    <UiModal v-if="retryOpen" @close="retryOpen = false">
      <form class="modal wf-modal" @submit.prevent="doRetry">
        <h2>{{ t("workflow.drawer.retryTitle") }}</h2>
        <p>{{ t("workflow.drawer.retryHint", { node: displayTitle }) }}</p>
        <p v-if="error" class="wf-modal-error">{{ error }}</p>
        <div class="wf-modal-actions">
          <UiButton @click="retryOpen = false">{{ t("common.cancel") }}</UiButton>
          <UiButton type="submit" variant="primary" :disabled="busy">{{ t("workflow.drawer.retry") }}</UiButton>
        </div>
      </form>
    </UiModal>

    <UiModal v-if="skipOpen" @close="skipOpen = false">
      <form class="modal wf-modal" @submit.prevent="doSkip">
        <h2>{{ t("workflow.drawer.skipTitle") }}</h2>
        <p>{{ t("workflow.drawer.skipHint") }}</p>
        <UiTextarea v-model="skipText" :rows="8" />
        <p v-if="error" class="wf-modal-error">{{ error }}</p>
        <div class="wf-modal-actions">
          <UiButton @click="skipOpen = false">{{ t("common.cancel") }}</UiButton>
          <UiButton type="submit" variant="primary" :disabled="busy">{{ t("workflow.drawer.skip") }}</UiButton>
        </div>
      </form>
    </UiModal>
  </aside>
</template>

<style scoped>
.node-drawer {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  height: 100%;
  border-left: 1px solid var(--color-border);
  background: var(--color-bg);
}
.drawer-head {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  padding: var(--spacing-1) var(--spacing-3);
  border-bottom: 1px solid var(--color-border-light);
}
.drawer-title {
  display: grid;
  flex: 1;
  min-width: 0;
}
.drawer-title small {
  color: var(--color-text-tertiary);
  font-size: var(--font-size-xs);
}
.drawer-run-select {
  width: 150px;
}
.drawer-tabs {
  margin: var(--spacing-1) var(--spacing-3) 0;
}
.drawer-body {
  display: grid;
  align-content: start;
  gap: var(--spacing-3);
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: var(--spacing-3);
}
.drawer-chat { flex: 1; min-height: 0; }
/* 提问卡贴在底部，限高后选项自己滚，消息区仍能看到上面的回答 */
.drawer-chat :deep(.message-list) {
  flex: 1 1 auto;
  min-height: 140px;
}
.drawer-chat :deep(.chat-question-card) {
  gap: 4px;
  margin-bottom: 4px;
  padding: 6px 8px 4px;
}
.drawer-chat :deep(.chat-question-body) {
  max-height: 148px;
  gap: 2px;
}
.drawer-chat :deep(.chat-question-text) {
  font-size: var(--font-size-sm);
  line-height: 1.35;
}
.drawer-chat :deep(.chat-question-option) {
  padding: 2px 4px;
}
.drawer-chat :deep(.chat-question-option-text small) {
  font-size: var(--font-size-xs);
  line-height: 1.3;
}
.drawer-body.fill {
  display: flex;
  flex-direction: column;
  padding: var(--spacing-2);
  overflow: hidden;
}
.drawer-empty {
  margin: 0;
  padding: var(--spacing-3);
  color: var(--color-text-tertiary);
  font-size: var(--font-size-sm);
}
.kv {
  display: grid;
  grid-template-columns: 88px 1fr;
  gap: var(--spacing-1-5) var(--spacing-3);
  align-items: center;
  font-size: var(--font-size-sm);
}
.kv > span:nth-child(odd) {
  color: var(--color-text-tertiary);
}
.drawer-error {
  display: grid;
  gap: var(--spacing-1);
  padding: var(--spacing-2) var(--spacing-3);
  border: 1px solid var(--color-error-border);
  border-radius: var(--radius-md);
  background: var(--color-error-subtle);
  color: var(--color-error);
  font-size: var(--font-size-sm);
  overflow-wrap: anywhere;
}
.confirm-bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--spacing-2);
  margin-bottom: var(--spacing-2);
}
.drawer-actions {
  display: flex;
  gap: var(--spacing-2);
}
.step-answer {
  min-width: 0;
}
.port-block {
  display: grid;
  gap: var(--spacing-1);
}
.result-label {
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: var(--font-size-sm);
}
.port-block header {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  font-size: var(--font-size-sm);
}
.port-block small {
  color: var(--color-text-tertiary);
}
.attempt-row {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  padding: var(--spacing-2) var(--spacing-3);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  color: var(--color-text);
  font-size: var(--font-size-sm);
  text-align: left;
  cursor: pointer;
}
.attempt-row:hover,
.attempt-row.active {
  background: var(--color-surface-hover);
}
.attempt-row small {
  overflow: hidden;
  color: var(--color-error);
  text-overflow: ellipsis;
  white-space: nowrap;
}
.wf-modal {
  display: grid;
  gap: var(--spacing-3);
  width: min(460px, 92vw);
}
.wf-modal h2,
.wf-modal p {
  margin: 0;
}
.wf-modal p {
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}
.wf-modal .wf-modal-error {
  color: var(--color-error);
}
.wf-modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-2);
}
.wf-mono :deep(textarea),
.wf-mono {
  font-family: var(--font-family-mono);
}
</style>
