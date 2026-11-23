<script setup lang="ts">
import { computed } from "vue";
import { ArrowRight, Trash2 } from "@lucide/vue";
import { useWorkflowsStore } from "../../../stores/workflows";
import { t } from "../../../i18n";
import { nodeDisplayTitle } from "../node-meta";
import "./inspector.css";

const props = defineProps<{ edgeId: string }>();
const store = useWorkflowsStore();
const edge = computed(() => store.draft?.edges.find((item) => item.id === props.edgeId) ?? null);
const issues = computed(() => store.diagnostics.filter((item) => item.edgeId === props.edgeId));
const isBack = computed(() => edge.value?.maxIterations != null);

function title(nodeId: string) {
  const node = store.draft?.nodes.find((item) => item.id === nodeId);
  return nodeDisplayTitle(node, nodeId, t);
}

function setMax(value: unknown) {
  const next = Math.min(20, Math.max(1, Math.round(Number(value) || 1)));
  store.updateEdge(props.edgeId, { maxIterations: next });
}

function remove() {
  store.removeEdges([props.edgeId]);
}
</script>

<template>
  <div v-if="edge" class="wf-form edge-inspector">
    <p v-for="(item, index) in issues" :key="`${item.code}-${index}`" class="wf-error">{{ item.message }}</p>
    <div class="wf-section-title">{{ isBack ? t("workflow.inspector.backEdge") : t("workflow.inspector.edge") }}</div>
    <div class="wf-card">
      <div class="wf-row"><strong>{{ title(edge.source) }}</strong></div>
      <ArrowRight :size="14" />
      <div class="wf-row"><strong>{{ title(edge.target) }}</strong></div>
    </div>
    <template v-if="isBack">
      <label class="wf-field">
        <span class="wf-label">{{ t("workflow.inspector.maxIterations") }}</span>
        <UiInput type="number" min="1" max="20" :model-value="edge.maxIterations" @update:model-value="setMax" />
        <span class="wf-hint">{{ t("workflow.inspector.maxIterationsHint") }}</span>
      </label>
    </template>
    <div><UiButton variant="danger" size="small" @click="remove"><Trash2 :size="14" /> {{ t("workflow.inspector.deleteEdge") }}</UiButton></div>
  </div>
</template>

<style scoped>
.edge-inspector {
  padding: var(--spacing-3);
}
</style>
