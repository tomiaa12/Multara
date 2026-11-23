<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  AlertTriangle, Check, CircleAlert, Copy, Download, FlaskConical, History, LayoutDashboard,
  Play, Redo2, Rocket, ShieldCheck, Trash2, Undo2, Upload, Save, LoaderCircle,
} from "@lucide/vue";
import FlowCanvas from "../../components/workflow/FlowCanvas.vue";
import NodePalette, { type PalettePayload } from "../../components/workflow/NodePalette.vue";
import ProblemsPanel from "../../components/workflow/ProblemsPanel.vue";
import NodeInspector from "../../components/workflow/inspector/NodeInspector.vue";
import { diagnosticField } from "../../components/workflow/inspector/diagnostic-field";
import EdgeInspector from "../../components/workflow/inspector/EdgeInspector.vue";
import WorkflowOutcome from "../../components/workflow/WorkflowOutcome.vue";
import { kindLabelKey } from "../../components/workflow/node-meta";
import WorkflowRunDialog from "./WorkflowRunDialog.vue";
import { confirmDialog } from "../../components/ui/confirm-dialog";
import { showToast } from "../../components/ui/toast";
import { useAppStore } from "../../stores/app";
import { useWorkflowsStore } from "../../stores/workflows";
import * as backend from "../../services/backend";
import { t } from "../../i18n";
import { checkConnection } from "../../domain/workflow-graph";
import {
  defaultConfig,
  type Diagnostic,
  type NodeKind,
  type NodePosition,
  type NodeTemplate,
  type WorkflowRun,
} from "../../domain/workflow";

const route = useRoute();
const router = useRouter();
const app = useAppStore();
const store = useWorkflowsStore();

const workflowId = computed(() => String(route.params.workflowId));
const loading = ref(true);
const loadError = ref("");
const canvas = ref<InstanceType<typeof FlowCanvas> | null>(null);
const selNodes = ref<string[]>([]);
const selEdges = ref<string[]>([]);
const problemsOpen = ref(false);
const focusRequest = ref<{ nodeId: string; field: string; nonce: number } | null>(null);
const nameDraft = ref("");
const actionError = ref("");
const busy = ref("");

// ---------- 加载 ----------
async function load() {
  loading.value = true;
  loadError.value = "";
  try {
    await Promise.all([store.loadWorkflow(workflowId.value), store.loadTemplates(), store.loadList().catch(() => undefined)]);
    nameDraft.value = store.workflow?.name ?? "";
    selNodes.value = [];
    selEdges.value = [];
  } catch (cause) {
    loadError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading.value = false;
  }
}

let remoteDraft: ReturnType<typeof setInterval> | undefined;
function watchRemoteDraft() {
  if (remoteDraft) clearInterval(remoteDraft);
  remoteDraft = setInterval(() => void store.pullExternalDraft(), 2000);
}
function stopRemoteDraft() {
  if (!remoteDraft) return;
  clearInterval(remoteDraft);
  remoteDraft = undefined;
}

onMounted(() => {
  void load();
  watchRemoteDraft();
});
onActivated(() => {
  window.addEventListener("keydown", onKeydown);
  watchRemoteDraft();
  if (store.workflowId !== workflowId.value && !loading.value) void load();
});
onDeactivated(() => {
  window.removeEventListener("keydown", onKeydown);
  stopRemoteDraft();
  void store.saveNow();
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  stopRemoteDraft();
  void store.saveNow();
});
watch(() => store.workflow?.name, (name) => { if (name !== undefined) nameDraft.value = name; });

const draft = computed(() => store.draft);
const diagnostics = computed(() => store.diagnostics);
const errorItems = computed(() => diagnostics.value.filter((item) => item.severity === "error"));
const errorCount = computed(() => errorItems.value.length);
watch(errorCount, (count, previous) => {
  if (count > 0 && !previous) problemsOpen.value = true;
});

// ---------- 顶部状态 ----------
const statusText = computed(() => {
  switch (store.saveState) {
    case "saving": return t("workflow.editor.saving");
    case "dirty": return t("workflow.editor.dirty");
    case "error": return t("workflow.editor.saveFailed", { error: store.saveError });
    case "conflict": return t("workflow.editor.conflict");
    default: break;
  }
  const version = store.workflow?.publishedVersion;
  if (!version) return t("workflow.editor.draftUnpublished");
  return store.unpublished ? t("workflow.editor.draftChanges", { version }) : t("workflow.editor.published", { version });
});

async function commitName() {
  const name = nameDraft.value.trim();
  if (!name || name === store.workflow?.name) {
    nameDraft.value = store.workflow?.name ?? "";
    return;
  }
  try {
    await store.renameWorkflow(name);
  } catch (cause) {
    actionError.value = cause instanceof Error ? cause.message : String(cause);
    nameDraft.value = store.workflow?.name ?? "";
  }
}

// ---------- 画布事件 ----------
function onSelection(selection: { nodes: string[]; edges: string[] }) {
  selNodes.value = selection.nodes;
  selEdges.value = selection.edges;
}

const rightPanel = computed<"node" | "edge" | "multi" | "settings">(() => {
  if (selNodes.value.length === 1 && !selEdges.value.length) return "node";
  if (selEdges.value.length === 1 && !selNodes.value.length) return "edge";
  if (selNodes.value.length + selEdges.value.length > 1) return "multi";
  return "settings";
});
function onMoveNodes(positions: Record<string, NodePosition>) {
  store.moveNodes(positions);
}

const pendingBack = ref<{ source: string; target: string; max: number } | null>(null);

function onConnect(connection: { source: string; target: string }) {
  const def = draft.value;
  if (!def) return;
  const source = def.nodes.find((node) => node.id === connection.source);
  const target = def.nodes.find((node) => node.id === connection.target);
  if (!source || !target) return;
  const check = checkConnection(def, source, target);
  if (check.kind === "invalid") return;
  if (check.kind === "back") {
    pendingBack.value = { ...connection, max: 3 };
    return;
  }
  addConnection(connection, undefined);
}

/** 非 multi 的输入端口只留一条同类连线：新连线替换旧连线 */
function addConnection(
  connection: { source: string; target: string },
  maxIterations: number | undefined,
) {
  store.mutate((def) => {
    if (def.edges.some((edge) => edge.source === connection.source && edge.target === connection.target && (edge.maxIterations != null) === (maxIterations != null))) return;
    let index = def.edges.length + 1;
    const ids = new Set(def.edges.map((edge) => edge.id));
    while (ids.has(`e${index}`)) index += 1;
    def.edges.push({ id: `e${index}`, source: connection.source, target: connection.target, ...(maxIterations != null ? { maxIterations } : {}) });
  });
}

function confirmBack() {
  const pending = pendingBack.value;
  if (!pending) return;
  const max = Math.min(20, Math.max(1, Math.round(Number(pending.max) || 3)));
  addConnection(pending, max);
  pendingBack.value = null;
}

function kindTitle(kind: NodeKind) {
  // 与示例工作流一致：触发器节点标题用「开始」，类型徽标仍显示「触发器」
  if (kind === "trigger") return t("workflow.node.startTitle");
  return t(kindLabelKey(kind));
}

function placeFromPointer(payload: PalettePayload, point: { x: number; y: number }) {
  const position = canvas.value?.clientToFlow(point.x, point.y) ?? { x: 200, y: 200 };
  addPayload(payload, position);
}

function addPayload(payload: PalettePayload, position: NodePosition = canvas.value?.viewportCenter() ?? { x: 200, y: 200 }) {
  let ids: string[] = [];
  if (payload.type === "kind") {
    const kind = payload.kind as NodeKind;
    const id = store.addNode(kind, position, { title: kindTitle(kind) });
    ids = id ? [id] : [];
  } else if (payload.type === "template") {
    const template = store.nodeTemplates.find((item) => item.id === payload.id);
    if (template) ids = store.addTemplate(template, position);
  } else if (payload.type === "agent") {
    const name = payload.agentType === "squad"
      ? app.data?.squads.find((item) => item.id === payload.id)?.name
      : app.data?.agents.find((item) => item.id === payload.id)?.name;
    const id = store.addNode("agent", position, {
      title: name ?? kindTitle("agent"),
      config: { ...defaultConfig("agent"), target: { type: payload.agentType, id: payload.id } },
    });
    ids = id ? [id] : [];
  }
  if (ids.length) window.setTimeout(() => canvas.value?.select(ids), 30);
}

async function deleteTemplate(template: NodeTemplate) {
  if (!(await confirmDialog(t("workflow.palette.deleteTemplate", { name: template.name }), { danger: true }))) return;
  try {
    store.nodeTemplates = await backend.deleteNodeTemplate(template.id);
  } catch (cause) {
    actionError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

// ---------- 删除 / 复制 / 撤销 ----------
function deleteSelection() {
  const draft = store.draft;
  const triedTrigger = Boolean(
    draft && selNodes.value.some((id) => draft.nodes.find((node) => node.id === id)?.kind === "trigger"),
  );
  if (selEdges.value.length) store.removeEdges(selEdges.value);
  if (selNodes.value.length) store.removeNodes(selNodes.value);
  if (triedTrigger) showToast(t("workflow.editor.cannotDeleteTrigger"), "warning");
  selNodes.value = [];
  selEdges.value = [];
  canvas.value?.select([]);
}

function copySelection() {
  store.copyNodes(selNodes.value);
}

async function paste() {
  const ids = await store.pasteFromSystemClipboard();
  if (ids.length) window.setTimeout(() => canvas.value?.select(ids), 30);
}

function onKeydown(event: KeyboardEvent) {
  const target = event.target;
  if (target instanceof HTMLElement && target.closest("input, textarea, select, [contenteditable='true'], .modal-backdrop")) return;
  const mod = event.metaKey || event.ctrlKey;
  const key = event.key.toLowerCase();
  if (mod && key === "z") {
    event.preventDefault();
    if (event.shiftKey) store.redo();
    else store.undo();
  } else if (mod && key === "y") {
    event.preventDefault();
    store.redo();
  } else if (mod && key === "c") {
    if (selNodes.value.length) {
      event.preventDefault();
      copySelection();
    }
  } else if (mod && key === "v") {
    event.preventDefault();
    void paste();
  } else if (mod && key === "a") {
    event.preventDefault();
    canvas.value?.selectAll();
  } else if (mod && key === "s") {
    event.preventDefault();
    void store.saveNow();
  } else if ((event.key === "Delete" || event.key === "Backspace") && (selNodes.value.length || selEdges.value.length)) {
    event.preventDefault();
    deleteSelection();
  }
}

// ---------- 问题面板 ----------
async function validate() {
  actionError.value = "";
  busy.value = "validate";
  try {
    await store.saveNow();
    await store.validate();
    problemsOpen.value = true;
  } catch (cause) {
    actionError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = "";
  }
}

function pickProblem(item: Diagnostic) {
  const node = item.nodeId ? draft.value?.nodes.find((entry) => entry.id === item.nodeId) : undefined;
  const field = diagnosticField(item, node);
  focusRequest.value = item.nodeId && field
    ? { nodeId: item.nodeId, field, nonce: Date.now() }
    : null;
  if (item.nodeId) canvas.value?.select([item.nodeId], [], true);
  else if (item.edgeId) canvas.value?.select([], [item.edgeId]);
}

function focusDiagnostic(item: Diagnostic) {
  publishOpen.value = false;
  problemsOpen.value = true;
  pickProblem(item);
}

// ---------- 发布 / 运行 ----------
const publishOpen = ref(false);
const publishNote = ref("");
const publishError = ref("");

function openPublish() {
  publishNote.value = "";
  publishError.value = "";
  publishOpen.value = true;
}

async function doPublish() {
  busy.value = "publish";
  publishError.value = "";
  try {
    await store.publish(publishNote.value.trim());
    publishOpen.value = false;
  } catch (cause) {
    publishError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = "";
  }
}

const runDialog = ref<null | { draft: boolean }>(null);

async function openRun(draftRun: boolean) {
  actionError.value = "";
  await store.saveNow();
  runDialog.value = { draft: draftRun };
}

function onStarted(run: WorkflowRun) {
  runDialog.value = null;
  void router.push(`/workflows/${run.workflowId}/runs/${run.id}`);
}

// ---------- 导入 / 导出 / 模板 ----------
async function exportFlow() {
  actionError.value = "";
  try {
    await store.saveNow();
    if (!backend.hasBackend()) {
      backend.downloadWorkflowExport(workflowId.value);
      return;
    }
    const path = await backend.pickWorkflowExportPath(store.workflow?.slug || "workflow");
    if (!path) return;
    await backend.exportWorkflow(workflowId.value, path);
  } catch (cause) {
    actionError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function importFlow() {
  actionError.value = "";
  try {
    const created = backend.hasBackend()
      ? await (async () => {
          const path = await backend.pickWorkflowImportFile();
          if (!path) return null;
          return backend.importWorkflow(path);
        })()
      : await (async () => {
          const text = await backend.pickWorkflowJsonText();
          if (!text) return null;
          return backend.importWorkflowText(text);
        })();
    if (!created) return;
    await router.push(`/workflows/${created.id}`);
  } catch (cause) {
    actionError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

const templateOpen = ref(false);
const templateName = ref("");
const templateDesc = ref("");
const templateError = ref("");

function openSaveTemplate() {
  if (!selNodes.value.length) return;
  const first = draft.value?.nodes.find((node) => node.id === selNodes.value[0]);
  templateName.value = selNodes.value.length === 1 ? first?.title ?? "" : "";
  templateDesc.value = "";
  templateError.value = "";
  templateOpen.value = true;
}

async function saveTemplate() {
  const payload = store.copyNodes(selNodes.value);
  if (!payload || !templateName.value.trim()) return;
  templateError.value = "";
  try {
    store.nodeTemplates = await backend.saveNodeTemplate(templateName.value.trim(), templateDesc.value.trim(), {
      nodes: payload.nodes.map((node) => ({ ...node, templateId: undefined })),
      edges: payload.edges,
    });
    templateOpen.value = false;
  } catch (cause) {
    templateError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

const canRun = computed(() => Boolean(store.workflow?.publishedVersion));
</script>

<template>
  <div class="wf-editor">
    <div v-if="loading" class="empty">{{ t("app.loading") }}</div>
    <div v-else-if="loadError || !draft" class="error-card">{{ loadError || t("workflow.editor.notFound") }}</div>
    <div v-else class="wf-body">
      <header class="ed-top">
        <input v-model="nameDraft" class="ed-name" :aria-label="t('workflow.editor.name')" @blur="commitName" @keydown.enter.prevent="($event.target as HTMLInputElement).blur()" />
        <span class="ed-state" :class="store.saveState">
          <LoaderCircle v-if="store.saveState === 'saving'" :size="12" class="spin" />
          <Check v-else-if="store.saveState === 'saved'" :size="12" />
          {{ statusText }}
        </span>
        <UiButton v-if="store.saveState === 'conflict'" size="small" @click="store.reloadAfterConflict()">{{ t("workflow.editor.reload") }}</UiButton>
        <span class="grow" />
        <span v-if="actionError" class="ed-error" :title="actionError">{{ actionError }}</span>
        <UiButton variant="ghost" size="icon" :tip="t('workflow.editor.undo')" :disabled="!store.canUndo" @click="store.undo()"><Undo2 :size="15" /></UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('workflow.editor.redo')" :disabled="!store.canRedo" @click="store.redo()"><Redo2 :size="15" /></UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('workflow.editor.autoLayout')" @click="canvas?.autoLayout()"><LayoutDashboard :size="15" /></UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('workflow.editor.saveTemplate')" :disabled="!selNodes.length" @click="openSaveTemplate"><Save :size="15" /></UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('workflow.editor.export')" @click="exportFlow"><Download :size="15" /></UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('workflow.editor.import')" @click="importFlow"><Upload :size="15" /></UiButton>
        <UiButton variant="ghost" size="small" @click="router.push(`/workflows/${workflowId}/runs`)"><History :size="14" /> {{ t("workflow.editor.runs") }}</UiButton>
        <UiButton size="small" :disabled="busy === 'validate'" @click="validate"><ShieldCheck :size="14" /> {{ t("workflow.editor.validate") }}</UiButton>
        <UiButton size="small" :tip="t('workflow.editor.testRunHint')" @click="openRun(true)"><FlaskConical :size="14" /> {{ t("workflow.editor.testRun") }}</UiButton>
        <UiButton size="small" :disabled="!canRun" :tip="canRun ? '' : t('workflow.editor.runNeedsPublish')" @click="openRun(false)"><Play :size="14" /> {{ t("workflow.editor.run") }}</UiButton>
        <UiButton variant="primary" size="small" :disabled="busy === 'publish'" @click="openPublish"><Rocket :size="14" /> {{ t("workflow.editor.publish") }}</UiButton>
      </header>

      <div class="ed-main">
        <NodePalette @add="addPayload($event)" @place="placeFromPointer" @delete-template="deleteTemplate" />
        <div class="ed-canvas">
          <FlowCanvas
            ref="canvas"
            :definition="draft"
            :diagnostics="diagnostics"
            @selection-change="onSelection"
            @pane-click="onSelection({ nodes: [], edges: [] })"
            @connect="onConnect"
            @move-nodes="onMoveNodes"
          />
          <button
            v-if="errorCount"
            type="button"
            class="ed-badge"
            :aria-label="t('workflow.editor.viewProblems', { count: errorCount })"
            :title="errorItems.map((item) => item.message).join('\n')"
            @click="problemsOpen = true"
          ><CircleAlert :size="13" /> {{ t("workflow.editor.errorsBadge", { count: errorCount }) }}</button>
        </div>
        <aside class="ed-inspector">
          <WorkflowOutcome v-if="rightPanel === 'settings'" tasks-only :workflow-id="workflowId" />
          <NodeInspector
            v-else-if="rightPanel === 'node'"
            :key="selNodes[0]"
            :node-id="selNodes[0]"
            :focus-field="focusRequest?.nodeId === selNodes[0] ? focusRequest.field : ''"
            :focus-nonce="focusRequest?.nodeId === selNodes[0] ? focusRequest.nonce : 0"
          />
          <EdgeInspector v-else-if="rightPanel === 'edge'" :key="selEdges[0]" :edge-id="selEdges[0]" />
          <div v-else-if="rightPanel === 'multi'" class="ed-multi">
            <h3>{{ t("workflow.editor.multiTitle", { count: selNodes.length + selEdges.length }) }}</h3>
            <div class="ed-multi-actions">
              <UiButton size="small" :disabled="!selNodes.length" @click="copySelection"><Copy :size="13" /> {{ t("workflow.editor.copy") }}</UiButton>
              <UiButton size="small" :disabled="!selNodes.length" @click="openSaveTemplate"><Save :size="13" /> {{ t("workflow.editor.saveTemplate") }}</UiButton>
              <UiButton size="small" variant="danger" @click="deleteSelection"><Trash2 :size="13" /> {{ t("common.delete") }}</UiButton>
            </div>
            <p class="ed-hint">{{ t("workflow.editor.shortcuts") }}</p>
          </div>
        </aside>
      </div>
      <ProblemsPanel :diagnostics="diagnostics" :open="problemsOpen" @toggle="problemsOpen = !problemsOpen" @pick="pickProblem" />
    </div>

    <UiModal v-if="pendingBack" @close="pendingBack = null">
      <form class="modal ed-dialog" @submit.prevent="confirmBack">
        <h2>{{ t("workflow.editor.backEdgeTitle") }}</h2>
        <p class="ed-hint">{{ t("workflow.editor.backEdgeHint") }}</p>
        <label>
          {{ t("workflow.editor.maxIterations") }}
          <UiInput v-model.number="pendingBack.max" type="number" min="1" max="20" autofocus />
        </label>
        <div class="ed-dialog-actions">
          <UiButton @click="pendingBack = null">{{ t("common.cancel") }}</UiButton>
          <UiButton type="submit" variant="primary">{{ t("common.confirm") }}</UiButton>
        </div>
      </form>
    </UiModal>

    <UiModal v-if="publishOpen" @close="publishOpen = false">
      <form class="modal ed-dialog" @submit.prevent="doPublish">
        <h2>{{ t("workflow.editor.publishTitle") }}</h2>
        <p v-if="errorCount" class="ed-warn"><AlertTriangle :size="13" /> {{ t("workflow.editor.publishBlocked", { count: errorCount }) }}</p>
        <ul v-if="errorItems.length" class="ed-error-list">
          <li v-for="(item, index) in errorItems" :key="`${item.code}-${item.nodeId ?? ''}-${item.edgeId ?? ''}-${index}`">
            <button type="button" @click="focusDiagnostic(item)">{{ item.message }}</button>
          </li>
        </ul>
        <label>
          {{ t("workflow.editor.publishNote") }}
          <UiTextarea v-model="publishNote" rows="3" :placeholder="t('workflow.editor.publishNotePlaceholder')" />
        </label>
        <p v-if="publishError" class="ed-error-line">{{ publishError }}</p>
        <div class="ed-dialog-actions">
          <UiButton @click="publishOpen = false">{{ t("common.cancel") }}</UiButton>
          <UiButton type="submit" variant="primary" :disabled="busy === 'publish' || errorCount > 0">{{ t("workflow.editor.publish") }}</UiButton>
        </div>
      </form>
    </UiModal>

    <UiModal v-if="templateOpen" @close="templateOpen = false">
      <form class="modal ed-dialog" @submit.prevent="saveTemplate">
        <h2>{{ t("workflow.editor.saveTemplate") }}</h2>
        <label>
          {{ t("workflow.editor.templateName") }}
          <UiInput v-model="templateName" required autofocus />
        </label>
        <label>
          {{ t("workflow.editor.templateDesc") }}
          <UiTextarea v-model="templateDesc" rows="2" />
        </label>
        <p v-if="templateError" class="ed-error-line">{{ templateError }}</p>
        <div class="ed-dialog-actions">
          <UiButton @click="templateOpen = false">{{ t("common.cancel") }}</UiButton>
          <UiButton type="submit" variant="primary">{{ t("common.save") }}</UiButton>
        </div>
      </form>
    </UiModal>

    <WorkflowRunDialog
      v-if="runDialog && draft"
      :workflow-id="workflowId"
      :definition="draft"
      :draft="runDialog.draft"
      @close="runDialog = null"
      @started="onStarted"
    />
  </div>
</template>

<style scoped>
.wf-editor, .wf-body { display: flex; flex-direction: column; height: 100%; min-height: 0; min-width: 0; background: var(--color-bg); }
.ed-top { display: flex; align-items: center; gap: var(--spacing-1-5); padding: 0 var(--spacing-3); height: 44px; flex: none; border-bottom: 1px solid var(--color-border); background: var(--color-surface); }
.ed-name { width: 220px; min-width: 100px; height: var(--control-height-sm); padding: 0 var(--spacing-2); border: 1px solid transparent; border-radius: var(--radius-md); background: transparent; color: var(--color-text-strong); font-size: var(--font-size-lg); font-weight: var(--font-weight-medium); }
.ed-name:hover { border-color: var(--color-border); }
.ed-name:focus { outline: 0; border-color: var(--color-focus); background: var(--color-field); }
.ed-state { display: inline-flex; align-items: center; gap: var(--spacing-1); font-size: var(--font-size-xs); color: var(--color-text-tertiary); white-space: nowrap; }
.ed-state.error, .ed-state.conflict { color: var(--color-error); }
.ed-state.dirty { color: var(--color-warning); }
.grow { flex: 1; }
.ed-error { max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--color-error); font-size: var(--font-size-xs); }
.ed-main { display: flex; flex: 1; min-height: 0; }
.ed-canvas { position: relative; flex: 1; min-width: 0; }
.ed-inspector { display: flex; flex-direction: column; width: 340px; flex: none; min-height: 0; overflow: hidden; border-left: 1px solid var(--color-border); background: var(--color-surface); padding: var(--spacing-3); }
.ed-inspector > :deep(*) { flex: 1; min-height: 0; overflow: auto; }
.ed-badge { position: absolute; top: var(--spacing-3); left: 50%; transform: translateX(-50%); display: inline-flex; gap: var(--spacing-1); align-items: center; padding: 2px var(--spacing-2-5); border-radius: var(--radius-full); background: var(--color-error-subtle); border: 1px solid var(--color-error-border); color: var(--color-error); font-size: var(--font-size-xs); white-space: nowrap; cursor: pointer; z-index: 5; }
.ed-badge:hover { background: var(--color-surface); }
.ed-error-list { display: grid; gap: var(--spacing-1); max-height: 220px; margin: 0; padding: 0; overflow-y: auto; list-style: none; }
.ed-error-list button { width: 100%; padding: var(--spacing-1-5) var(--spacing-2); border: 0; border-radius: var(--radius-md); background: var(--color-error-subtle); color: var(--color-error); font-size: var(--font-size-sm); text-align: left; cursor: pointer; }
.ed-error-list button:hover { background: var(--color-surface-hover); }
.ed-multi h3 { margin: 0 0 var(--spacing-3); font-size: var(--font-size-lg); }
.ed-multi-actions { display: flex; flex-wrap: wrap; gap: var(--spacing-2); }
.ed-hint { color: var(--color-text-tertiary); font-size: var(--font-size-xs); line-height: var(--line-height-normal); }
.ed-dialog { display: grid; gap: var(--spacing-3); width: min(420px, 90vw); }
.ed-dialog h2 { margin: 0; font-size: var(--font-size-xl); }
.ed-dialog label { display: grid; gap: var(--spacing-1); font-size: var(--font-size-sm); color: var(--color-text-secondary); }
.ed-dialog-actions { display: flex; justify-content: flex-end; gap: var(--spacing-2); }
.ed-warn { display: flex; gap: var(--spacing-1-5); align-items: center; color: var(--color-warning); font-size: var(--font-size-sm); margin: 0; }
.ed-error-line { color: var(--color-error); font-size: var(--font-size-sm); margin: 0; }
.spin { animation: ed-spin 1s linear infinite; }
@keyframes ed-spin { to { transform: rotate(360deg); } }
</style>
