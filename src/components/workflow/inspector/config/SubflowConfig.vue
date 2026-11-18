<script setup lang="ts">
import { computed } from "vue";
import { RefreshCw } from "@lucide/vue";
import type { NodeDef, SubflowConfig } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import KeyValueEditor from "../KeyValueEditor.vue";
import { useChildWorkflow } from "./child-workflow";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { store, cfg, update, set } = useNodeConfig<SubflowConfig>(() => props.node);
const child = useChildWorkflow(computed(() => cfg.value.workflowId ?? ""));

const versionOptions = computed(() => [
  { value: "latest", label: t("workflow.config.subflow.latest") },
  ...child.versions.value.map((item) => ({ value: String(item.version), label: `v${item.version}` })),
]);

/** 子流程的入参 / 出参同步成本节点端口 */
function syncPorts() {
  update((_config, node) => {
    node.inputs = child.inputs.value.map((port) => ({ ...JSON.parse(JSON.stringify(port)), primary: undefined }));
    node.outputs = child.outputs.value.map((port) => JSON.parse(JSON.stringify(port)));
  });
}

async function pick(id: string) {
  update((config) => {
    config.workflowId = id;
    config.version = null;
    config.inputs = {};
  });
  await child.reload();
  syncPorts();
  const name = child.options.value.find((item) => item.value === id)?.label;
  if (name && !props.node.title) store.updateNode(props.node.id, (node) => { node.title = name; });
}

function setVersion(value: string) {
  set({ version: value === "latest" ? null : Number(value) });
}
</script>

<template>
  <div class="wf-form">
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.subflow.workflow") }}</span>
      <UiSelect :model-value="cfg.workflowId ?? ''" :options="child.options.value" :placeholder="t('workflow.config.subflow.pick')" @update:model-value="pick(String($event))" />
      <span class="wf-hint">{{ t("workflow.config.subflow.publishedOnly") }}</span>
    </label>
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.subflow.version") }}</span>
      <UiSelect :model-value="cfg.version == null ? 'latest' : String(cfg.version)" :options="versionOptions" @update:model-value="setVersion(String($event))" />
    </label>
    <p v-if="child.error.value" class="wf-error">{{ child.error.value }}</p>
    <div v-if="cfg.workflowId" class="wf-section">
      <div class="wf-section-title">
        {{ t("workflow.config.subflow.inputs") }}
        <UiButton size="small" variant="ghost" :disabled="child.loading.value" @click="child.reload().then(syncPorts)"><RefreshCw :size="12" /> {{ t("workflow.config.subflow.sync") }}</UiButton>
      </div>
      <p class="wf-hint">{{ t("workflow.config.subflow.inputsHint") }}</p>
      <KeyValueEditor
        :model-value="cfg.inputs ?? {}"
        :node-id="node.id"
        :fixed-keys="child.inputs.value.map((port) => port.name)"
        @update:model-value="set({ inputs: $event })"
      />
      <div v-if="!child.inputs.value.length && !child.loading.value" class="wf-empty">{{ t("workflow.config.subflow.noInputs") }}</div>
    </div>
  </div>
</template>
