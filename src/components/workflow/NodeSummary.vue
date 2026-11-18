<script setup lang="ts">
import { computed } from "vue";
import AgentAvatar from "../AgentAvatar.vue";
import { useAppStore } from "../../stores/app";
import { useWorkflowsStore } from "../../stores/workflows";
import { t } from "../../i18n";
import {
  type AgentConfig,
  type GateConfig,
  type GitConfig,
  type HttpConfig,
  type MapConfig,
  type NodeDef,
  type NotifyConfig,
  type ScriptConfig,
  type SubflowConfig,
  type SwitchConfig,
  type TransformConfig,
  type TriggerConfig,
} from "../../domain/workflow";

const props = defineProps<{ node: NodeDef }>();
const app = useAppStore();
const flows = useWorkflowsStore();

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
const childName = computed(() => {
  const id = (cfg.value as unknown as SubflowConfig | MapConfig).workflowId;
  return flows.list.find((item) => item.id === id)?.name ?? id;
});
const gateDecisions = computed(() => {
  const config = cfg.value as unknown as GateConfig;
  return config.decisions?.length ? config.decisions : [
    { port: "approve", label: t("workflow.canvas.gateApprove"), primary: true },
    { port: "changes", label: t("workflow.canvas.gateChanges") },
    { port: "reject", label: t("workflow.canvas.gateReject") },
  ];
});
const switchCases = computed(() => ((cfg.value as unknown as SwitchConfig).cases ?? []).slice(0, 3));
const switchExtra = computed(() => Math.max(0, ((cfg.value as unknown as SwitchConfig).cases ?? []).length - 3));
</script>

<template>
  <div class="ns">
    <template v-if="node.kind === 'agent' && agent">
      <div class="ns-agent">
        <AgentAvatar v-if="agent.name" :value="agent.avatar" :name="agent.name" :size="18" />
        <span class="ns-strong">{{ agent.name || t("workflow.canvas.noAgent") }}</span>
        <span v-if="agent.mode === 'plan'" class="ds-badge">{{ t("workflow.canvas.planMode") }}</span>
      </div>
      <div v-if="(cfg as unknown as AgentConfig).prompt" class="ns-text">{{ firstLine((cfg as unknown as AgentConfig).prompt, 80) }}</div>
    </template>
    <template v-else-if="node.kind === 'script'">
      <code class="ns-code">{{ firstLine((cfg as unknown as ScriptConfig).command) || t("workflow.canvas.noCommand") }}</code>
    </template>
    <template v-else-if="node.kind === 'gate'">
      <div class="ns-chips">
        <span v-for="decision in gateDecisions" :key="decision.port" class="ns-chip" :class="{ primary: decision.primary }">{{ decision.label }}</span>
      </div>
    </template>
    <template v-else-if="node.kind === 'switch'">
      <div v-for="item in switchCases" :key="item.port" class="ns-case">
        <b>{{ item.port }}</b><code>{{ firstLine(item.when, 34) }}</code>
      </div>
      <div v-if="switchExtra" class="ns-text">+{{ switchExtra }}</div>
    </template>
    <template v-else-if="node.kind === 'transform'">
      <div class="ns-text">{{ t("workflow.canvas.assignments", { count: ((cfg as unknown as TransformConfig).assignments ?? []).length }) }}</div>
    </template>
    <template v-else-if="node.kind === 'http'">
      <code class="ns-code"><b>{{ (cfg as unknown as HttpConfig).method }}</b> {{ firstLine((cfg as unknown as HttpConfig).url, 40) }}</code>
    </template>
    <template v-else-if="node.kind === 'git'">
      <div class="ns-text"><b>{{ (cfg as unknown as GitConfig).op }}</b> · {{ (cfg as unknown as GitConfig).slot || t("workflow.canvas.noSlot") }}</div>
    </template>
    <template v-else-if="node.kind === 'notify'">
      <div class="ns-text"><b>{{ (cfg as unknown as NotifyConfig).channel }}</b> · {{ firstLine((cfg as unknown as NotifyConfig).text, 44) }}</div>
    </template>
    <template v-else-if="node.kind === 'subflow' || node.kind === 'map'">
      <div class="ns-text">{{ childName || t("workflow.canvas.noWorkflow") }}</div>
    </template>
    <template v-else-if="node.kind === 'trigger'">
      <div class="ns-chips">
        <span v-for="source in ((cfg as unknown as TriggerConfig).sources ?? [])" :key="source" class="ns-chip">{{ source }}</span>
      </div>
    </template>
    <template v-else-if="node.kind === 'note'">
      <div class="ns-note">{{ (cfg as { text?: string }).text || node.title }}</div>
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
