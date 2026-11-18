<script setup lang="ts">
import type { NodeDef, NotifyConfig } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import TemplateField from "../TemplateField.vue";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { cfg, set } = useNodeConfig<NotifyConfig>(() => props.node);
</script>

<template>
  <div class="wf-form">
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.notify.channel") }}</span>
      <UiSelect :model-value="cfg.channel ?? 'inbox'" :options="[{ value: 'inbox', label: t('workflow.config.notify.inbox') }, { value: 'feishu', label: t('channels.feishu') }]" @update:model-value="set({ channel: $event })" />
    </label>
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.notify.text") }}</span>
      <TemplateField :model-value="cfg.text ?? ''" :node-id="node.id" multiline :rows="5" @update:model-value="set({ text: $event })" />
    </div>
    <p class="wf-hint">{{ t("workflow.config.notify.hint") }}</p>
  </div>
</template>
