<script setup lang="ts">
import { Plus, Trash2 } from "@lucide/vue";
import type { NodeDef, TransformConfig } from "../../../../domain/workflow";
import { isValidPortName } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import TemplateField from "../TemplateField.vue";
import { renameSourcePort, useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { cfg, update } = useNodeConfig<TransformConfig>(() => props.node);

function assignments() {
  return cfg.value.assignments ?? [];
}

function add() {
  update((config, node) => {
    const used = new Set((config.assignments ??= []).map((item) => item.port));
    let name = "result";
    let index = 2;
    while (used.has(name)) name = `result_${index++}`;
    config.assignments.push({ port: name, expr: "" });
    if (!node.outputs.some((port) => port.name === name)) node.outputs.push({ name, type: "any" });
  });
}

function rename(index: number, name: string) {
  update((config, node, def) => {
    const item = config.assignments[index];
    const previous = item.port;
    item.port = name;
    const output = node.outputs.find((port) => port.name === previous);
    if (output) output.name = name;
    else if (!node.outputs.some((port) => port.name === name)) node.outputs.push({ name, type: "any" });
    renameSourcePort(def, node.id, previous, name);
  }, `cfg:${props.node.id}:tf${index}`);
}

function setExpr(index: number, expr: string) {
  update((config) => { config.assignments[index].expr = expr; }, `cfg:${props.node.id}:tfx${index}`);
}

function remove(index: number) {
  update((config, node) => {
    const [gone] = config.assignments.splice(index, 1);
    node.outputs = node.outputs.filter((port) => port.name !== gone.port);
  });
}
</script>

<template>
  <div class="wf-form">
    <p class="wf-hint">{{ t("workflow.config.transform.hint") }}</p>
    <div v-if="!assignments().length" class="wf-empty">{{ t("workflow.config.transform.empty") }}</div>
    <div v-for="(item, index) in assignments()" :key="index" class="wf-card">
      <div class="wf-row">
        <input class="ui-input wf-mono grow-1" :class="{ 'is-error': !isValidPortName(item.port) }" :value="item.port" @change="rename(index, ($event.target as HTMLInputElement).value)">
        <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(index)"><Trash2 :size="13" /></UiButton>
      </div>
      <TemplateField :model-value="item.expr" :node-id="node.id" expression multiline :rows="2" placeholder="inputs.value | tojson" @update:model-value="setExpr(index, $event)" />
    </div>
    <div><UiButton size="small" @click="add"><Plus :size="14" /> {{ t("workflow.config.transform.add") }}</UiButton></div>
  </div>
</template>
