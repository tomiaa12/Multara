<script setup lang="ts">
import type { NodeDef, PortDef, TriggerConfig, WorkflowSettings } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import PortListEditor from "../PortListEditor.vue";
import { renamedPort, renameSourcePort, useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { store, cfg, set } = useNodeConfig<TriggerConfig>(() => props.node);
const sources = ["manual", "automation", "webhook", "channel", "cli"];

function toggle(source: string, on: boolean) {
  const list = new Set(cfg.value.sources ?? []);
  if (on) list.add(source);
  else list.delete(source);
  set({ sources: sources.filter((item) => list.has(item)) });
}

function setInputs(ports: PortDef[]) {
  store.mutate((def) => {
    const trigger = def.nodes.find((node) => node.kind === "trigger");
    const renamed = renamedPort(def.inputs, ports);
    if (trigger && renamed) renameSourcePort(def, trigger.id, renamed.from, renamed.to);
    def.inputs = ports;
  }, "wf-inputs");
}

function patchSettings(change: Partial<WorkflowSettings>) {
  store.mutate((def) => {
    Object.assign(def.settings, change);
    if (!def.settings.runTimeoutMinutes) delete def.settings.runTimeoutMinutes;
  }, `settings:${Object.keys(change).join(",")}`);
}
</script>

<template>
  <div class="wf-form">
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.trigger.sources") }}</span>
      <label v-for="source in sources" :key="source" class="wf-check source">
        <input type="checkbox" class="ui-checkbox" :checked="(cfg.sources ?? []).includes(source)" @change="toggle(source, ($event.target as HTMLInputElement).checked)">
        <span class="source-copy">
          <span>{{ t(`workflow.config.trigger.source.${source}`) }}</span>
          <span class="wf-hint">{{ t(`workflow.config.trigger.sourceHint.${source}`) }}</span>
        </span>
      </label>
      <span class="wf-hint">{{ t("workflow.config.trigger.sourcesHint") }}</span>
    </div>
    <div class="wf-section">
      <div class="wf-section-title">{{ t("workflow.config.trigger.inputs") }}</div>
      <p class="wf-hint">{{ t("workflow.config.trigger.inputsHint") }}</p>
      <PortListEditor :model-value="store.draft?.inputs ?? []" mode="workflow-input" :show-primary="(cfg.sources ?? []).includes('channel')" @update:model-value="setInputs" />
    </div>
    <div v-if="store.draft" class="wf-section">
      <div class="wf-section-title">{{ t("workflow.settings.general") }}</div>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.settings.runTimeout") }}</span>
        <UiInput type="number" min="0" :model-value="store.draft.settings.runTimeoutMinutes ?? ''" :placeholder="t('workflow.settings.noLimit')" @update:model-value="patchSettings({ runTimeoutMinutes: Number($event) || undefined })" />
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.settings.concurrency") }}</span>
        <UiSelect :model-value="store.draft.settings.concurrency === 'queue' ? 'allow' : store.draft.settings.concurrency" :options="['allow', 'skip_if_running'].map((value) => ({ value, label: t(`workflow.settings.concurrency.${value}`) }))" @update:model-value="patchSettings({ concurrency: $event as WorkflowSettings['concurrency'] })" />
      </label>
    </div>
  </div>
</template>

<style scoped>
.source { align-items: flex-start; }
.source-copy { display: grid; gap: 1px; min-width: 0; }
.source-copy .wf-hint { margin: 0; }
</style>
