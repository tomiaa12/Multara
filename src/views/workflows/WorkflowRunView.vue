<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { t } from "../../i18n";
import { useWorkflowsStore } from "../../stores/workflows";
import SegmentedTabs from "../../components/ui/SegmentedTabs.vue";
import FlowCanvas from "../../components/workflow/FlowCanvas.vue";
import AgentSessionPane from "../../components/workflow/run/AgentSessionPane.vue";
import NodeRunDrawer from "../../components/workflow/run/NodeRunDrawer.vue";
import RunHeader from "../../components/workflow/run/RunHeader.vue";
import RunTimeline from "../../components/workflow/run/RunTimeline.vue";
import { buildRunOverlay } from "../../components/workflow/run/run-overlay";

const route = useRoute();
const store = useWorkflowsStore();

const runId = computed(() => String(route.params.runId ?? ""));
const detail = computed(() => store.runs[runId.value]);
const error = ref("");
const view = ref("canvas");
const selectedNodeId = ref<string | null>(null);
const selectedEdgeId = ref<string | null>(null);
const overviewOpen = ref(false);

const overlay = computed(() => (detail.value ? buildRunOverlay(detail.value) : undefined));
const selectedEdge = computed(() => {
  const id = selectedEdgeId.value;
  if (!id || !detail.value) return null;
  const edge = detail.value.definition.edges.find((item) => item.id === id);
  return edge ? { edge, info: overlay.value?.edges[id] } : null;
});

async function load() {
  if (!runId.value) return;
  error.value = "";
  try {
    await store.loadRun(runId.value);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function selectNode(id: string | null) {
  selectedNodeId.value = id;
  if (id) selectedEdgeId.value = null;
}

function selectEdge(id: string | null) {
  selectedEdgeId.value = id;
  if (id) selectedNodeId.value = null;
}

onMounted(() => {
  void store.subscribe();
  void load();
});
onBeforeUnmount(() => store.unsubscribe());
watch(runId, () => {
  selectedNodeId.value = null;
  selectedEdgeId.value = null;
  void load();
});
</script>

<template>
  <div class="run-page">
    <div v-if="error" class="error-card run-error">{{ error }}</div>
    <div v-else-if="!detail" class="empty">{{ t("app.loading") }}</div>
    <template v-else>
      <RunHeader :detail="detail" :overview-open="overviewOpen" @toggle-overview="overviewOpen = !overviewOpen" />
      <div class="run-body">
        <section class="run-main">
          <div class="run-toolbar">
            <SegmentedTabs
              v-model="view"
              :label="t('workflow.run.view')"
              :items="[{ id: 'canvas', label: t('workflow.run.viewCanvas') }, { id: 'timeline', label: t('workflow.run.viewTimeline') }]"
            />
            <span v-if="selectedEdge" class="edge-info">
              <code>{{ selectedEdge.edge.source }}.{{ selectedEdge.edge.sourcePort }} → {{ selectedEdge.edge.target }}.{{ selectedEdge.edge.targetPort }}</code>
              <span class="ds-badge" :data-tone="selectedEdge.info?.state === 'live' ? 'success' : undefined">{{ t(`workflow.run.edge.${selectedEdge.info?.state ?? 'pending'}`) }}</span>
              <span v-if="selectedEdge.info?.tooltip" class="edge-value">{{ selectedEdge.info.tooltip }}</span>
            </span>
          </div>
          <div class="run-stage">
            <FlowCanvas
              v-show="view === 'canvas'"
              :definition="detail.definition"
              readonly
              :overlay="overlay"
              :selected-node-id="selectedNodeId"
              :selected-edge-id="selectedEdgeId"
              @select-node="selectNode"
              @select-edge="selectEdge"
            />
            <RunTimeline v-if="view === 'timeline'" :detail="detail" :selected-node-id="selectedNodeId" @select-node="selectNode" />
          </div>
        </section>
        <NodeRunDrawer v-if="selectedNodeId" class="run-drawer" :detail="detail" :node-id="selectedNodeId" @close="selectNode(null)" />
        <aside v-if="overviewOpen" class="run-overview">
          <AgentSessionPane :session-id="detail.run.overviewSessionId" />
        </aside>
      </div>
    </template>
  </div>
</template>

<style scoped>
.run-page {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.run-error {
  margin: var(--spacing-6);
}
.run-body {
  display: flex;
  flex: 1;
  min-height: 0;
}
.run-main {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}
.run-toolbar {
  display: flex;
  align-items: center;
  gap: var(--spacing-3);
  padding: var(--spacing-2) var(--spacing-4);
  border-bottom: 1px solid var(--color-border-light);
}
.edge-info {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  min-width: 0;
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}
.edge-value {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-family-mono);
}
.run-stage {
  position: relative;
  flex: 1;
  min-height: 0;
}
.run-stage > :deep(*) {
  height: 100%;
}
.run-drawer {
  flex: 0 0 420px;
  width: 420px;
}
.run-overview {
  flex: 0 0 380px;
  width: 380px;
  min-height: 0;
  border-left: 1px solid var(--color-border);
}
</style>
