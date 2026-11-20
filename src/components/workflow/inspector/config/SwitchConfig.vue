<script setup lang="ts">
import { ArrowDown, ArrowUp, Plus, Trash2 } from "@lucide/vue";
import type { NodeDef, SwitchConfig } from "../../../../domain/workflow";
import { isValidPortName } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import TemplateField from "../TemplateField.vue";
import { renameSourcePort, useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { cfg, set, update } = useNodeConfig<SwitchConfig>(() => props.node);

function cases() {
  return cfg.value.cases ?? [];
}

function add() {
  update((config) => {
    const list = (config.cases ??= []);
    const used = new Set(list.map((item) => item.port));
    let name = `case_${list.length + 1}`;
    let index = list.length + 2;
    while (used.has(name)) name = `case_${index++}`;
    list.push({ port: name, when: "" });
  });
}

function rename(index: number, name: string) {
  update((config, node, def) => {
    const previous = config.cases[index].port;
    config.cases[index].port = name;
    renameSourcePort(def, node.id, previous, name);
  });
}

function setWhen(index: number, when: string) {
  update((config) => { config.cases[index].when = when; }, `cfg:${props.node.id}:when${index}`);
}

function move(index: number, delta: number) {
  update((config) => {
    const target = index + delta;
    if (target < 0 || target >= config.cases.length) return;
    [config.cases[index], config.cases[target]] = [config.cases[target], config.cases[index]];
  });
}

function remove(index: number) {
  update((config) => { config.cases.splice(index, 1); });
}

function setDefault(name: string) {
  update((config, node, def) => {
    renameSourcePort(def, node.id, config.defaultPort ?? "", name);
    if (name) config.defaultPort = name;
    else delete config.defaultPort;
  });
}
</script>

<template>
  <div class="wf-form">
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.switch.value") }}</span>
      <TemplateField :model-value="cfg.value ?? ''" :node-id="node.id" expression placeholder="inputs.value" @update:model-value="set({ value: $event })" />
      <span class="wf-hint">{{ t("workflow.config.switch.valueHint") }}</span>
    </div>
    <div class="wf-section-title">{{ t("workflow.config.switch.cases") }}</div>
    <div v-for="(item, index) in cases()" :key="index" class="wf-card">
      <div class="wf-row">
        <input class="ui-input wf-mono grow-1" :class="{ 'is-error': !isValidPortName(item.port) }" :value="item.port" @change="rename(index, ($event.target as HTMLInputElement).value)">
        <UiButton variant="ghost" size="icon" :tip="t('workflow.port.moveUp')" :disabled="index === 0" @click="move(index, -1)"><ArrowUp :size="13" /></UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('workflow.port.moveDown')" :disabled="index === cases().length - 1" @click="move(index, 1)"><ArrowDown :size="13" /></UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(index)"><Trash2 :size="13" /></UiButton>
      </div>
      <TemplateField :model-value="item.when" :node-id="node.id" expression placeholder="inputs.value.exit_code == 0" @update:model-value="setWhen(index, $event)" />
    </div>
    <div><UiButton size="small" @click="add"><Plus :size="14" /> {{ t("workflow.config.switch.addCase") }}</UiButton></div>
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.switch.defaultPort") }}</span>
        <input class="ui-input wf-mono" :value="cfg.defaultPort ?? ''" :placeholder="t('workflow.config.switch.noDefault')" @change="setDefault(($event.target as HTMLInputElement).value)">
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.switch.mode") }}</span>
        <UiSelect :model-value="cfg.mode ?? 'first'" :options="[{ value: 'first', label: t('workflow.config.switch.first') }, { value: 'all', label: t('workflow.config.switch.all') }]" @update:model-value="set({ mode: $event })" />
      </label>
    </div>
    <p class="wf-hint">{{ t("workflow.config.switch.exhaustedHint") }}</p>
  </div>
</template>
