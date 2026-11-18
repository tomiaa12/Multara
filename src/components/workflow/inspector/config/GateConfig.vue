<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Plus, Trash2 } from "@lucide/vue";
import { defaultGateDecisions, isValidPortName, type GateConfig, type NodeDef } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import TemplateField from "../TemplateField.vue";
import { parseJsonText } from "../schema-infer";
import { renameSourcePort, useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { store, cfg, set, update } = useNodeConfig<GateConfig>(() => props.node);

const custom = computed(() => (cfg.value.decisions?.length ?? 0) > 0);
const shown = computed(() => (custom.value ? cfg.value.decisions : defaultGateDecisions({
  approve: t("workflow.config.gate.approve"),
  changes: t("workflow.config.gate.changes"),
  reject: t("workflow.config.gate.reject"),
})));
const formText = ref("");
const formError = ref("");
watch(() => props.node.id, () => {
  formText.value = cfg.value.form ? JSON.stringify(cfg.value.form, null, 2) : "";
  formError.value = "";
}, { immediate: true });

function customize() {
  update((config) => { config.decisions = JSON.parse(JSON.stringify(shown.value)); });
}

function patchDecision(index: number, change: Record<string, unknown>) {
  update((config, node, def) => {
    const item = config.decisions[index];
    if (typeof change.port === "string") renameSourcePort(def, node.id, item.port, change.port);
    Object.assign(item, change);
    if (item.primary === false) delete item.primary;
    if (item.requireComment === false) delete item.requireComment;
  }, `cfg:${props.node.id}:dec${index}`);
}

function setPrimary(index: number, value: boolean) {
  update((config) => {
    config.decisions.forEach((item, i) => {
      if (i === index && value) item.primary = true;
      else delete item.primary;
    });
  });
}

function addDecision() {
  update((config) => {
    const used = new Set(config.decisions.map((item) => item.port));
    let name = "option";
    let index = 2;
    while (used.has(name)) name = `option_${index++}`;
    config.decisions.push({ port: name, label: name });
  });
}

function removeDecision(index: number) {
  update((config) => { config.decisions.splice(index, 1); });
}

function applyForm() {
  const parsed = parseJsonText(formText.value);
  if (parsed instanceof Error) {
    formError.value = parsed.message;
    return;
  }
  formError.value = "";
  set({ form: parsed });
}

function toggleSlot(key: string, on: boolean) {
  const list = new Set(cfg.value.showDiffOf ?? []);
  if (on) list.add(key);
  else list.delete(key);
  set({ showDiffOf: [...list] });
}

function setTimeout_(value: unknown) {
  const minutes = Number(value);
  store.updateNode(props.node.id, (node) => {
    if (minutes > 0) node.timeoutMinutes = minutes;
    else delete node.timeoutMinutes;
  });
}
</script>

<template>
  <div class="wf-form">
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.gate.title") }}</span>
      <TemplateField :model-value="cfg.title ?? ''" :node-id="node.id" @update:model-value="set({ title: $event })" />
    </div>
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.gate.body") }}</span>
      <TemplateField :model-value="cfg.body ?? ''" :node-id="node.id" multiline :rows="6" @update:model-value="set({ body: $event })" />
    </div>
    <div class="wf-section-title">
      {{ t("workflow.config.gate.decisions") }}
      <UiButton v-if="!custom" size="small" @click="customize">{{ t("workflow.config.gate.customize") }}</UiButton>
    </div>
    <div v-for="(item, index) in shown" :key="index" class="wf-card">
      <div class="wf-row">
        <input class="ui-input wf-mono" style="width: 34%" :class="{ 'is-error': !isValidPortName(item.port) }" :value="item.port" :disabled="!custom" @change="patchDecision(index, { port: ($event.target as HTMLInputElement).value })">
        <input class="ui-input grow-1" :value="item.label" :disabled="!custom" @input="patchDecision(index, { label: ($event.target as HTMLInputElement).value })">
        <UiButton v-if="custom" variant="ghost" size="icon" :tip="t('common.delete')" @click="removeDecision(index)"><Trash2 :size="13" /></UiButton>
      </div>
      <div class="wf-row">
        <label class="wf-check"><input type="checkbox" class="ui-checkbox" :checked="Boolean(item.primary)" :disabled="!custom" @change="setPrimary(index, ($event.target as HTMLInputElement).checked)"> {{ t("workflow.config.gate.primary") }}</label>
        <label class="wf-check"><input type="checkbox" class="ui-checkbox" :checked="Boolean(item.requireComment)" :disabled="!custom" @change="patchDecision(index, { requireComment: ($event.target as HTMLInputElement).checked })"> {{ t("workflow.config.gate.requireComment") }}</label>
      </div>
    </div>
    <div v-if="custom"><UiButton size="small" @click="addDecision"><Plus :size="14" /> {{ t("workflow.config.gate.addDecision") }}</UiButton></div>
    <p class="wf-hint">{{ t("workflow.config.gate.decisionsHint") }}</p>
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.gate.form") }}</span>
      <textarea v-model="formText" class="ui-textarea wf-textarea" rows="5" spellcheck="false" :placeholder="t('workflow.config.gate.formPlaceholder')" @blur="applyForm" />
      <span v-if="formError" class="wf-error">{{ formError }}</span>
    </label>
    <div v-if="store.draft?.workspaces.length" class="wf-field">
      <span class="wf-label">{{ t("workflow.config.gate.showDiff") }}</span>
      <label v-for="slot in store.draft.workspaces" :key="slot.key" class="wf-check">
        <input type="checkbox" class="ui-checkbox" :checked="(cfg.showDiffOf ?? []).includes(slot.key)" @change="toggleSlot(slot.key, ($event.target as HTMLInputElement).checked)">
        <span class="wf-mono">{{ slot.key }}</span>
      </label>
    </div>
    <label class="wf-check">
      <input type="checkbox" class="ui-checkbox" :checked="Boolean(cfg.notify?.feishu)" @change="set({ notify: { feishu: ($event.target as HTMLInputElement).checked } })">
      {{ t("workflow.config.gate.notifyFeishu") }}
    </label>
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.gate.timeout") }}</span>
        <UiInput type="number" min="0" :model-value="node.timeoutMinutes ?? ''" :placeholder="t('workflow.config.gate.noTimeout')" @update:model-value="setTimeout_" />
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.gate.onTimeout") }}</span>
        <UiSelect :model-value="cfg.onTimeout ?? 'fail'" :options="['approve', 'reject', 'fail'].map((value) => ({ value, label: t(`workflow.config.gate.timeout.${value}`) }))" @update:model-value="set({ onTimeout: $event })" />
      </label>
    </div>
  </div>
</template>
