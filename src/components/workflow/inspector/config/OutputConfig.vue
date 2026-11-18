<script setup lang="ts">
import type { NodeDef } from "../../../../domain/workflow";
import { PORT_COLORS } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { store } = useNodeConfig<Record<string, never>>(() => props.node);

function bound(name: string) {
  return props.node.inputs.some((port) => port.name === name);
}

function toggle(name: string, on: boolean) {
  store.updateNode(props.node.id, (node, def) => {
    const source = def.outputs.find((port) => port.name === name);
    if (!source) return;
    if (on && !node.inputs.some((port) => port.name === name)) node.inputs.push({ name, type: source.type });
    if (!on) node.inputs = node.inputs.filter((port) => port.name !== name);
  });
}
</script>

<template>
  <div class="wf-form">
    <p class="wf-hint">{{ t("workflow.config.output.hint") }}</p>
    <div v-if="!store.draft?.outputs.length" class="wf-empty">{{ t("workflow.config.output.noOutputs") }}</div>
    <label v-for="port in store.draft?.outputs ?? []" :key="port.name" class="wf-check">
      <input type="checkbox" class="ui-checkbox" :checked="bound(port.name)" @change="toggle(port.name, ($event.target as HTMLInputElement).checked)">
      <span class="wf-mono">{{ port.name }}</span>
      <em :style="{ color: PORT_COLORS[port.type], fontStyle: 'normal' }">{{ port.type }}</em>
    </label>
    <p class="wf-hint">{{ t("workflow.config.output.manageHint") }}</p>
  </div>
</template>
