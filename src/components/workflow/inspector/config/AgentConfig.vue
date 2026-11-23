<script setup lang="ts">
import { computed } from "vue";
import type { AgentConfig, NodeDef } from "../../../../domain/workflow";
import { t } from "../../../../i18n";
import { useNodeConfig } from "./use-node-config";
import "../inspector.css";

const props = defineProps<{ node: NodeDef }>();
const { app, store, cfg, set, update } = useNodeConfig<AgentConfig>(() => props.node);

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
const severalWays = computed(() => (store.draft?.edges.filter((edge) => edge.source === props.node.id).length ?? 0) > 1);

function setTarget(value: string) {
  const [type, id] = value.split(":", 2);
  update((config) => { config.target = { type: type as "agent" | "squad", id }; });
}
</script>

<template>
  <div class="wf-form">
    <label class="wf-field" data-field="config:target">
      <span class="wf-label">{{ t("workflow.config.agent.target") }}</span>
      <UiSelect :model-value="target" :groups="groups" :placeholder="t('automation.targetPlaceholder')" @update:model-value="setTarget(String($event))" />
    </label>
    <label class="wf-field" data-field="config:prompt">
      <span class="wf-label">{{ t("workflow.config.agent.prompt") }}</span>
      <UiTextarea :model-value="cfg.prompt ?? ''" :rows="6" :placeholder="t('workflow.config.agent.promptPlaceholder')" @update:model-value="set({ prompt: String($event) })" />
    </label>
    <label class="wf-field" data-field="config:context">
      <span class="wf-label">{{ t("workflow.config.agent.context") }}</span>
      <UiSelect
        :model-value="cfg.context ?? 'previous'"
        :options="[
          { value: 'previous', label: t('workflow.config.agent.contextPrevious') },
          { value: 'all', label: t('workflow.config.agent.contextAll') },
        ]"
        @update:model-value="set({ context: $event })"
      />
    </label>
    <label v-if="severalWays" class="wf-field" data-field="config:route">
      <span class="wf-label">{{ t("workflow.config.agent.route") }}</span>
      <UiSelect
        :model-value="cfg.route ?? 'auto'"
        :options="[
          { value: 'all', label: t('workflow.config.agent.routeAll') },
          { value: 'auto', label: t('workflow.config.agent.routeAuto') },
          { value: 'human', label: t('workflow.config.agent.routeHuman') },
        ]"
        @update:model-value="set({ route: $event })"
      />
      <span class="wf-hint">{{ t("workflow.config.agent.routeHint") }}</span>
    </label>
    <div class="wf-confirm" data-field="config:humanConfirm">
      <label class="wf-check">
        <input type="checkbox" class="ui-checkbox" :checked="Boolean(cfg.humanConfirm)" @change="set({ humanConfirm: ($event.target as HTMLInputElement).checked })">
        {{ t("workflow.config.agent.humanConfirm") }}
      </label>
      <p class="wf-hint">{{ t("workflow.config.agent.humanConfirmHint") }}</p>
    </div>
    <label class="wf-check">
      <input type="checkbox" class="ui-checkbox" :checked="Boolean(cfg.allowTaskCommands)" @change="set({ allowTaskCommands: ($event.target as HTMLInputElement).checked })">
      {{ t("workflow.config.agent.allowTasks") }}
    </label>
  </div>
</template>
