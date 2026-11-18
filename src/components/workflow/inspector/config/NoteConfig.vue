<script setup lang="ts">
import type { NodeDef, NoteConfig } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { cfg, set } = useNodeConfig<NoteConfig>(() => props.node);
</script>

<template>
  <div class="wf-form">
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.note.text") }}</span>
      <UiTextarea class="wf-textarea" :model-value="cfg.text ?? ''" @update:model-value="set({ text: $event })" />
    </label>
    <p class="wf-hint">{{ t("workflow.config.note.hint") }}</p>
  </div>
</template>
