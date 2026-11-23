import { defineStore } from "pinia";
import { computed, ref, shallowRef } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import * as backend from "../services/backend";
import { mergeById } from "./merge-by-id";
import { remapClipboard } from "../domain/workflow-graph";
import {
  createNode,
  isRunTerminal,
  uniqueEdgeId,
  uniqueNodeId,
  type Diagnostic,
  type EdgeDef,
  type NodeDef,
  type NodeKind,
  type NodePosition,
  type NodeTemplate,
  type RunDetail,
  type SecretName,
  type VersionMeta,
  type Workflow,
  type WorkflowDef,
  type WorkflowSummary,
  type WorkflowTemplateMeta,
} from "../domain/workflow";

export const AUTOSAVE_DELAY_MS = 800;
const UNDO_LIMIT = 100;
const COALESCE_MS = 700;
const CLIP_PREFIX = "multara-flow-clip:";

export interface ClipboardPayload { nodes: NodeDef[]; edges: EdgeDef[] }

export type SaveState = "idle" | "dirty" | "saving" | "saved" | "error" | "conflict";

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

let memoryClipboard: ClipboardPayload | null = null;

export const useWorkflowsStore = defineStore("workflows", () => {
  // ---------- 列表与模板 ----------
  const list = ref<WorkflowSummary[]>([]);
  const listLoaded = ref(false);
  const nodeTemplates = ref<NodeTemplate[]>([]);
  const flowTemplates = ref<WorkflowTemplateMeta[]>([]);
  const secrets = ref<SecretName[]>([]);

  async function loadList() {
    const next = await backend.listWorkflows();
    list.value = mergeById(list.value, next);
    listLoaded.value = true;
  }

  async function loadTemplates() {
    const [nodes, flows] = await Promise.all([backend.listNodeTemplates(), backend.listWorkflowTemplates()]);
    nodeTemplates.value = nodes;
    flowTemplates.value = flows;
  }

  async function loadSecrets() {
    secrets.value = await backend.listWorkflowSecrets();
  }

  // ---------- 当前草稿 ----------
  const workflowId = ref<string | null>(null);
  const workflow = ref<Workflow | null>(null);
  const draft = ref<WorkflowDef | null>(null);
  const versions = ref<VersionMeta[]>([]);
  const diagnostics = ref<Diagnostic[]>([]);
  const updatedAt = ref<string | null>(null);
  const saveState = ref<SaveState>("idle");
  const saveError = ref("");
  /** 与已发布版本相比是否有改动（保存后由后端 `hasUnpublishedChanges` 刷新；本地编辑立即置 true） */
  const unpublished = ref(false);

  const undoStack = shallowRef<string[]>([]);
  const redoStack = shallowRef<string[]>([]);
  const canUndo = computed(() => undoStack.value.length > 0);
  const canRedo = computed(() => redoStack.value.length > 0);

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let saving: Promise<void> | null = null;
  let lastCoalesceKey = "";
  let lastCoalesceAt = 0;

  async function loadWorkflow(id: string) {
    flushTimerOnly();
    const detail = await backend.getWorkflow(id);
    workflowId.value = id;
    workflow.value = detail.workflow;
    draft.value = detail.draft;
    versions.value = detail.versions;
    diagnostics.value = detail.diagnostics;
    updatedAt.value = detail.workflow.draftUpdatedAt;
    unpublished.value = detail.workflow.hasUnpublishedChanges;
    undoStack.value = [];
    redoStack.value = [];
    saveState.value = "idle";
    saveError.value = "";
  }

  function unloadWorkflow() {
    flushTimerOnly();
    workflowId.value = null;
    workflow.value = null;
    draft.value = null;
    undoStack.value = [];
    redoStack.value = [];
  }

  function flushTimerOnly() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = null;
  }

  function scheduleSave() {
    saveState.value = "dirty";
    flushTimerOnly();
    saveTimer = setTimeout(() => void saveNow(), AUTOSAVE_DELAY_MS);
  }

  /** 立即保存（发布 / 试运行 / 离开页面前调用）；保存中再次编辑会在本次结束后补存 */
  async function saveNow(): Promise<void> {
    flushTimerOnly();
    if (saving) {
      await saving;
      if (saveState.value === "dirty") return saveNow();
      return;
    }
    if (!draft.value || !workflowId.value || saveState.value === "conflict") return;
    const id = workflowId.value;
    const snapshot = clone(draft.value);
    saveState.value = "saving";
    saving = (async () => {
      try {
        const result = await backend.saveWorkflowDraft(id, snapshot, updatedAt.value);
        if (workflowId.value !== id) return;
        updatedAt.value = result.updatedAt;
        diagnostics.value = result.diagnostics;
        saveError.value = "";
        // 保存期间又有编辑：保持 dirty，由下面补存
        if (saveState.value === "saving") saveState.value = "saved";
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        saveError.value = message;
        saveState.value = /\[conflict\]|conflict/i.test(message) ? "conflict" : "error";
      } finally {
        saving = null;
      }
    })();
    await saving;
    // 保存期间又有编辑（mutate 会把状态置回 dirty）：补存一次
    if ((saveState.value as SaveState) === "dirty") await saveNow();
  }

  /** 冲突后以服务器版本为准重新加载 */
  async function reloadAfterConflict() {
    if (!workflowId.value) return;
    await loadWorkflow(workflowId.value);
  }

  /** 智能体用 CLI 改了草稿时，编辑器空闲就换成库里的版本。本地还没存完时不动。 */
  async function pullExternalDraft() {
    const id = workflowId.value;
    const stamp = updatedAt.value;
    if (!id || !stamp) return;
    if (saveState.value !== "idle" && saveState.value !== "saved") return;
    try {
      const detail = await backend.getWorkflow(id);
      if (workflowId.value !== id || updatedAt.value !== stamp) return;
      if (saveState.value !== "idle" && saveState.value !== "saved") return;
      if (detail.workflow.draftUpdatedAt === stamp) return;
      draft.value = detail.draft;
      workflow.value = detail.workflow;
      versions.value = detail.versions;
      diagnostics.value = detail.diagnostics;
      updatedAt.value = detail.workflow.draftUpdatedAt;
      unpublished.value = detail.workflow.hasUnpublishedChanges;
      undoStack.value = [];
      redoStack.value = [];
      saveState.value = "idle";
    } catch {
      // 下一轮再试
    }
  }

  /**
   * 修改草稿。`coalesceKey` 相同且间隔很短的连续修改合并成一步撤销（输入框打字、拖动节点）。
   */
  function mutate(change: (def: WorkflowDef) => void, coalesceKey = "") {
    if (!draft.value) return;
    const before = JSON.stringify(draft.value);
    const next = clone(draft.value);
    change(next);
    const after = JSON.stringify(next);
    if (before === after) return;
    const now = Date.now();
    const coalesce = coalesceKey !== "" && coalesceKey === lastCoalesceKey && now - lastCoalesceAt < COALESCE_MS;
    lastCoalesceKey = coalesceKey;
    lastCoalesceAt = now;
    if (!coalesce) undoStack.value = [...undoStack.value, before].slice(-UNDO_LIMIT);
    redoStack.value = [];
    draft.value = next;
    unpublished.value = true;
    scheduleSave();
  }

  function restore(snapshot: string) {
    draft.value = JSON.parse(snapshot) as WorkflowDef;
    unpublished.value = true;
    lastCoalesceKey = "";
    scheduleSave();
  }

  function undo() {
    if (!draft.value || !undoStack.value.length) return;
    const stack = undoStack.value;
    const previous = stack[stack.length - 1];
    redoStack.value = [...redoStack.value, JSON.stringify(draft.value)];
    undoStack.value = stack.slice(0, -1);
    restore(previous);
  }

  function redo() {
    if (!draft.value || !redoStack.value.length) return;
    const stack = redoStack.value;
    const next = stack[stack.length - 1];
    undoStack.value = [...undoStack.value, JSON.stringify(draft.value)];
    redoStack.value = stack.slice(0, -1);
    restore(next);
  }

  // ---------- 编辑操作 ----------
  function addNode(kind: NodeKind, position: NodePosition, patch?: Partial<NodeDef>): string | null {
    let created: string | null = null;
    mutate((def) => {
      const node = createNode(kind, def, position, patch?.title ?? "");
      Object.assign(node, patch ?? {}, { id: node.id });
      def.nodes.push(node);
      created = node.id;
    });
    return created;
  }

  /** 把节点模板（可含多个节点 + 内部连线）放进画布，返回新节点 id */
  function addTemplate(template: NodeTemplate, position: NodePosition): string[] {
    let ids: string[] = [];
    mutate((def) => {
      const hasTrigger = def.nodes.some((node) => node.kind === "trigger");
      const nodes = template.nodes.filter((node) => !(node.kind === "trigger" && hasTrigger));
      const allowed = new Set(nodes.map((node) => node.id));
      const edges = template.edges.filter((edge) => allowed.has(edge.source) && allowed.has(edge.target));
      const minX = Math.min(...nodes.map((node) => node.position?.x ?? 0), 0);
      const minY = Math.min(...nodes.map((node) => node.position?.y ?? 0), 0);
      const placed = remapClipboard(
        nodes.map((node) => ({ ...clone(node), templateId: template.builtin || template.id ? template.id : node.templateId })),
        edges,
        def.nodes.map((node) => node.id),
        def.edges.map((edge) => edge.id),
        { x: position.x - minX, y: position.y - minY },
      );
      def.nodes.push(...placed.nodes);
      def.edges.push(...placed.edges);
      ids = placed.nodes.map((node) => node.id);
    });
    return ids;
  }

  function updateNode(nodeId: string, change: (node: NodeDef, def: WorkflowDef) => void, coalesceKey?: string) {
    mutate((def) => {
      const node = def.nodes.find((item) => item.id === nodeId);
      if (node) change(node, def);
    }, coalesceKey ?? `node:${nodeId}`);
  }

  function moveNodes(positions: Record<string, NodePosition>) {
    mutate((def) => {
      for (const node of def.nodes) {
        const next = positions[node.id];
        if (next) node.position = { x: Math.round(next.x), y: Math.round(next.y) };
      }
    }, `move:${Object.keys(positions).sort().join(",")}`);
  }

  /** 改节点 id，同步修正连线；返回是否成功 */
  function renameNode(oldId: string, newId: string): boolean {
    if (!draft.value || oldId === newId) return false;
    if (draft.value.nodes.some((node) => node.id === newId)) return false;
    mutate((def) => {
      const node = def.nodes.find((item) => item.id === oldId);
      if (!node) return;
      node.id = newId;
      for (const edge of def.edges) {
        if (edge.source === oldId) edge.source = newId;
        if (edge.target === oldId) edge.target = newId;
      }
    });
    return true;
  }

  /** 删除节点及其连线；触发器不可删除 */
  function removeNodes(ids: string[]) {
    mutate((def) => {
      const drop = new Set(ids.filter((id) => def.nodes.find((node) => node.id === id)?.kind !== "trigger"));
      def.nodes = def.nodes.filter((node) => !drop.has(node.id));
      def.edges = def.edges.filter((edge) => !drop.has(edge.source) && !drop.has(edge.target));
    });
  }

  function addEdge(edge: Omit<EdgeDef, "id">): string | null {
    let id: string | null = null;
    mutate((def) => {
      if (def.edges.some((item) =>
        item.source === edge.source && item.target === edge.target && item.when === edge.when)) return;
      id = uniqueEdgeId(def.edges.map((item) => item.id));
      def.edges.push({ ...edge, id });
    });
    return id;
  }

  function updateEdge(edgeId: string, patch: Partial<EdgeDef>) {
    mutate((def) => {
      const edge = def.edges.find((item) => item.id === edgeId);
      if (edge) Object.assign(edge, patch);
    }, `edge:${edgeId}`);
  }

  function removeEdges(ids: string[]) {
    const drop = new Set(ids);
    mutate((def) => {
      def.edges = def.edges.filter((edge) => !drop.has(edge.id));
    });
  }

  // ---------- 复制 / 粘贴 ----------
  function copyNodes(ids: string[]): ClipboardPayload | null {
    if (!draft.value || !ids.length) return null;
    const picked = new Set(ids);
    const nodes = draft.value.nodes.filter((node) => picked.has(node.id) && node.kind !== "trigger");
    if (!nodes.length) return null;
    const inside = new Set(nodes.map((node) => node.id));
    const edges = draft.value.edges.filter((edge) => inside.has(edge.source) && inside.has(edge.target));
    const payload: ClipboardPayload = clone({ nodes, edges });
    memoryClipboard = payload;
    try {
      void navigator.clipboard?.writeText(CLIP_PREFIX + JSON.stringify(payload));
    } catch {
      // 剪贴板不可用时只留内存副本
    }
    return payload;
  }

  /** 粘贴：重新生成节点 / 连线 id 并修正内部连线，返回新节点 id */
  function pasteNodes(payload: ClipboardPayload | null = memoryClipboard, offset = { x: 40, y: 40 }): string[] {
    if (!payload || !draft.value) return [];
    let ids: string[] = [];
    mutate((def) => {
      const placed = remapClipboard(
        payload.nodes.filter((node) => node.kind !== "trigger"),
        payload.edges,
        def.nodes.map((node) => node.id),
        def.edges.map((edge) => edge.id),
        offset,
      );
      def.nodes.push(...placed.nodes);
      def.edges.push(...placed.edges);
      ids = placed.nodes.map((node) => node.id);
    });
    return ids;
  }

  async function pasteFromSystemClipboard(offset?: { x: number; y: number }): Promise<string[]> {
    try {
      const text = await navigator.clipboard.readText();
      if (text.startsWith(CLIP_PREFIX)) {
        return pasteNodes(JSON.parse(text.slice(CLIP_PREFIX.length)) as ClipboardPayload, offset);
      }
    } catch {
      // 无权限读剪贴板：退回内存副本
    }
    return pasteNodes(memoryClipboard, offset);
  }

  function hasClipboard(): boolean {
    return memoryClipboard !== null;
  }

  // ---------- 元信息 / 发布 ----------
  async function renameWorkflow(name: string) {
    if (!workflow.value) return;
    workflow.value = await backend.updateWorkflowMeta(workflow.value.id, name, workflow.value.description, workflow.value.icon);
  }

  async function publish(note: string): Promise<number> {
    if (!workflowId.value) throw new Error("no workflow");
    await saveNow();
    const version = await backend.publishWorkflow(workflowId.value, note);
    await refreshMeta();
    return version;
  }

  async function refreshMeta() {
    if (!workflowId.value) return;
    const detail = await backend.getWorkflow(workflowId.value);
    workflow.value = detail.workflow;
    versions.value = detail.versions;
    unpublished.value = detail.workflow.hasUnpublishedChanges;
    updatedAt.value = detail.workflow.draftUpdatedAt;
  }

  async function validate() {
    if (!draft.value) return;
    diagnostics.value = await backend.validateWorkflow(draft.value);
  }

  // ---------- 运行详情缓存与事件 ----------
  const runs = ref<Record<string, RunDetail>>({});
  const runLoading = ref<Record<string, boolean>>({});
  /** 脚本节点实时日志缓冲（nodeRunId -> 文本），ScriptLogPane 读取 */
  const liveLogs = ref<Record<string, string>>({});
  const LIVE_LOG_LIMIT = 512 * 1024;
  const refetchTimers = new Map<string, ReturnType<typeof setTimeout>>();

  function applyRunDetail(detail: RunDetail) {
    const previous = runs.value[detail.run.id];
    runs.value[detail.run.id] = previous
      ? {
          ...detail,
          nodeRuns: mergeById(previous.nodeRuns, detail.nodeRuns),
          values: mergeById(previous.values, detail.values),
          children: mergeById(previous.children, detail.children),
        }
      : detail;
  }

  async function loadRun(runId: string): Promise<RunDetail> {
    runLoading.value[runId] = true;
    try {
      const detail = await backend.getWorkflowRun(runId);
      applyRunDetail(detail);
      return runs.value[runId];
    } finally {
      runLoading.value[runId] = false;
    }
  }

  function scheduleRunRefetch(runId: string) {
    if (!runs.value[runId] || refetchTimers.has(runId)) return;
    refetchTimers.set(runId, setTimeout(() => {
      refetchTimers.delete(runId);
      void loadRun(runId).catch(() => undefined);
    }, 150));
  }

  function onRunChanged(payload: { runId: string; workflowId: string; status: string }) {
    const cached = runs.value[payload.runId];
    if (cached) cached.run = { ...cached.run, status: payload.status };
    scheduleRunRefetch(payload.runId);
    if (listLoaded.value) void loadList().catch(() => undefined);
  }

  function onNodeChanged(payload: { runId: string; nodeRunId: string; nodeId: string; status: string }) {
    const cached = runs.value[payload.runId];
    if (cached) {
      const target = cached.nodeRuns.find((item) => item.id === payload.nodeRunId);
      if (target && target.status !== payload.status) {
        cached.nodeRuns = cached.nodeRuns.map((item) => (item.id === payload.nodeRunId ? { ...item, status: payload.status } : item));
      }
    }
    scheduleRunRefetch(payload.runId);
  }

  function onNodeLog(payload: { nodeRunId: string; stream: string; chunk: string }) {
    const current = liveLogs.value[payload.nodeRunId] ?? "";
    const next = current + payload.chunk;
    liveLogs.value[payload.nodeRunId] = next.length > LIVE_LOG_LIMIT ? next.slice(next.length - LIVE_LOG_LIMIT) : next;
  }

  function clearLiveLog(nodeRunId: string) {
    delete liveLogs.value[nodeRunId];
  }

  let subscribers = 0;
  let unlisten: UnlistenFn[] = [];

  /** 页面挂载时订阅后端事件，卸载时取消；多个页面共用同一组监听 */
  async function subscribe() {
    subscribers += 1;
    if (subscribers > 1) return;
    try {
      const handlers = await Promise.all([
        listen<{ runId: string; workflowId: string; status: string }>("workflow-run-changed", (event) => onRunChanged(event.payload)),
        listen<{ runId: string; nodeRunId: string; nodeId: string; status: string }>("workflow-node-changed", (event) => onNodeChanged(event.payload)),
        listen<{ nodeRunId: string; stream: string; chunk: string }>("workflow-node-log", (event) => onNodeLog(event.payload)),
      ]);
      if (subscribers === 0) handlers.forEach((fn) => fn());
      else unlisten = handlers;
    } catch {
      // 浏览器预览没有事件总线
    }
  }

  function unsubscribe() {
    subscribers = Math.max(0, subscribers - 1);
    if (subscribers > 0) return;
    unlisten.forEach((fn) => fn());
    unlisten = [];
  }

  function runIsLive(runId: string): boolean {
    const run = runs.value[runId]?.run;
    return Boolean(run) && !isRunTerminal(run!.status);
  }

  return {
    list, listLoaded, nodeTemplates, flowTemplates, secrets,
    loadList, loadTemplates, loadSecrets,
    workflowId, workflow, draft, versions, diagnostics, updatedAt, saveState, saveError, unpublished,
    canUndo, canRedo,
    loadWorkflow, unloadWorkflow, saveNow, reloadAfterConflict, pullExternalDraft, mutate, undo, redo,
    addNode, addTemplate, updateNode, moveNodes, renameNode, removeNodes, addEdge, updateEdge, removeEdges,
    copyNodes, pasteNodes, pasteFromSystemClipboard, hasClipboard,
    renameWorkflow, publish, refreshMeta, validate,
    runs, runLoading, liveLogs, loadRun, applyRunDetail, onRunChanged, onNodeChanged, onNodeLog, clearLiveLog,
    subscribe, unsubscribe, runIsLive,
    uniqueNodeId,
  };
});
