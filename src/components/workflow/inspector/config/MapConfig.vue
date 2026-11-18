<script setup lang="ts">
import { computed } from "vue";
import type { MapConfig, NodeDef } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import KeyValueEditor from "../KeyValueEditor.vue";
import TemplateField from "../TemplateField.vue";
import { useChildWorkflow } from "./child-workflow";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { cfg, update, set } = useNodeConfig<MapConfig>(() => props.node);
const child = useChildWorkflow(computed(() => cfg.value.workflowId ?? ""));

const versionOptions = computed(() => [
  { value: "latest", label: t("workflow.config.subflow.latest") },
  ...child.versions.value.map((item) => ({ value: String(item.version), label: `v${item.version}` })),
]);
const itemOptions = computed(() => child.inputs.value.map((port) => ({ value: port.name, label: port.name })));
const extraKeys = computed(() => child.inputs.value.map((port) => port.name).filter((name) => name !== cfg.value.itemInput));

function pick(id: string) {
  update((config) => {
    config.workflowId = id;
    config.version = null;
    config.itemInput = "";
    config.extraInputs = {};
  });
}

function setVersion(value: string) {
  set({ version: value === "latest" ? null : Number(value) });
}
</script>

<template>
  <div class="wf-form">
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.map.items") }}</span>
      <TemplateField :model-value="cfg.items ?? ''" :node-id="node.id" expression placeholder="inputs.items" @update:model-value="set({ items: $event })" />
      <span class="wf-hint">{{ t("workflow.config.map.itemsHint") }}</span>
    </div>
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.subflow.workflow") }}</span>
      <UiSelect :model-value="cfg.workflowId ?? ''" :options="child.options.value" :placeholder="t('workflow.config.subflow.pick')" @update:model-value="pick(String($event))" />
    </label>
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.subflow.version") }}</span>
        <UiSelect :model-value="cfg.version == null ? 'latest' : String(cfg.version)" :options="versionOptions" @update:model-value="setVersion(String($event))" />
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.map.itemInput") }}</span>
        <UiSelect :model-value="cfg.itemInput ?? ''" :options="itemOptions" :placeholder="t('workflow.config.map.itemInputPlaceholder')" @update:model-value="set({ itemInput: $event })" />
      </label>
    </div>
    <p v-if="child.error.value" class="wf-error">{{ child.error.value }}</p>
    <div v-if="extraKeys.length" class="wf-field">
      <span class="wf-label">{{ t("workflow.config.map.extraInputs") }}</span>
      <KeyValueEditor :model-value="cfg.extraInputs ?? {}" :node-id="node.id" :fixed-keys="extraKeys" @update:model-value="set({ extraInputs: $event })" />
    </div>
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.map.concurrency") }}</span>
        <UiInput type="number" min="1" max="16" :model-value="cfg.concurrency ?? 2" @update:model-value="set({ concurrency: Math.max(1, Number($event) || 1) })" />
      </label>
      <label class="wf-check" style="align-self: end; min-height: 32px">
        <input type="checkbox" class="ui-checkbox" :checked="Boolean(cfg.failFast)" @change="set({ failFast: ($event.target as HTMLInputElement).checked })">
        {{ t("workflow.config.map.failFast") }}
      </label>
    </div>
    <p class="wf-hint">{{ t("workflow.config.map.outputsHint") }}</p>
  </div>
</template>
