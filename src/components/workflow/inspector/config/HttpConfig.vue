<script setup lang="ts">
import type { HttpConfig, NodeDef } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import KeyValueEditor from "../KeyValueEditor.vue";
import TemplateField from "../TemplateField.vue";
import { parseNumberList, useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { cfg, set } = useNodeConfig<HttpConfig>(() => props.node);
const methods = ["GET", "POST", "PUT", "PATCH", "DELETE"].map((value) => ({ value, label: value }));
</script>

<template>
  <div class="wf-form">
    <div class="wf-row">
      <UiSelect style="width: 104px; flex: none" :model-value="cfg.method ?? 'GET'" :options="methods" @update:model-value="set({ method: $event })" />
      <TemplateField class="grow-1" :model-value="cfg.url ?? ''" :node-id="node.id" placeholder="https://api.example.com/{{ inputs.id }}" @update:model-value="set({ url: $event })" />
    </div>
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.http.headers") }}</span>
      <KeyValueEditor :model-value="cfg.headers ?? {}" :node-id="node.id" key-placeholder="Header" value-placeholder='{{ secret("TOKEN") }}' @update:model-value="set({ headers: $event })" />
    </div>
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.http.bodyType") }}</span>
      <UiSelect :model-value="cfg.bodyType ?? 'none'" :options="['none', 'json', 'text', 'form'].map((value) => ({ value, label: value }))" @update:model-value="set({ bodyType: $event })" />
    </label>
    <div v-if="(cfg.bodyType ?? 'none') !== 'none'" class="wf-field">
      <span class="wf-label">{{ t("workflow.config.http.body") }}</span>
      <TemplateField :model-value="cfg.body ?? ''" :node-id="node.id" multiline :rows="5" @update:model-value="set({ body: $event })" />
    </div>
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.http.expectStatus") }}</span>
        <input class="ui-input wf-mono" :value="(cfg.expectStatus ?? []).join(', ')" placeholder="200, 201" @change="set({ expectStatus: parseNumberList(($event.target as HTMLInputElement).value) })">
        <span class="wf-hint">{{ t("workflow.config.http.expectHint") }}</span>
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.http.responseType") }}</span>
        <UiSelect :model-value="cfg.responseType ?? 'json'" :options="['json', 'text'].map((value) => ({ value, label: value }))" @update:model-value="set({ responseType: $event })" />
      </label>
    </div>
  </div>
</template>
