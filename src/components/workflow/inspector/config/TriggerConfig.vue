<script setup lang="ts">
import type { NodeDef, PortDef, TriggerConfig } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import PortListEditor from "../PortListEditor.vue";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { store, cfg, set } = useNodeConfig<TriggerConfig>(() => props.node);
const sources = ["manual", "automation", "webhook", "channel", "subflow", "cli"];

function toggle(source: string, on: boolean) {
  const list = new Set(cfg.value.sources ?? []);
  if (on) list.add(source);
  else list.delete(source);
  set({ sources: sources.filter((item) => list.has(item)) });
}

function setInputs(ports: PortDef[]) {
  store.mutate((def) => { def.inputs = ports; }, "wf-inputs");
}
</script>

<template>
  <div class="wf-form">
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.trigger.sources") }}</span>
      <label v-for="source in sources" :key="source" class="wf-check">
        <input type="checkbox" class="ui-checkbox" :checked="(cfg.sources ?? []).includes(source)" @change="toggle(source, ($event.target as HTMLInputElement).checked)">
        {{ t(`workflow.config.trigger.source.${source}`) }}
      </label>
      <span class="wf-hint">{{ t("workflow.config.trigger.sourcesHint") }}</span>
    </div>
    <div class="wf-section">
      <div class="wf-section-title">{{ t("workflow.config.trigger.inputs") }}</div>
      <p class="wf-hint">{{ t("workflow.config.trigger.inputsHint") }}</p>
      <PortListEditor :model-value="store.draft?.inputs ?? []" mode="workflow-input" @update:model-value="setInputs" />
    </div>
  </div>
</template>
