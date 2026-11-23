<script setup lang="ts">
import { computed } from "vue";
import AgentAvatar from "../AgentAvatar.vue";
import AgentLiveStatus from "../AgentLiveStatus.vue";
import { useAppStore } from "../../stores/app";
import { t } from "../../i18n";
import {
  type AgentConfig,
  type NodeDef,
  type TriggerConfig,
} from "../../domain/workflow";

function triggerSourceLabel(source: string) {
  const key = `workflow.config.trigger.source.${source}`;
  const text = t(key);
  return text === key ? source : text;
}

const props = defineProps<{ node: NodeDef; status?: string; sessionId?: string | null }>();
const app = useAppStore();

function firstLine(text: string | undefined, max = 60) {
  const line = (text ?? "").split("\n").find((item) => item.trim()) ?? "";
  return line.length > max ? `${line.slice(0, max)}…` : line;
}

const cfg = computed(() => (props.node.config ?? {}) as Record<string, unknown>);
const agent = computed(() => {
  if (props.node.kind !== "agent") return null;
  const config = cfg.value as unknown as AgentConfig;
  const target = config.target;
  if (!target?.id) return { name: "", avatar: "", mode: config.mode };
  if (target.type === "squad") {
    const squad = app.data?.squads.find((item) => item.id === target.id);
    const leader = app.data?.agents.find((item) => item.id === squad?.leaderAgentId);
    return { name: squad?.name ?? target.id, avatar: leader?.avatar ?? "◉", mode: config.mode };
  }
  const found = app.data?.agents.find((item) => item.id === target.id);
  return { name: found?.name ?? target.id, avatar: found?.avatar ?? "", mode: config.mode };
});
const needsConfirm = computed(() => props.node.kind === "agent" && Boolean((cfg.value as unknown as AgentConfig).humanConfirm));
</script>

<template>
  <div class="ns">
    <template v-if="node.kind === 'agent' && agent">
      <AgentLiveStatus
        v-if="status === 'running' && sessionId"
        :session-ids="[sessionId]"
        :fallback="[{ key: agent.name || 'agent', name: agent.name || t('workflow.canvas.noAgent'), image: agent.avatar }]"
        show-name
        :size="18"
      />
      <div v-else class="ns-agent">
        <AgentAvatar v-if="agent.name" :value="agent.avatar" :name="agent.name" :size="18" />
        <span class="ns-strong">{{ agent.name || t("workflow.canvas.noAgent") }}</span>
        <span v-if="agent.mode === 'plan'" class="ds-badge">{{ t("workflow.canvas.planMode") }}</span>
      </div>
      <div v-if="needsConfirm" class="ns-chips"><span class="ns-chip primary">{{ t("workflow.config.agent.humanConfirm") }}</span></div>
      <div v-if="status !== 'running' && (cfg as unknown as AgentConfig).prompt" class="ns-text">{{ firstLine((cfg as unknown as AgentConfig).prompt, 80) }}</div>
    </template>
    <template v-else-if="node.kind === 'trigger'">
      <div class="ns-chips">
        <span v-for="source in ((cfg as unknown as TriggerConfig).sources ?? [])" :key="source" class="ns-chip">{{ triggerSourceLabel(source) }}</span>
      </div>
    </template>
  </div>
</template>

<style scoped>
.ns { display: grid; gap: var(--spacing-1); padding: var(--spacing-1-5) var(--spacing-2-5); min-width: 0; font-size: var(--font-size-xs); color: var(--color-text-secondary); }
.ns:empty { display: none; }
.ns-agent { display: flex; align-items: center; gap: var(--spacing-1-5); min-width: 0; }
.ns-strong { color: var(--color-text); font-size: var(--font-size-sm); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ns-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ns-code { font-family: var(--font-family-mono); font-size: var(--font-size-xs); color: var(--color-text); background: var(--color-bg-tertiary); border-radius: var(--radius-xs); padding: 1px var(--spacing-1-5); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: block; }
.ns-chips { display: flex; flex-wrap: wrap; gap: var(--spacing-1); }
.ns-chip { padding: 0 var(--spacing-1-5); border-radius: var(--radius-full); background: var(--color-bg-tertiary); border: 1px solid var(--color-border); }
.ns-chip.primary { border-color: var(--color-primary-border); color: var(--color-text); }
.ns-case { display: flex; gap: var(--spacing-1-5); align-items: baseline; min-width: 0; }
.ns-case b { color: var(--color-text); font-weight: var(--font-weight-medium); flex: none; }
.ns-case code { font-family: var(--font-family-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ns-note { white-space: pre-wrap; color: var(--color-text); max-height: 120px; overflow: hidden; }
</style>
