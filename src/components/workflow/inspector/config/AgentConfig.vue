<script setup lang="ts">
import { computed } from "vue";
import type { AgentConfig, NodeDef } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import TemplateField from "../TemplateField.vue";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { app, cfg, set, update } = useNodeConfig<AgentConfig>(() => props.node);

const groups = computed(() => {
  const data = app.data;
  const agents = (data?.agents ?? []).map((agent) => ({ value: `agent:${agent.id}`, label: agent.name, avatar: agent.avatar }));
  const squads = (data?.squads ?? []).map((squad) => ({
    value: `squad:${squad.id}`,
    label: squad.name,
    avatar: data?.agents.find((agent) => agent.id === squad.leaderAgentId)?.avatar ?? "◉",
  }));
  return [
    { label: t("automation.group.agent"), options: agents },
    { label: t("automation.group.squad"), options: squads },
  ].filter((group) => group.options.length);
});

const target = computed(() => (cfg.value.target?.id ? `${cfg.value.target.type}:${cfg.value.target.id}` : ""));
const textPorts = computed(() => [
  { value: "", label: t("workflow.config.agent.noFallback") },
  ...props.node.outputs
    .filter((port) => port.type === "text" || port.type === "markdown")
    .map((port) => ({ value: port.name, label: port.name })),
]);

function setTarget(value: string) {
  const [type, id] = value.split(":", 2);
  update((config) => { config.target = { type: type as "agent" | "squad", id }; });
}
</script>

<template>
  <div class="wf-form">
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.agent.target") }}</span>
      <UiSelect :model-value="target" :groups="groups" :placeholder="t('automation.targetPlaceholder')" @update:model-value="setTarget(String($event))" />
    </label>
    <div class="wf-field">
      <span class="wf-label">{{ t("workflow.config.agent.prompt") }}</span>
      <TemplateField :model-value="cfg.prompt ?? ''" :node-id="node.id" multiline :rows="8" :placeholder="t('workflow.config.agent.promptPlaceholder')" @update:model-value="set({ prompt: $event })" />
    </div>
    <div class="wf-grid-2">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.agent.mode") }}</span>
        <UiSelect :model-value="cfg.mode ?? 'auto'" :options="[{ value: 'auto', label: t('workflow.config.agent.modeAuto') }, { value: 'plan', label: t('workflow.config.agent.modePlan') }]" @update:model-value="set({ mode: $event })" />
      </label>
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.config.agent.repairTurns") }}</span>
        <UiInput type="number" min="0" max="10" :model-value="cfg.maxRepairTurns ?? 2" @update:model-value="set({ maxRepairTurns: Math.max(0, Number($event) || 0) })" />
      </label>
    </div>
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.agent.continueSession") }}</span>
      <UiSelect :model-value="cfg.continueSession ?? 'same_node'" :options="[{ value: 'same_node', label: t('workflow.config.agent.sameNode') }, { value: 'never', label: t('workflow.config.agent.never') }]" @update:model-value="set({ continueSession: $event })" />
    </label>
    <label class="wf-field">
      <span class="wf-label">{{ t("workflow.config.agent.fallbackPort") }}</span>
      <UiSelect :model-value="cfg.fallbackTextPort ?? ''" :options="textPorts" @update:model-value="set({ fallbackTextPort: $event })" />
      <span class="wf-hint">{{ t("workflow.config.agent.fallbackHint") }}</span>
    </label>
    <label class="wf-check">
      <input type="checkbox" class="ui-checkbox" :checked="Boolean(cfg.allowTaskCommands)" @change="set({ allowTaskCommands: ($event.target as HTMLInputElement).checked })">
      {{ t("workflow.config.agent.allowTasks") }}
    </label>
  </div>
</template>
