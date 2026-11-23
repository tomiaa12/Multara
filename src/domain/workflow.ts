/**
 * 工作流定义类型，与 src-tauri/src/workflow/model.rs 一一对应（均为 camelCase）。
 * 权威校验在后端；这里只做编辑器里的即时提示。
 */
import compat from "../../shared/workflow-port-compat.json";
import { t } from "../i18n";

export type PortType =
  | "text" | "markdown" | "number" | "boolean"
  | "json" | "url" | "file" | "files" | "any";

export const PORT_TYPES: PortType[] = [
  "text", "markdown", "number", "boolean", "json", "url", "file", "files", "any",
];

export type JsonSchema = Record<string, unknown>;

export interface PortDef {
  name: string;
  type: PortType;
  label?: string;
  description?: string;
  required?: boolean;
  schema?: JsonSchema;
  multi?: boolean;
  default?: unknown;
  /** 集成消息触发时，消息正文填到这个入参 */
  primary?: boolean;
}

export type NodeKind = "trigger" | "output" | "agent";

export const NODE_KINDS: NodeKind[] = ["trigger", "output", "agent"];

/** 节点库里展示的种类。开始每个工作流只有一个，创建时自带。 */
export const PALETTE_KINDS: NodeKind[] = ["agent", "output"];

export interface WorkspaceUse { slot: string; access: "read" | "write"; fork?: boolean }

export interface RetryPolicy { maxAttempts: number; backoffSeconds: number; on: string[] }

export interface NodePosition { x: number; y: number }

export type OnError = "fail_run" | "continue";
export type JoinMode = "all" | "any";

export interface NodeDef<C = Record<string, unknown>> {
  id: string;
  kind: NodeKind;
  title: string;
  description?: string;
  config: C;
  workspaces?: WorkspaceUse[];
  timeoutMinutes?: number;
  position: NodePosition;
}

export interface EdgeDef {
  id: string;
  source: string;
  target: string;
  /** 人工审批的决定：approve / changes / reject */
  when?: string;
  /** 有值即为回边，只能指回上游，1~20 */
  maxIterations?: number;
}

export type Isolation = "worktree_new" | "worktree_existing" | "direct";

export interface WorkspaceSlotDef {
  key: string;
  label: string;
  project: string;
  workspace?: string;
  isolation: Isolation;
  baseRef?: string;
  branch?: string;
  autoCommit?: boolean;
  /** 子流程：继承父运行的槽位 */
  inherit?: string;
}

export type Concurrency = "allow" | "skip_if_running" | "queue";

export interface WorkflowSettings {
  ownerAgentId: string;
  maxParallelNodes: number;
  runTimeoutMinutes?: number;
  concurrency: Concurrency;
}

export interface WorkflowDef {
  schemaVersion: number;
  inputs: PortDef[];
  workspaces: WorkspaceSlotDef[];
  nodes: NodeDef[];
  edges: EdgeDef[];
  settings: WorkflowSettings;
}

// ---------- 各类型配置 ----------

export interface TargetRef { type: "agent" | "squad"; id: string }
export interface AgentConfig {
  target: TargetRef;
  prompt: string;
  mode: "auto" | "plan";
  /** previous 只带上一步；all 带上从开始到现在 */
  context: "previous" | "all";
  /** 多条出线时谁来选。all 是都送下去。只有一条出线时忽略 */
  route: "auto" | "human" | "all";
  /** 做完后停住，等人确认才进入下一步 */
  humanConfirm?: boolean;
  allowTaskCommands?: boolean;
}
export interface ScriptConfig {
  command: string;
  shell: "default" | "sh" | "bash" | "pwsh" | "cmd";
  cwd: { slot: string; subdir?: string } | null;
  env: Record<string, string>;
  successExitCodes: number[];
  failOnNonZero: boolean;
  parseStdout: "none" | "text" | "json" | "lines";
  maxOutputKB: number;
}
export interface GateDecisionDef { id: string; label: string; primary?: boolean; requireComment?: boolean }
export interface GateConfig {
  title: string;
  body: string;
  decisions: GateDecisionDef[];
  form?: JsonSchema;
  showDiffOf: string[];
  notify?: { feishu?: boolean };
  onTimeout?: "approve" | "reject" | "fail";
}
export interface SwitchConfig {
  value?: string;
  cases: { port: string; when: string }[];
  defaultPort?: string;
  mode: "first" | "all";
}
export interface TransformConfig { assignments: { port: string; expr: string }[] }
export interface HttpConfig {
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  url: string;
  headers: Record<string, string>;
  body?: string;
  bodyType: "json" | "text" | "form" | "none";
  expectStatus: number[];
  responseType: "json" | "text";
}
export interface GitConfig {
  slot: string;
  op: "commit" | "squash" | "push" | "merge" | "create_branch" | "diff";
  message?: string;
  branch?: string;
  remote?: string;
  setUpstream?: boolean;
  skipVerify?: boolean;
}
export interface NotifyConfig { channel: "feishu" | "inbox"; text: string }
export interface SubflowConfig { workflowId: string; version?: number | null; inputs: Record<string, string> }
export interface MapConfig {
  items: string;
  workflowId: string;
  version?: number | null;
  itemInput: string;
  extraInputs: Record<string, string>;
  concurrency: number;
  failFast: boolean;
}
export interface TriggerConfig { sources: string[] }
export interface NoteConfig { text?: string; color?: string }

// ---------- 隐式端口（镜像 model.rs） ----------

export const IN_PORT = "$in";
export const OUT_PORT = "$out";
export const ERROR_PORT = "$error";
export const RESULT_PORT = "$result";
export const EXHAUSTED_PORT = "exhausted";

export function isImplicitPort(name: string): boolean {
  return name.startsWith("$");
}

export function isBranchingKind(kind: NodeKind): boolean {
  return kind === "agent";
}

export function defaultGateDecisions(labels?: { approve: string; changes: string; reject: string }): GateDecisionDef[] {
  return [
    { id: "approve", label: labels?.approve ?? "通过", primary: true },
    { id: "changes", label: labels?.changes ?? "打回修改", requireComment: true },
    { id: "reject", label: labels?.reject ?? "拒绝" },
  ];
}

export function effectiveInputs(_node: NodeDef): PortDef[] {
  return [];
}

export function branchPorts(_node: NodeDef): string[] {
  return [];
}

export function effectiveOutputs(_node: NodeDef, _def: Pick<WorkflowDef, "inputs">): PortDef[] {
  return [];
}

export function findPort(ports: PortDef[], name: string): PortDef | undefined {
  return ports.find((port) => port.name === name);
}

/** 画布和表单上看到的端口名。示例工作流用 label 放翻译，连线仍用 name。 */
export function portLabel(port: Pick<PortDef, "name" | "label">): string {
  const text = port.label?.trim();
  return text || port.name;
}

export function isValidNodeId(id: string): boolean {
  return /^[a-z][a-z0-9_]*$/.test(id) && id.length <= 48;
}

/** 端口名只要求去掉首尾空白后非空，中文和其它字符都可以。 */
export function isValidPortName(name: string | null | undefined): boolean {
  return typeof name === "string" && name.trim().length > 0;
}

// ---------- 端口类型颜色与兼容性 ----------

export const PORT_COLORS: Record<PortType, string> = {
  text: "var(--color-info)",
  markdown: "var(--color-purple)",
  number: "var(--color-warning)",
  boolean: "var(--color-success)",
  json: "var(--color-accent)",
  url: "var(--color-git-modified)",
  file: "var(--color-git-untracked)",
  files: "var(--color-git-added)",
  any: "var(--color-text-tertiary)",
};

/** 控制类端口（$in / $out / $error / 分支）的颜色 */
export const CONTROL_PORT_COLOR = "var(--color-text-tertiary)";

const COMPAT = compat as Record<string, string[]>;

export function isTypeCompatible(source: PortType, target: PortType): boolean {
  return COMPAT[source]?.includes(target) ?? false;
}

export function portType(ports: PortDef[], name: string): PortType {
  return findPort(ports, name)?.type ?? "any";
}

export function isCompatiblePorts(
  sourceNode: NodeDef,
  sourcePort: string,
  targetNode: NodeDef,
  targetPort: string,
  def: Pick<WorkflowDef, "inputs">,
): boolean {
  const from = findPort(effectiveOutputs(sourceNode, def), sourcePort);
  const to = findPort(effectiveInputs(targetNode), targetPort);
  if (!from || !to) return false;
  return isTypeCompatible(from.type, to.type);
}

// ---------- 默认值 / 创建 ----------

export function defaultConfig(kind: NodeKind): Record<string, unknown> {
  switch (kind) {
    case "trigger":
      return { sources: ["manual"] };
    case "agent":
      return {
        target: { type: "agent", id: "" },
        prompt: "",
        mode: "auto",
        context: "previous",
        route: "auto",
        humanConfirm: false,
        allowTaskCommands: false,
      };
    default:
      return {};
  }
}

export function defaultInputs(kind: NodeKind): PortDef[] {
  if (kind === "switch") return [{ name: "value", type: "any" }];
  return [];
}

/** 默认节点 id 前缀 */
export function idBase(kind: NodeKind): string {
  return kind === "output" ? "output" : kind;
}

export function uniqueNodeId(base: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  const clean = /^[a-z]/.test(base) ? base.replace(/[^a-z0-9_]/g, "_") : `n_${base.replace(/[^a-z0-9_]/g, "_")}`;
  let index = 1;
  let id = `${clean}_${index}`;
  // 第一个不带后缀
  if (!used.has(clean)) return clean;
  while (used.has(id)) {
    index += 1;
    id = `${clean}_${index}`;
  }
  return id;
}

export function uniqueEdgeId(taken: Iterable<string>): string {
  const used = new Set(taken);
  let index = used.size + 1;
  let id = `e${index}`;
  while (used.has(id)) {
    index += 1;
    id = `e${index}`;
  }
  return id;
}

export function createNode(
  kind: NodeKind,
  def: Pick<WorkflowDef, "nodes">,
  position: NodePosition,
  title = "",
): NodeDef {
  const id = uniqueNodeId(idBase(kind), def.nodes.map((node) => node.id));
  return {
    id,
    kind,
    title,
    config: defaultConfig(kind),
    workspaces: [],
    position,
  };
}

export function emptyWorkflowDef(): WorkflowDef {
  return {
    schemaVersion: 2,
    inputs: [],
    workspaces: [],
    nodes: [{
      id: "start",
      kind: "trigger",
      title: t("workflow.node.startTitle"),
      config: { sources: ["manual"] },
      position: { x: 80, y: 200 },
    }],
    edges: [],
    settings: { ownerAgentId: "miku", maxParallelNodes: 4, concurrency: "allow" },
  };
}

// ---------- 后端返回结构 ----------

export interface Workflow {
  id: string;
  name: string;
  slug: string;
  description: string;
  icon: string;
  publishedVersion: number | null;
  enabled: boolean;
  draftUpdatedAt: string;
  createdAt: string;
  updatedAt: string;
  hasUnpublishedChanges: boolean;
}

export interface WorkflowSummary extends Workflow {
  lastRunStatus: string | null;
  lastRunAt: string | null;
  nodeCount: number;
}

export interface VersionMeta { version: number; note: string; createdAt: string }

export interface Diagnostic {
  severity: "error" | "warning";
  code: string;
  nodeId?: string;
  edgeId?: string;
  /** 检查器控件，如 config:prompt、config:target、inputs:url */
  field?: string;
  message: string;
}

export interface WorkflowDetail {
  workflow: Workflow;
  draft: WorkflowDef;
  /** 最新发布版本的定义；没发布过为 null */
  published?: WorkflowDef | null;
  versions: VersionMeta[];
  diagnostics: Diagnostic[];
}

export interface SaveResult { updatedAt: string; diagnostics: Diagnostic[] }

export interface RenderPreview {
  rendered: string | null;
  error: string | null;
  /** 变量树（对象），用于 `{{` 自动补全 */
  variables: Record<string, unknown>;
}

export interface NodeTemplate {
  id: string;
  category: string;
  name: string;
  description: string;
  recommends: { skills?: string[]; mcp?: string[] } | null;
  nodes: NodeDef[];
  edges: EdgeDef[];
  builtin: boolean;
}

export interface WorkflowTemplateMeta {
  id: string;
  name: string;
  description: string;
  nodeCount: number;
}

export type RunStatus = "queued" | "running" | "waiting" | "succeeded" | "failed" | "cancelled";

export type NodeRunStatus =
  | "pending" | "ready" | "running" | "waiting_human" | "waiting_children"
  | "succeeded" | "failed" | "skipped" | "cancelled" | "reused";

export interface WorkflowRun {
  id: string;
  workflowId: string;
  /** 0 = 草稿试运行 */
  version: number;
  status: RunStatus | string;
  inputs: unknown;
  outputs: unknown;
  triggerSource: string;
  triggerRef: string | null;
  parentNodeRunId: string | null;
  mapIndex: number | null;
  extra: unknown;
  overviewSessionId: string;
  taskId: string | null;
  errorCode: string | null;
  error: string | null;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
}

export interface NodeRun {
  id: string;
  runId: string;
  nodeId: string;
  kind: string;
  iteration: number;
  attempt: number;
  status: NodeRunStatus | string;
  inputs: unknown;
  firedPorts: string[];
  /** 这一步实际选中的下一个节点 */
  nextNodeId?: string | null;
  carried: unknown;
  state: unknown;
  sessionId: string | null;
  repairCount: number;
  workspaceBefore: unknown;
  workspaceAfter: unknown;
  forkWorkspaceId: string | null;
  skipReason: string | null;
  errorCode: string | null;
  error: string | null;
  retryAt: string | null;
  deadlineAt: string | null;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  /** 这一步交给下一步的正文 */
  content?: string;
}

export interface ValueMeta {
  id: string;
  nodeRunId: string;
  nodeId: string;
  port: string;
  portType: string;
  size: number;
  source: string;
  preview: unknown;
}

export interface RunWorkspace {
  runId: string;
  slotKey: string;
  workspaceId: string;
  path: string;
  branch: string | null;
  projectId: string;
  baseCommit: string | null;
  headCommit: string | null;
  owned: boolean;
}

export interface GateDecisionRow {
  nodeRunId: string;
  decision: string;
  comment: string;
  form: unknown;
  source: string;
  decidedAt: string;
}

export interface RunDetail {
  run: WorkflowRun;
  workflow: Workflow;
  definition: WorkflowDef;
  nodeRuns: NodeRun[];
  values: ValueMeta[];
  workspaces: RunWorkspace[];
  gates: GateDecisionRow[];
  /** edgeId -> count */
  loopCounters: Record<string, number>;
  children: WorkflowRun[];
  /** nodeId -> { agentRuns, elapsedMs } */
  usage: Record<string, { agentRuns: number; elapsedMs: number }>;
}

export interface WorkflowEvent {
  id: string;
  runId: string;
  nodeRunId: string | null;
  seq: number;
  kind: string;
  message: string;
  data: unknown;
  createdAt: string;
}

export interface SecretName { name: string; description: string; updatedAt: string }

export interface NodeLogChunk { text: string; nextOffset: number; eof: boolean }

export interface WorkflowNodeValues { inputs: unknown; outputs: unknown }

export const RUN_TERMINAL = new Set(["succeeded", "failed", "cancelled"]);
export const NODE_RUN_TERMINAL = new Set(["succeeded", "failed", "skipped", "cancelled", "reused"]);

export function isRunTerminal(status: string): boolean {
  return RUN_TERMINAL.has(status);
}

/** 当前节点运行 = iteration 最大、其次 attempt 最大 */
export function currentNodeRun(nodeRuns: NodeRun[], nodeId: string): NodeRun | undefined {
  let best: NodeRun | undefined;
  for (const run of nodeRuns) {
    if (run.nodeId !== nodeId) continue;
    if (!best || run.iteration > best.iteration || (run.iteration === best.iteration && run.attempt > best.attempt)) best = run;
  }
  return best;
}

export function nodeRunHistory(nodeRuns: NodeRun[], nodeId: string): NodeRun[] {
  return nodeRuns
    .filter((run) => run.nodeId === nodeId)
    .sort((a, b) => a.iteration - b.iteration || a.attempt - b.attempt);
}

export const WF_RUN_PREFIX = "wf-run:";
export const WF_NODE_PREFIX = "wf-node:";

export function isWorkflowRunSession(sessionId: string | null | undefined): boolean {
  return Boolean(sessionId?.startsWith(WF_RUN_PREFIX));
}
export function isWorkflowNodeSession(sessionId: string | null | undefined): boolean {
  return Boolean(sessionId?.startsWith(WF_NODE_PREFIX));
}
export function workflowRunIdFromSession(sessionId: string): string {
  return sessionId.slice(WF_RUN_PREFIX.length);
}

/** 后端 QuickAction.action 里的审批动作 */
export interface GateQuickAction {
  type: "workflow.gate";
  nodeRunId: string;
  decision: string;
  requireComment?: boolean;
  form?: unknown;
}

export function asGateAction(action: unknown): GateQuickAction | null {
  if (!action || typeof action !== "object") return null;
  const value = action as Record<string, unknown>;
  if (value.type !== "workflow.gate" || typeof value.nodeRunId !== "string" || typeof value.decision !== "string") return null;
  return value as unknown as GateQuickAction;
}
