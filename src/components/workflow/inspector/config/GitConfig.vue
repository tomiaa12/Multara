<script setup lang="ts">
import type { GitConfig, NodeDef } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import TemplateField from "../TemplateField.vue";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { cfg, set, slotOptions } = useNodeConfig<GitConfig>(() => props.node);
const ops = ["commit", "squash", "push", "merge", "create_branch", "diff"].map((value) => ({ value, label: t(`workflow.config.git.op.${value}`) }));
</script>

<template>
  <div class="wf-form">
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.git.slot") }}</span>
        <UiSelect :model-value="cfg.slot ?? ''" :options="slotOptions" :placeholder="t('workflow.config.git.slotPlaceholder')" @update:model-value="set({ slot: $event })" />
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.git.operation") }}</span>
        <UiSelect :model-value="cfg.op ?? 'commit'" :options="ops" @update:model-value="set({ op: $event })" />
      </label>
    </div>
    <div v-if="cfg.op === 'commit' || cfg.op === 'squash' || !cfg.op" class="wf-field">
      <span class="wf-label">{{ t("workflow.config.git.message") }}</span>
      <TemplateField :model-value="cfg.message ?? ''" :node-id="node.id" multiline :rows="3" @update:model-value="set({ message: $event })" />
    </div>
    <div v-if="cfg.op === 'merge' || cfg.op === 'create_branch'" class="wf-field">
      <span class="wf-label">{{ cfg.op === "merge" ? t("workflow.config.git.mergeBranch") : t("workflow.config.git.newBranch") }}</span>
      <TemplateField :model-value="cfg.branch ?? ''" :node-id="node.id" placeholder="{{ nodes.code.outputs.branch }}" @update:model-value="set({ branch: $event })" />
    </div>
    <template v-if="cfg.op === 'push'">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.git.remote") }}</span>
        <UiInput :model-value="cfg.remote ?? ''" placeholder="origin" @update:model-value="set({ remote: String($event) })" />
      </label>
      <label class="wf-check">
        <input type="checkbox" class="ui-checkbox" :checked="Boolean(cfg.setUpstream)" @change="set({ setUpstream: ($event.target as HTMLInputElement).checked })">
        {{ t("workflow.config.git.setUpstream") }}
      </label>
    </template>
    <label v-if="cfg.op !== 'diff' && cfg.op !== 'create_branch'" class="wf-check">
      <input type="checkbox" class="ui-checkbox" :checked="Boolean(cfg.skipVerify)" @change="set({ skipVerify: ($event.target as HTMLInputElement).checked })">
      {{ t("workflow.config.git.skipVerify") }}
    </label>
  </div>
</template>
