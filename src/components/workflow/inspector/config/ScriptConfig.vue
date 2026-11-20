<script setup lang="ts">
import { computed } from "vue";
import type { NodeDef, ScriptConfig } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import KeyValueEditor from "../KeyValueEditor.vue";
import TemplateField from "../TemplateField.vue";
import { parseNumberList, useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { cfg, set, update, slotOptions } = useNodeConfig<ScriptConfig>(() => props.node);

const cwdSlot = computed(() => cfg.value.cwd?.slot ?? "");
const slots = computed(() => [{ value: "", label: t("workflow.config.script.tempDir") }, ...slotOptions.value]);

function setCwd(slot: string, subdir?: string) {
  update((config) => {
    config.cwd = slot ? { slot, ...(subdir ? { subdir } : {}) } : null;
  });
}
</script>

<template>
  <div class="wf-form">
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.script.command") }}</span>
      <TemplateField :model-value="cfg.command ?? ''" :node-id="node.id" multiline :rows="5" placeholder="pnpm test" @update:model-value="set({ command: $event })" />
      <span class="wf-hint">{{ t("workflow.config.script.commandHint") }}</span>
    </div>
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.script.shell") }}</span>
      <UiSelect :model-value="cfg.shell ?? 'default'" :options="['default', 'sh', 'bash', 'pwsh', 'cmd'].map((value) => ({ value, label: value }))" @update:model-value="set({ shell: $event })" />
    </label>
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.script.cwd") }}</span>
        <UiSelect :model-value="cwdSlot" :options="slots" @update:model-value="setCwd(String($event), cfg.cwd?.subdir)" />
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.script.subdir") }}</span>
        <UiInput :model-value="cfg.cwd?.subdir ?? ''" :disabled="!cwdSlot" @update:model-value="setCwd(cwdSlot, String($event))" />
      </label>
    </div>
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.script.env") }}</span>
      <KeyValueEditor :model-value="cfg.env ?? {}" :node-id="node.id" key-placeholder="NAME" value-placeholder='{{ secret("TOKEN") }}' @update:model-value="set({ env: $event })" />
    </div>
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.script.successCodes") }}</span>
        <input class="ui-input wf-mono" :value="(cfg.successExitCodes ?? [0]).join(', ')" @change="set({ successExitCodes: parseNumberList(($event.target as HTMLInputElement).value) })">
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.script.parseStdout") }}</span>
        <UiSelect :model-value="cfg.parseStdout ?? 'none'" :options="['none', 'text', 'json', 'lines'].map((value) => ({ value, label: t(`workflow.config.script.parse.${value}`) }))" @update:model-value="set({ parseStdout: $event })" />
      </label>
    </div>
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.script.maxOutput") }}</span>
      <UiInput type="number" min="1" :model-value="cfg.maxOutputKB ?? 256" @update:model-value="set({ maxOutputKB: Math.max(1, Number($event) || 256) })" />
    </label>
    <label class="wf-check">
      <input type="checkbox" class="ui-checkbox" :checked="cfg.failOnNonZero ?? true" @change="set({ failOnNonZero: ($event.target as HTMLInputElement).checked })">
      {{ t("workflow.config.script.failOnNonZero") }}
    </label>
    <p v-if="!(cfg.failOnNonZero ?? true)" class="wf-hint">{{ t("workflow.config.script.failOnNonZeroHint") }}</p>
  </div>
</template>
