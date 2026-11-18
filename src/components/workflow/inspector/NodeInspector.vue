<script setup lang="ts">
import { computed, ref, watch, type Component } from "vue";
import { Plus, Trash2 } from "@lucide/vue";
import SegmentedTabs from "../../ui/SegmentedTabs.vue";
import { useWorkflowsStore } from "../../../stores/workflows";
import {
  PORT_COLORS,
  effectiveInputs,
  effectiveOutputs,
  isImplicitPort,
  isValidNodeId,
  type NodeDef,
  type NodeKind,
  type PortDef,
  type WorkspaceUse,
} from "../../../domain/workflow";
import { t } from "../../../i18n";
import PortListEditor from "./PortListEditor.vue";
import AgentConfig from "./config/AgentConfig.vue";
import GateConfig from "./config/GateConfig.vue";
import GitConfig from "./config/GitConfig.vue";
import HttpConfig from "./config/HttpConfig.vue";
import MapConfig from "./config/MapConfig.vue";
import NoteConfig from "./config/NoteConfig.vue";
import NotifyConfig from "./config/NotifyConfig.vue";
import OutputConfig from "./config/OutputConfig.vue";
import ScriptConfig from "./config/ScriptConfig.vue";
import SubflowConfig from "./config/SubflowConfig.vue";
import SwitchConfig from "./config/SwitchConfig.vue";
import TransformConfig from "./config/TransformConfig.vue";
import TriggerConfig from "./config/TriggerConfig.vue";
import "./inspector.css";

const props = defineProps<{ nodeId: string }>();

const store = useWorkflowsStore();
const node = computed(() => store.draft?.nodes.find((item) => item.id === props.nodeId) ?? null);
const tab = ref("basic");
const idDraft = ref("");
const idError = ref("");

const CONFIGS: Record<NodeKind, Component> = {
  trigger: TriggerConfig, output: OutputConfig, agent: AgentConfig, script: ScriptConfig, gate: GateConfig,
  switch: SwitchConfig, transform: TransformConfig, http: HttpConfig, git: GitConfig, notify: NotifyConfig,
  subflow: SubflowConfig, map: MapConfig, note: NoteConfig,
};

watch(() => props.nodeId, () => {
  idDraft.value = props.nodeId;
  idError.value = "";
}, { immediate: true });

const tabs = computed(() => {
  const kind = node.value?.kind;
  const all = [
    { id: "basic", label: t("workflow.inspector.tab.basic") },
    { id: "inputs", label: t("workflow.inspector.tab.inputs") },
    { id: "outputs", label: t("workflow.inspector.tab.outputs") },
    { id: "config", label: t("workflow.inspector.tab.config") },
    { id: "advanced", label: t("workflow.inspector.tab.advanced") },
  ];
  if (kind === "trigger" || kind === "note" || kind === "output") return all.filter((item) => item.id === "basic" || item.id === "config");
  return all;
});
watch(tabs, (list) => { if (!list.some((item) => item.id === tab.value)) tab.value = "basic"; });

const implicitInputs = computed<PortDef[]>(() => (node.value ? effectiveInputs(node.value).filter((port) => isImplicitPort(port.name)) : []));
const implicitOutputs = computed<PortDef[]>(() => {
  if (!node.value || !store.draft) return [];
  const declared = new Set(node.value.outputs.map((port) => port.name));
  return effectiveOutputs(node.value, store.draft).filter((port) => !declared.has(port.name));
});

function commitId() {
  const next = idDraft.value.trim();
  if (next === props.nodeId) {
    idError.value = "";
    return;
  }
  if (!isValidNodeId(next)) {
    idError.value = t("workflow.inspector.idInvalid");
    return;
  }
  if (!store.renameNode(props.nodeId, next)) {
    idError.value = t("workflow.inspector.idDuplicate");
    return;
  }
  idError.value = "";
}

function patch(change: (node: NodeDef) => void, key?: string) {
  store.updateNode(props.nodeId, change, key);
}

function setText(field: "title" | "description", value: string) {
  patch((item) => {
    if (value === "" && field === "description") delete item.description;
    else item[field] = value;
  }, `node:${props.nodeId}:${field}`);
}

function setInputs(ports: PortDef[]) {
  patch((item) => { item.inputs = ports; }, `ports:${props.nodeId}:in`);
}
function setOutputs(ports: PortDef[]) {
  patch((item) => { item.outputs = ports; }, `ports:${props.nodeId}:out`);
}

// ---------- 高级 ----------
const retry = computed(() => node.value?.retry);

function setRetryEnabled(on: boolean) {
  patch((item) => {
    if (on) item.retry = { maxAttempts: 2, backoffSeconds: 10, on: ["failed", "timeout"] };
    else delete item.retry;
  });
}

function patchRetry(change: Partial<{ maxAttempts: number; backoffSeconds: number }>) {
  patch((item) => { if (item.retry) Object.assign(item.retry, change); }, `retry:${props.nodeId}`);
}

function toggleRetryOn(reason: string, on: boolean) {
  patch((item) => {
    if (!item.retry) return;
    const list = new Set(item.retry.on);
    if (on) list.add(reason);
    else list.delete(reason);
    item.retry.on = [...list];
  });
}

function setTimeoutMinutes(value: unknown) {
  const minutes = Number(value);
  patch((item) => {
    if (minutes > 0) item.timeoutMinutes = Math.floor(minutes);
    else delete item.timeoutMinutes;
  }, `timeout:${props.nodeId}`);
}

function addWorkspace() {
  const slot = store.draft?.workspaces.find((item) => !node.value?.workspaces?.some((use) => use.slot === item.key));
  if (!slot) return;
  patch((item) => { (item.workspaces ??= []).push({ slot: slot.key, access: "read" }); });
}

function patchWorkspace(index: number, change: Partial<WorkspaceUse>) {
  patch((item) => {
    const use = item.workspaces?.[index];
    if (!use) return;
    Object.assign(use, change);
    if (use.access === "read" || !use.fork) delete use.fork;
  });
}

function removeWorkspace(index: number) {
  patch((item) => { item.workspaces?.splice(index, 1); });
}

const slotOptions = computed(() => (store.draft?.workspaces ?? []).map((slot) => ({ value: slot.key, label: slot.label ? `${slot.key} · ${slot.label}` : slot.key })));
const canWorkspace = computed(() => ["agent", "script", "git"].includes(node.value?.kind ?? ""));
const canRetry = computed(() => ["agent", "script", "http", "git", "notify"].includes(node.value?.kind ?? ""));
</script>

<template>
  <div v-if="node" class="node-inspector">
    <header class="head">
      <span class="kind-chip">{{ t(`workflow.inspector.kind.${node.kind}`) }}</span>
      <span class="wf-mono head-id">{{ node.id }}</span>
    </header>
    <SegmentedTabs v-model="tab" :items="tabs" :label="t('workflow.inspector.title')" />

    <div class="body">
      <div v-if="tab === 'basic'" class="wf-form">
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.nodeTitle") }}</span>
          <UiInput :model-value="node.title" @update:model-value="setText('title', String($event))" />
        </label>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.nodeId") }}</span>
          <input v-model="idDraft" class="ui-input wf-mono" :class="{ 'is-error': idError }" :disabled="node.kind === 'trigger'" @change="commitId" @keydown.enter="commitId">
          <span v-if="idError" class="wf-error">{{ idError }}</span>
          <span v-else class="wf-hint">{{ t("workflow.inspector.idHint") }}</span>
        </label>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.description") }}</span>
          <UiTextarea class="wf-textarea" :model-value="node.description ?? ''" @update:model-value="setText('description', $event ?? '')" />
        </label>
        <p v-if="node.templateId" class="wf-hint">{{ t("workflow.inspector.fromTemplate", { id: node.templateId }) }}</p>
      </div>

      <div v-else-if="tab === 'inputs'" class="wf-form">
        <PortListEditor :model-value="node.inputs" mode="input" @update:model-value="setInputs" />
        <div v-if="implicitInputs.length" class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.implicit") }}</span>
          <div class="chips"><span v-for="port in implicitInputs" :key="port.name" class="chip wf-mono"><i :style="{ background: PORT_COLORS[port.type] }" />{{ port.name }}</span></div>
          <span class="wf-hint">{{ t("workflow.inspector.implicitInHint") }}</span>
        </div>
      </div>

      <div v-else-if="tab === 'outputs'" class="wf-form">
        <PortListEditor :model-value="node.outputs" mode="output" :reserved="implicitOutputs.map((port) => port.name)" @update:model-value="setOutputs" />
        <div v-if="implicitOutputs.length" class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.implicit") }}</span>
          <div class="chips"><span v-for="port in implicitOutputs" :key="port.name" class="chip wf-mono"><i :style="{ background: PORT_COLORS[port.type] }" />{{ port.name }}</span></div>
          <span class="wf-hint">{{ t("workflow.inspector.implicitOutHint") }}</span>
        </div>
      </div>

      <component :is="CONFIGS[node.kind]" v-else-if="tab === 'config'" :key="node.id" :node="node" />

      <div v-else class="wf-form">
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.join") }}</span>
          <UiSelect :model-value="node.join ?? 'all'" :options="[{ value: 'all', label: t('workflow.inspector.joinAll') }, { value: 'any', label: t('workflow.inspector.joinAny') }]" @update:model-value="patch((item) => { item.join = $event as 'all' | 'any'; })" />
          <span class="wf-hint">{{ t("workflow.inspector.joinHint") }}</span>
        </label>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.onError") }}</span>
          <UiSelect :model-value="node.onError ?? 'fail_run'" :options="[{ value: 'fail_run', label: t('workflow.inspector.failRun') }, { value: 'continue', label: t('workflow.inspector.continue') }]" @update:model-value="patch((item) => { item.onError = $event as 'fail_run' | 'continue'; })" />
          <span class="wf-hint">{{ t("workflow.inspector.onErrorHint") }}</span>
        </label>
        <label v-if="node.kind !== 'gate'" class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.timeout") }}</span>
          <UiInput type="number" min="0" :model-value="node.timeoutMinutes ?? ''" :placeholder="t('workflow.inspector.timeoutDefault')" @update:model-value="setTimeoutMinutes" />
        </label>
        <div v-if="canRetry" class="wf-section">
          <label class="wf-check">
            <input type="checkbox" class="ui-checkbox" :checked="Boolean(retry)" @change="setRetryEnabled(($event.target as HTMLInputElement).checked)">
            {{ t("workflow.inspector.retry") }}
          </label>
          <template v-if="retry">
            <div class="wf-grid-2">
              <label class="wf-field">
                <span class="wf-label">{{ t("workflow.inspector.maxAttempts") }}</span>
                <UiInput type="number" min="1" max="10" :model-value="retry.maxAttempts" @update:model-value="patchRetry({ maxAttempts: Math.max(1, Number($event) || 1) })" />
              </label>
              <label class="wf-field">
                <span class="wf-label">{{ t("workflow.inspector.backoff") }}</span>
                <UiInput type="number" min="0" :model-value="retry.backoffSeconds" @update:model-value="patchRetry({ backoffSeconds: Math.max(0, Number($event) || 0) })" />
              </label>
            </div>
            <div class="wf-row">
              <label v-for="reason in ['failed', 'timeout']" :key="reason" class="wf-check">
                <input type="checkbox" class="ui-checkbox" :checked="retry.on.includes(reason)" @change="toggleRetryOn(reason, ($event.target as HTMLInputElement).checked)">
                {{ t(`workflow.inspector.retryOn.${reason}`) }}
              </label>
            </div>
            <span class="wf-hint">{{ t("workflow.inspector.backoffHint") }}</span>
          </template>
        </div>
        <div v-if="canWorkspace" class="wf-section">
          <div class="wf-section-title">
            {{ t("workflow.inspector.workspaces") }}
            <UiButton size="small" :disabled="!slotOptions.length || (node.workspaces?.length ?? 0) >= slotOptions.length" @click="addWorkspace"><Plus :size="14" /> {{ t("workflow.config.add") }}</UiButton>
          </div>
          <div v-if="!slotOptions.length" class="wf-empty">{{ t("workflow.inspector.noSlots") }}</div>
          <div v-for="(use, index) in node.workspaces ?? []" :key="index" class="wf-card">
            <div class="wf-row">
              <UiSelect class="grow-1" :model-value="use.slot" :options="slotOptions" @update:model-value="patchWorkspace(index, { slot: String($event) })" />
              <UiSelect style="width: 96px; flex: none" :model-value="use.access" :options="[{ value: 'read', label: t('workflow.inspector.read') }, { value: 'write', label: t('workflow.inspector.write') }]" @update:model-value="patchWorkspace(index, { access: $event as 'read' | 'write' })" />
              <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="removeWorkspace(index)"><Trash2 :size="13" /></UiButton>
            </div>
            <label v-if="use.access === 'write'" class="wf-check">
              <input type="checkbox" class="ui-checkbox" :checked="Boolean(use.fork)" @change="patchWorkspace(index, { fork: ($event.target as HTMLInputElement).checked })">
              {{ t("workflow.inspector.fork") }}
            </label>
          </div>
          <span class="wf-hint">{{ t("workflow.inspector.workspacesHint") }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.node-inspector {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-3);
  min-width: 0;
  padding: var(--spacing-3);
}
.head {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  min-width: 0;
}
.kind-chip {
  padding: 1px var(--spacing-2);
  border-radius: var(--radius-full);
  background: var(--color-bg-tertiary);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
.head-id {
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
}
.body {
  min-width: 0;
}
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--spacing-1);
}
.chip {
  display: inline-flex;
  align-items: center;
  gap: var(--spacing-1);
  padding: 1px var(--spacing-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-full);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
.chip i {
  width: 6px;
  height: 6px;
  border-radius: var(--radius-full);
}
</style>
