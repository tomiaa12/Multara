<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch, type Component } from "vue";
import { AlertTriangle, CircleAlert } from "@lucide/vue";
import { useWorkflowsStore } from "../../../stores/workflows";
import {
  type Diagnostic,
  type NodeDef,
  type NodeKind,
} from "../../../domain/workflow";
import { t } from "../../../i18n";
import AgentConfig from "./config/AgentConfig.vue";
import TriggerConfig from "./config/TriggerConfig.vue";
import "./inspector.css";
import { diagnosticField } from "./diagnostic-field";

const props = defineProps<{ nodeId: string; focusField?: string; focusNonce?: number }>();

const store = useWorkflowsStore();
const node = computed(() => store.draft?.nodes.find((item) => item.id === props.nodeId) ?? null);
const nodeIssues = computed(() => store.diagnostics.filter((item) => item.nodeId === props.nodeId));
const root = ref<HTMLElement | null>(null);

const CONFIGS: Partial<Record<NodeKind, Component>> = {
  trigger: TriggerConfig,
  agent: AgentConfig,
};

function issueField(item: Diagnostic) {
  return diagnosticField(item, node.value);
}

function markInvalid() {
  const host = root.value;
  if (!host) return;
  const fields = new Set(nodeIssues.value.map((item) => issueField(item)).filter((field): field is string => Boolean(field)));
  host.querySelectorAll<HTMLElement>("[data-field]").forEach((el) => {
    el.classList.toggle("wf-invalid", fields.has(el.dataset.field ?? ""));
  });
}

async function reveal(field: string) {
  await nextTick();
  await nextTick();
  markInvalid();
  const el = root.value?.querySelector<HTMLElement>(`[data-field="${CSS.escape(field)}"]`);
  if (!el) return;
  el.scrollIntoView({ block: "center", behavior: "smooth" });
  const focusable = el.querySelector<HTMLElement>("[data-focus]")
    ?? el.querySelector<HTMLElement>("textarea, input:not([type=checkbox]):not([type=hidden]), button");
  focusable?.focus({ preventScroll: true });
}

watch(() => props.focusNonce, (nonce) => {
  if (nonce && props.focusField) void reveal(props.focusField);
});
onMounted(() => {
  if (props.focusNonce && props.focusField) void reveal(props.focusField);
});
watch(nodeIssues, () => { void nextTick(markInvalid); });

function patch(change: (node: NodeDef) => void, key?: string) {
  store.updateNode(props.nodeId, change, key);
}

function setTitle(value: string) {
  patch((item) => { item.title = value; }, `node:${props.nodeId}:title`);
}
</script>

<template>
  <div v-if="node" ref="root" class="node-inspector">
    <ul v-if="nodeIssues.length" class="node-issues" role="status">
      <li v-for="(item, index) in nodeIssues" :key="`${item.code}-${index}`" :class="[item.severity, { jump: Boolean(issueField(item)) }]" @click="issueField(item) && reveal(issueField(item)!)">
        <CircleAlert v-if="item.severity === 'error'" :size="13" class="error" />
        <AlertTriangle v-else :size="13" class="warning" />
        <span>{{ item.message }}</span>
      </li>
    </ul>
    <header class="head">
      <span class="kind-chip">{{ t(`workflow.inspector.kind.${node.kind}`) }}</span>
      <span class="wf-mono head-id">{{ node.id }}</span>
    </header>

    <div class="body">
      <div class="wf-form">
        <label class="wf-field">
          <span class="wf-label">{{ t("workflow.inspector.nodeTitle") }}</span>
          <UiInput :model-value="node.title" @update:model-value="setTitle(String($event))" />
        </label>
      </div>
      <p v-if="node.kind === 'output'" class="wf-hint output-hint">{{ t("workflow.inspector.outputHint") }}</p>
      <component :is="CONFIGS[node.kind]" v-else-if="CONFIGS[node.kind]" :key="node.id" :node="node" />
    </div>
  </div>
</template>

<style scoped>
.node-inspector {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-3);
  min-width: 0;
  padding: var(--spacing-3);
}
.node-issues {
  display: grid;
  gap: var(--spacing-1);
  margin: 0;
  padding: 0;
  list-style: none;
}
.node-issues li {
  display: flex;
  align-items: flex-start;
  gap: var(--spacing-1-5);
  padding: var(--spacing-1-5) var(--spacing-2);
  border-radius: var(--radius-md);
  background: var(--color-error-subtle);
  color: var(--color-text);
  font-size: var(--font-size-sm);
  line-height: var(--line-height-normal);
}
.node-issues li.warning { background: var(--color-warning-subtle); }
.node-issues li.jump { cursor: pointer; }
.node-issues .error { flex: none; color: var(--color-error); margin-top: 2px; }
.node-issues .warning { flex: none; color: var(--color-warning); margin-top: 2px; }
.head {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  min-width: 0;
}
.kind-chip {
  padding: 1px var(--spacing-2);
  border-radius: var(--radius-full);
  background: var(--color-bg-tertiary);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
.head-id {
  color: var(--color-text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
}
.id-readonly {
  display: block;
  min-width: 0;
  padding: var(--spacing-1-5) var(--spacing-2-5);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-bg-tertiary);
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  line-height: var(--line-height-normal);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: all;
}
.body {
  min-width: 0;
}
.implicit-list {
  display: grid;
  gap: var(--spacing-2);
  margin: 0;
  padding: 0;
  list-style: none;
}
.implicit-list li {
  display: grid;
  gap: 2px;
}
.implicit-list code {
  color: var(--color-text);
}
.implicit-list span {
  color: var(--color-text-tertiary);
  font-size: var(--font-size-xs);
  line-height: var(--line-height-normal);
}
</style>
