<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { KeyRound, Plus, Trash2 } from "@lucide/vue";
import { useWorkflowsStore } from "../../../stores/workflows";
import { useAppStore } from "../../../stores/app";
import { deleteWorkflowSecret, setWorkflowSecret } from "../../../services/backend";
import { isValidNodeId, type Isolation, type PortDef, type WorkflowSettings, type WorkspaceSlotDef } from "../../../domain/workflow";
import { t } from "../../../i18n";
import PortListEditor from "./PortListEditor.vue";
import "./inspector.css";

const store = useWorkflowsStore();
const app = useAppStore();
const draft = computed(() => store.draft);

onMounted(() => {
  void store.loadSecrets().catch(() => undefined);
});

// ---------- 入参 / 出参 ----------
function setInputs(ports: PortDef[]) {
  store.mutate((def) => { def.inputs = ports; }, "wf-inputs");
}
function setOutputs(ports: PortDef[]) {
  store.mutate((def) => { def.outputs = ports; }, "wf-outputs");
}

// ---------- 槽位 ----------
const isolationOptions = computed(() => (["worktree_new", "worktree_existing", "direct"] as Isolation[]).map((value) => ({
  value,
  label: t(`workflow.settings.isolation.${value}`),
})));

function projectOptions(current: string) {
  const options = (app.data?.projects ?? [])
    .filter((project) => !project.archivedAt)
    .map((project) => ({ value: project.id, label: project.name }));
  const templated = (draft.value?.inputs ?? [])
    .filter((port) => port.type === "text")
    .map((port) => ({ value: `{{ run.inputs.${port.name} }}`, label: t("workflow.settings.fromInput", { name: port.name }) }));
  const all = [...templated, ...options];
  if (current && !all.some((item) => item.value === current)) all.unshift({ value: current, label: current });
  return all;
}

function addSlot() {
  store.mutate((def) => {
    const used = new Set(def.workspaces.map((slot) => slot.key));
    let key = "app";
    let index = 2;
    while (used.has(key)) key = `app_${index++}`;
    def.workspaces.push({ key, label: "", project: "", isolation: "worktree_new", autoCommit: true });
  });
}

function patchSlot(index: number, change: Partial<WorkspaceSlotDef>, coalesce?: string) {
  store.mutate((def) => {
    const slot = def.workspaces[index];
    if (!slot) return;
    const previous = slot.key;
    Object.assign(slot, change);
    for (const key of Object.keys(change) as (keyof WorkspaceSlotDef)[]) {
      if (slot[key] === "" && !["key", "label", "project"].includes(key)) delete slot[key];
    }
    // 槽位改名：同步节点引用
    if (change.key !== undefined && change.key !== previous) {
      for (const node of def.nodes) {
        for (const use of node.workspaces ?? []) if (use.slot === previous) use.slot = change.key;
        const config = node.config as Record<string, unknown>;
        if (config.slot === previous) config.slot = change.key;
        const cwd = config.cwd as { slot?: string } | null | undefined;
        if (cwd && cwd.slot === previous) cwd.slot = change.key;
        if (Array.isArray(config.showDiffOf)) config.showDiffOf = config.showDiffOf.map((item) => (item === previous ? change.key : item));
      }
    }
  }, coalesce ?? `slot:${index}`);
}

function slotKeyError(slot: WorkspaceSlotDef, index: number) {
  if (!isValidNodeId(slot.key)) return t("workflow.settings.slotKeyInvalid");
  if (draft.value?.workspaces.some((other, i) => i !== index && other.key === slot.key)) return t("workflow.inspector.idDuplicate");
  return "";
}

function removeSlot(index: number) {
  store.mutate((def) => { def.workspaces.splice(index, 1); });
}

// ---------- 通用设置 ----------
const agentOptions = computed(() => (app.data?.agents ?? []).map((agent) => ({ value: agent.id, label: agent.name, avatar: agent.avatar })));

function patchSettings(change: Partial<WorkflowSettings>) {
  store.mutate((def) => {
    Object.assign(def.settings, change);
    if (!def.settings.runTimeoutMinutes) delete def.settings.runTimeoutMinutes;
  }, `settings:${Object.keys(change).join(",")}`);
}

// ---------- 密钥（值只写不读） ----------
const secretName = ref("");
const secretValue = ref("");
const secretDescription = ref("");
const secretBusy = ref(false);
const secretError = ref("");

const secretNameValid = computed(() => /^[A-Za-z0-9_]+$/.test(secretName.value.trim()));

async function saveSecret() {
  if (!secretNameValid.value || !secretValue.value) return;
  secretBusy.value = true;
  secretError.value = "";
  try {
    store.secrets = await setWorkflowSecret(secretName.value.trim(), secretValue.value, secretDescription.value.trim());
    secretName.value = "";
    secretDescription.value = "";
  } catch (cause) {
    secretError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    secretValue.value = "";
    secretBusy.value = false;
  }
}

async function removeSecret(name: string) {
  secretError.value = "";
  try {
    store.secrets = await deleteWorkflowSecret(name);
  } catch (cause) {
    secretError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function editSecret(name: string, description: string) {
  secretName.value = name;
  secretDescription.value = description;
  secretValue.value = "";
}
</script>

<template>
  <div v-if="draft" class="wf-form settings">
    <section class="wf-section first">
      <div class="wf-section-title">{{ t("workflow.settings.inputs") }}</div>
      <p class="wf-hint">{{ t("workflow.settings.inputsHint") }}</p>
      <PortListEditor :model-value="draft.inputs" mode="workflow-input" @update:model-value="setInputs" />
    </section>

    <section class="wf-section">
      <div class="wf-section-title">{{ t("workflow.settings.outputs") }}</div>
      <p class="wf-hint">{{ t("workflow.settings.outputsHint") }}</p>
      <PortListEditor :model-value="draft.outputs" mode="workflow-output" @update:model-value="setOutputs" />
    </section>

    <section class="wf-section">
      <div class="wf-section-title">
        {{ t("workflow.settings.workspaces") }}
        <UiButton size="small" @click="addSlot"><Plus :size="14" /> {{ t("workflow.config.add") }}</UiButton>
      </div>
      <p class="wf-hint">{{ t("workflow.settings.workspacesHint") }}</p>
      <div v-for="(slot, index) in draft.workspaces" :key="index" class="wf-card">
        <div class="wf-row">
          <input class="ui-input wf-mono" style="width: 38%" :class="{ 'is-error': slotKeyError(slot, index) }" :value="slot.key" @change="patchSlot(index, { key: ($event.target as HTMLInputElement).value })">
          <input class="ui-input grow-1" :value="slot.label" :placeholder="t('workflow.settings.slotLabel')" @input="patchSlot(index, { label: ($event.target as HTMLInputElement).value })">
          <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="removeSlot(index)"><Trash2 :size="13" /></UiButton>
        </div>
        <p v-if="slotKeyError(slot, index)" class="wf-error">{{ slotKeyError(slot, index) }}</p>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.settings.project") }}</span>
          <UiSelect :model-value="slot.project" :options="projectOptions(slot.project)" :placeholder="t('workflow.settings.projectPlaceholder')" @update:model-value="patchSlot(index, { project: String($event) })" />
        </label>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.settings.isolation") }}</span>
          <UiSelect :model-value="slot.isolation" :options="isolationOptions" @update:model-value="patchSlot(index, { isolation: $event as Isolation })" />
        </label>
        <div class="wf-grid-2">
          <label class="wf-field">
            <span class="wf-label">{{ t("workflow.settings.workspaceName") }}</span>
            <input class="ui-input" :value="slot.workspace ?? ''" @input="patchSlot(index, { workspace: ($event.target as HTMLInputElement).value })">
          </label>
          <label class="wf-field">
            <span class="wf-label">{{ t("workflow.settings.baseRef") }}</span>
            <input class="ui-input wf-mono" :value="slot.baseRef ?? ''" placeholder="main" @input="patchSlot(index, { baseRef: ($event.target as HTMLInputElement).value })">
          </label>
        </div>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.settings.branch") }}</span>
          <input class="ui-input wf-mono" :value="slot.branch ?? ''" placeholder="wf/{{ workflow.slug }}-{{ run.short_id }}" @input="patchSlot(index, { branch: ($event.target as HTMLInputElement).value })">
        </label>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.settings.inherit") }}</span>
          <input class="ui-input wf-mono" :value="slot.inherit ?? ''" @input="patchSlot(index, { inherit: ($event.target as HTMLInputElement).value })">
          <span class="wf-hint">{{ t("workflow.settings.inheritHint") }}</span>
        </label>
        <label class="wf-check">
          <input type="checkbox" class="ui-checkbox" :checked="slot.autoCommit ?? true" @change="patchSlot(index, { autoCommit: ($event.target as HTMLInputElement).checked })">
          {{ t("workflow.settings.autoCommit") }}
        </label>
      </div>
    </section>

    <section class="wf-section">
      <div class="wf-section-title">{{ t("workflow.settings.general") }}</div>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.settings.owner") }}</span>
        <UiSelect :model-value="draft.settings.ownerAgentId" :options="agentOptions" @update:model-value="patchSettings({ ownerAgentId: String($event) })" />
        <span class="wf-hint">{{ t("workflow.settings.ownerHint") }}</span>
      </label>
      <div class="wf-grid-2">
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.settings.maxParallel") }}</span>
          <UiInput type="number" min="1" max="32" :model-value="draft.settings.maxParallelNodes" @update:model-value="patchSettings({ maxParallelNodes: Math.max(1, Number($event) || 1) })" />
        </label>
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.settings.runTimeout") }}</span>
          <UiInput type="number" min="0" :model-value="draft.settings.runTimeoutMinutes ?? ''" :placeholder="t('workflow.settings.noLimit')" @update:model-value="patchSettings({ runTimeoutMinutes: Number($event) || undefined })" />
        </label>
      </div>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.settings.concurrency") }}</span>
        <UiSelect :model-value="draft.settings.concurrency" :options="['queue', 'skip_if_running', 'allow'].map((value) => ({ value, label: t(`workflow.settings.concurrency.${value}`) }))" @update:model-value="patchSettings({ concurrency: $event as WorkflowSettings['concurrency'] })" />
      </label>
    </section>

    <section class="wf-section">
      <div class="wf-section-title"><span><KeyRound :size="13" /> {{ t("workflow.settings.secrets") }}</span></div>
      <p class="wf-hint">{{ t("workflow.settings.secretsHint") }}</p>
      <div v-if="!store.secrets.length" class="wf-empty">{{ t("workflow.settings.noSecrets") }}</div>
      <div v-for="secret in store.secrets" :key="secret.name" class="wf-card secret">
        <div class="wf-row">
          <span class="wf-mono grow-1">{{ secret.name }}</span>
          <span class="wf-hint">••••••••</span>
          <UiButton size="small" variant="ghost" @click="editSecret(secret.name, secret.description)">{{ t("workflow.settings.replace") }}</UiButton>
          <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="removeSecret(secret.name)"><Trash2 :size="13" /></UiButton>
        </div>
        <span v-if="secret.description" class="wf-hint">{{ secret.description }}</span>
      </div>
      <form class="wf-card" autocomplete="off" @submit.prevent="saveSecret">
        <input v-model="secretName" class="ui-input wf-mono" :placeholder="t('workflow.settings.secretName')" autocomplete="off">
        <input v-model="secretValue" class="ui-input" type="password" :placeholder="t('workflow.settings.secretValue')" autocomplete="new-password">
        <input v-model="secretDescription" class="ui-input" :placeholder="t('workflow.settings.secretDescription')">
        <p v-if="secretName && !secretNameValid" class="wf-error">{{ t("workflow.settings.secretNameInvalid") }}</p>
        <p v-if="secretError" class="wf-error">{{ secretError }}</p>
        <div><UiButton type="submit" size="small" :disabled="secretBusy || !secretNameValid || !secretValue">{{ t("workflow.settings.saveSecret") }}</UiButton></div>
      </form>
    </section>
  </div>
</template>

<style scoped>
.settings {
  padding: var(--spacing-3);
}
.first {
  border-top: 0;
  padding-top: 0;
}
.wf-section-title span {
  display: inline-flex;
  align-items: center;
  gap: var(--spacing-1);
}
</style>
