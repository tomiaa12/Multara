<script setup lang="ts">
import { computed } from "vue";
import { Plus, Trash2 } from "@lucide/vue";
import { defaultGateDecisions, isValidPortName, type GateConfig, type NodeDef } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { store, cfg, set, update } = useNodeConfig<GateConfig>(() => props.node);

const custom = computed(() => (cfg.value.decisions?.length ?? 0) > 0);
const shown = computed(() => (custom.value ? cfg.value.decisions : defaultGateDecisions({
  approve: t("workflow.config.gate.approve"),
  changes: t("workflow.config.gate.changes"),
  reject: t("workflow.config.gate.reject"),
})));
function customize() {
  update((config) => { config.decisions = JSON.parse(JSON.stringify(shown.value)); });
}

function patchDecision(index: number, change: Record<string, unknown>) {
  update((config) => {
    const item = config.decisions[index];
    if (typeof change.id === "string") item.id = change.id;
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
    const used = new Set(config.decisions.map((item) => item.id));
    let name = "option";
    let index = 2;
    while (used.has(name)) name = `option_${index++}`;
    config.decisions.push({ id: name, label: name });
  });
}

function removeDecision(index: number) {
  update((config) => { config.decisions.splice(index, 1); });
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
    <label class="wf-field" data-field="config:title">
      <span class="wf-label">{{ t("workflow.config.gate.title") }}</span>
      <UiInput :model-value="cfg.title ?? ''" @update:model-value="set({ title: String($event) })" />
    </label>
    <label class="wf-field" data-field="config:body">
      <span class="wf-label">{{ t("workflow.config.gate.body") }}</span>
      <UiTextarea :model-value="cfg.body ?? ''" :rows="4" @update:model-value="set({ body: String($event) })" />
    </label>
    <div class="wf-section-title">
      {{ t("workflow.config.gate.decisions") }}
      <UiButton v-if="!custom" size="small" @click="customize">{{ t("workflow.config.gate.customize") }}</UiButton>
    </div>
    <div v-for="(item, index) in shown" :key="index" class="wf-card">
      <div class="wf-row">
        <input
          class="ui-input wf-mono grow-1"
          :class="{ 'is-error': !isValidPortName(item.id) }"
          :value="item.id"
          :disabled="!custom"
          :placeholder="t('workflow.field.name')"
          spellcheck="false"
          @change="patchDecision(index, { id: ($event.target as HTMLInputElement).value })"
        >
        <UiButton v-if="custom" variant="ghost" size="icon" :tip="t('common.delete')" @click="removeDecision(index)"><Trash2 :size="13" /></UiButton>
      </div>
      <p v-if="!isValidPortName(item.id)" class="wf-error">{{ t("workflow.port.nameInvalid") }}</p>
      <input
        class="ui-input"
        :value="item.label"
        :disabled="!custom"
        :placeholder="t('workflow.port.label')"
        @input="patchDecision(index, { label: ($event.target as HTMLInputElement).value })"
      >
      <div class="wf-row">
        <label class="wf-check"><input type="checkbox" class="ui-checkbox" :checked="Boolean(item.primary)" :disabled="!custom" @change="setPrimary(index, ($event.target as HTMLInputElement).checked)"> {{ t("workflow.config.gate.primary") }}</label>
        <label class="wf-check"><input type="checkbox" class="ui-checkbox" :checked="Boolean(item.requireComment)" :disabled="!custom" @change="patchDecision(index, { requireComment: ($event.target as HTMLInputElement).checked })"> {{ t("workflow.config.gate.requireComment") }}</label>
      </div>
    </div>
    <div v-if="custom"><UiButton size="small" @click="addDecision"><Plus :size="14" /> {{ t("workflow.config.gate.addDecision") }}</UiButton></div>
    <p class="wf-hint">{{ t("workflow.config.gate.decisionsHint") }}</p>
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
