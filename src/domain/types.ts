export type RuntimeStatus = "online" | "offline" | "not_found";
export type RunStatus = "queued" | "running" | "completed" | "failed" | "stopped";
export type ResourceKind = "skill" | "mcp";

export interface Project {
  id: string;
  name: string;
  note: string;
  workspaceCount: number;
  createdAt: string;
  openedAt: string;
  /** 归档时间；为空表示未归档。 */
  archivedAt?: string | null;
}

export interface Workspace {
  id: string;
  projectId: string;
  sessionId?: string | null;
  sourceWorkspaceId?: string | null;
  name: string;
  kind: "git" | "folder";
  sourcePath: string;
  workPath: string;
  branch: string | null;
  isolationMode: "worktree" | "copy" | "direct";
  createdAt: string;
}

export interface WorkspaceCandidate {
  path: string;
  name: string;
  kind: "git" | "folder";
  branches: string[];
  currentBranch: string | null;
}

export interface FileEntry {
  name: string;
  path: string;
  isDirectory: boolean;
  depth: number;
  symlinkTarget?: string;
  mtime?: number;
  size?: number;
  readonly?: boolean;
}

export interface FileContent {
  workspaceId: string;
  path: string;
  content: string;
  language: string;
  size: number;
  readOnly: boolean;
  media?: "" | "image" | "audio" | "video";
}

export interface GitFileStatus {
  path: string;
  indexStatus: string;
  worktreeStatus: string;
  staged: boolean;
}

export interface GitIndexFile {
  inRepo: boolean;
  tracked: boolean;
  binary: boolean;
  content: string;
}

export interface GitLineBlame {
  committed: boolean;
  hash: string;
  authorName: string;
  authorEmail: string;
  authorTime: number;
  subject: string;
}

export interface GitHistoryItem {
  hash: string;
  parents: string[];
  subject: string;
  author: string;
  committedAt: string;
  refs: string[];
}

export interface GitHistoryPage {
  items: GitHistoryItem[];
  hasMore: boolean;
}

export interface GitCommitFile {
  path: string;
  status: string;
}

export interface GitRepositoryStatus {
  workspaceId: string;
  name: string;
  path: string;
  branch: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  hasRemote: boolean;
  merging: boolean;
  changes: GitFileStatus[];
  branches: string[];
  remoteBranches: string[];
}

export interface GitCommitInput {
  workspaceId: string;
  message: string;
}

export interface Runtime {
  id: string;
  name: string;
  provider: string;
  command: string;
  args: string[];
  path: string | null;
  version: string | null;
  status: RuntimeStatus;
  maxConcurrent: number;
  protocol?: string;
  apiUrl?: string;
  models?: string[];
  kind?: "builtin" | "profile" | "gateway" | string;
  description?: string;
}

export interface CustomRuntimeInput {
  id?: string;
  name: string;
  command: string;
  args: string[];
  maxConcurrent: number;
  provider?: string;
  protocol?: string;
  apiUrl?: string;
  apiKey?: string;
  models?: string[];
  env?: Record<string, string>;
  kind?: "profile" | "gateway" | string;
  description?: string;
  wireApi?: string;
  keyStyle?: string;
}

export interface CustomRuntimeForm {
  id: string;
  name: string;
  command: string;
  args: string[];
  maxConcurrent: number;
  provider: string;
  protocol: string;
  apiUrl: string;
  apiKey: string;
  models: string[];
  env: Record<string, string>;
  kind: string;
  description: string;
  wireApi: string;
  keyStyle: string;
  hasApiKey: boolean;
}

export interface Agent {
  id: string;
  name: string;
  avatar: string;
  description: string;
  instructions: string;
  runtimeId: string;
  model: string;
  maxConcurrentTasks: number;
  skillIds: string[];
  mcpIds: string[];
  systemKey?: string | null;
  systemInstructions?: string | null;
}

export function isMiku(agent: Pick<Agent, "id" | "systemKey">): boolean {
  return agent.systemKey === "miku";
}

export interface Squad {
  id: string;
  name: string;
  description: string;
  /** 处理分配给该小队的任务时，注入到队长 prompt。 */
  instructions: string;
  leaderAgentId: string;
  memberAgentIds: string[];
}

export interface ChatSession {
  id: string;
  projectId: string | null;
  /** 会话关联的全部需求；projectId 保留为首项兼容值。 */
  projectIds?: string[];
  title: string;
  targetType: "agent" | "squad";
  targetId: string;
  createdAt: string;
  updatedAt: string;
}

export type TaskStatus =
  | "backlog"
  | "todo"
  | "in_progress"
  | "in_review"
  | "done"
  | "blocked"
  | "cancelled";

export type TaskPriority = "none" | "urgent" | "high" | "medium" | "low";

export interface TaskChecklistItem {
  id: string;
  text: string;
  done: boolean;
}

export interface Task {
  id: string;
  sessionId: string;
  title: string;
  description: string;
  agentId: string;
  status: TaskStatus;
  createdAt: string;
  updatedAt: string;
  parentTaskId?: string | null;
  priority?: TaskPriority;
  labels?: string[];
  checklist?: TaskChecklistItem[];
  prUrl?: string;
}

export interface TaskInput {
  title: string;
  description: string;
  agentId: string;
  projectId?: string | null;
  projectIds?: string[];
  status?: TaskStatus;
  sourceSessionId?: string | null;
  parentTaskId?: string | null;
}

export interface TaskRunRecord {
  id: string;
  agentId: string;
  runtimeId: string;
  status: string;
  assignment: string;
  createdAt: string;
}

export interface EditorSelectionContext {
  id: string;
  workspaceId: string;
  path: string;
  startLine: number;
  endLine: number;
  text: string;
  isDirectory?: boolean;
  source?: "terminal";
}

export interface ChatMessage {
  id: string;
  sessionId: string;
  turnId?: string | null;
  role: "user" | "assistant" | "system";
  content: string;
  runId: string | null;
  agentId?: string | null;
  audienceType?: "user" | "agent" | null;
  audienceId?: string | null;
  targetType?: "agent" | "squad" | null;
  targetId?: string | null;
  failureReason: string | null;
  elapsedMs: number | null;
  messageKind: "message" | "no_response" | "assignment";
  quickActions: QuickAction[];
  timeline: TimelineItem[];
  attachments: MessageAttachment[];
  createdAt: string;
  feedback?: "up" | "down" | null;
  usage?: {
    inputTokens: number;
    outputTokens: number;
    cacheReadTokens: number;
    cacheWriteTokens: number;
  } | null;
}

export type TimelineItemType = "text" | "thinking" | "tool_use" | "tool_result" | "error" | "plan" | "permission";

export interface TimelineItem {
  seq: number;
  eventType: TimelineItemType;
  callId: string | null;
  tool: string | null;
  content: string | null;
  input: Record<string, unknown> | null;
  output: string | null;
  createdAt: string;
  replacesFinalText?: boolean;
  parentCallId?: string | null;
  status?: string | null;
}

export interface QuickAction {
  label: string;
  prompt: string;
  primary: boolean;
}

export interface MessageAttachment {
  id: string;
  name: string;
  path: string;
  mimeType: string;
  size: number;
}

export interface MessageAttachmentInput {
  name: string;
  path: string;
  mimeType: string;
  size: number;
}

export interface AgentRun {
  id: string;
  sessionId: string;
  agentId: string;
  runtimeId: string;
  parentRunId: string | null;
  assignment: string;
  status: RunStatus;
  createdAt: string;
}

export interface RunEvent {
  runId: string;
  sessionId: string;
  agentId: string;
  status: RunStatus;
  chunk: string | null;
  error: string | null;
  timelineItem: TimelineItem | null;
  replacesFinalText?: boolean;
  elapsedMs: number | null;
  eventKind?: string | null;
  messageId?: string | null;
  filePath?: string | null;
  snapshotContent?: string | null;
  affectedFile?: string | null;
  workspaceId?: string | null;
}

export interface GlobalResource {
  id: string;
  kind: ResourceKind;
  owner: string;
  /** global 或 workspace。 */
  scope: "global" | "workspace";
  /** 来源显示名，例如 Claude Code 或 项目名 · .github/skills。 */
  sourceLabel: string;
  name: string;
  description: string;
  sourcePath: string;
  enabled: boolean;
  /** 技能取 SKILL.md 的修改时间，MCP 取配置文件的修改时间。 */
  updatedAt: string;
  /** MCP 传输：stdio 本地命令，http / sse 远程。技能为空。 */
  transport: "" | "stdio" | "http" | "sse";
  /** 命令或 URL，不含密钥。 */
  summary: string;
  needsAuth: boolean;
  /** ready | needs_auth | needs_input | disabled | incomplete。技能为空。 */
  configStatus: "" | "ready" | "needs_auth" | "needs_input" | "disabled" | "incomplete";
}

export interface ResourceSource {
  owner: string;
  kind: ResourceKind;
  label: string;
  scope: "global" | "workspace";
  path: string;
  exists: boolean;
}

export interface PromptFile {
  path: string;
  name: string;
  description: string;
  sourceLabel: string;
}

export interface CatalogItem {
  kind: ResourceKind;
  id: string;
  name: string;
  description: string;
  source: string;
  url: string;
  install: string;
}

export interface McpInput {
  name: string;
  previousName?: string;
  owner?: string;
  transport: "stdio" | "http" | "sse";
  command: string;
  cwd?: string;
  envFile?: string;
  args: string[];
  url: string;
  envText: string;
  headersText: string;
  needsAuth: boolean;
  enabled: boolean;
}

export interface McpForm extends McpInput {
  owner: string;
}

export interface McpInputs {
  ids: string[];
  values: Record<string, string>;
}

export interface McpCapability {
  name: string;
  title: string;
  description: string;
  uri: string;
}

export interface McpProbe {
  ok: boolean;
  status: string;
  serverName: string;
  serverVersion: string;
  protocolVersion: string;
  instructions: string;
  tools: McpCapability[];
  resources: McpCapability[];
  prompts: McpCapability[];
  log: string;
  error: string;
  durationMs: number;
  missingInputs: string[];
}

export interface McpToolsCacheEntry {
  id: string;
  owner: string;
  name: string;
  probe: McpProbe;
  updatedAtMs: number;
}

export interface McpToolsCacheEntry {
  id: string;
  owner: string;
  name: string;
  probe: McpProbe;
  updatedAtMs: number;
}

export interface DashboardStats {
  running: number;
  queued: number;
  completedToday: number;
  failedToday: number;
}

export interface Automation {
  id: string;
  name: string;
  projectId: string | null;
  targetType: "agent" | "squad";
  targetId: string;
  prompt: string;
  triggerType: "manual" | "schedule" | "webhook" | "api";
  intervalMinutes: number | null;
  eventName: string | null;
  enabled: boolean;
  lastRunAt: string | null;
  createdAt: string;
  triggerId: string | null;
  cronExpression: string | null;
  timezone: string | null;
  nextRunAt: string | null;
  webhookToken: string | null;
  lastRunStatus: string | null;
}

export type AutomationInput = Omit<
  Automation,
  "id" | "lastRunAt" | "createdAt" | "triggerId" | "nextRunAt" | "webhookToken" | "lastRunStatus"
> & { id?: string; signingSecret?: string | null };

export interface AutomationRun {
  id: string;
  automationId: string;
  source: "schedule" | "manual" | "webhook" | "api";
  status: "pending" | "running" | "skipped" | "completed" | "failed";
  plannedAt: string | null;
  triggeredAt: string;
  completedAt: string | null;
  failureReason: string | null;
  reasonCode: string | null;
}

export interface WebhookDelivery {
  id: string;
  dedupeKey: string;
  event: string | null;
  status: "queued" | "dispatched" | "rejected" | "ignored" | "failed";
  automationRunId: string | null;
  reasonCode: string | null;
  createdAt: string;
}

export interface StatisticGroup {
  /** 按智能体分组时为 agent id，用来取头像 */
  id?: string;
  name: string;
  total: number;
  completed: number;
  failed: number;
  seconds: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  costUsd: number;
}

export interface UsageDay {
  date: string;
  total: number;
  failed: number;
  seconds: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  costUsd: number;
}

export interface RunStatistics {
  total: number;
  completed: number;
  failed: number;
  stopped: number;
  successRate: number;
  averageDurationSeconds: number;
  totalSeconds: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  costUsd: number;
  daily: UsageDay[];
  byRuntime: StatisticGroup[];
  byAgent: StatisticGroup[];
}

export interface AgentActivityRecord {
  id: string;
  status: string;
  assignment: string;
  createdAt: string;
  completedAt: string | null;
  sessionId: string;
  sessionTitle: string;
  taskId: string | null;
  projectId: string | null;
}

export interface InboxItem {
  id: string;
  sessionId: string;
  projectId: string | null;
  taskId: string | null;
  title: string;
  preview: string;
  status: "completed" | "failed" | "stopped" | string;
  read: boolean;
  readMessageId: string | null;
  unreadCount: number;
  createdAt: string;
  archivedAt?: string | null;
}

export interface InboxInput {
  sessionId: string;
  projectId?: string | null;
  taskId?: string | null;
  title: string;
  preview: string;
  status: string;
}

export interface BootstrapData {
  projects: Project[];
  workspaces: Workspace[];
  runtimes: Runtime[];
  agents: Agent[];
  squads: Squad[];
  sessions: ChatSession[];
  tasks: Task[];
  resources: GlobalResource[];
  stats: DashboardStats;
  automations: Automation[];
  inbox: InboxItem[];
  settings: Record<string, string>;
  /** 进行中的冲突合并（每个对应一个专属临时 Worktree 和一个聊天会话） */
  mergeSessions?: MergeSession[];
}

export interface MergeSession {
  id: string;
  projectId: string;
  /** 解决冲突用的聊天会话 */
  sessionId: string;
  /** 临时合并 Worktree 对应的工作区 */
  workspaceId: string;
  /** 发起合并的原工作区 */
  sourceWorkspaceId: string;
  sourceLabel: string;
  targetBranch: string;
  targetRemote?: string | null;
  push: boolean;
  conflicts: string[];
  createdAt: string;
}

export interface MergePreview {
  sourceLabel: string;
  sourceCommit: string;
  /** 有未提交改动：这些改动不会被合并 */
  dirty: boolean;
  hasRemote: boolean;
  /** Worktree 创建时的来源分支 */
  origin?: { baseRef: string; baseCommit: string; commits: number; files: number } | null;
  target?: {
    branch: string;
    remote?: string | null;
    display: string;
    exists: boolean;
    hasRemoteBranch: boolean;
    commits: number;
    files: number;
    /** fast_forward / merge / conflict / up_to_date / missing */
    plan: string;
    targetAhead: number;
    conflictFiles: string[];
    checkedOutAt?: string | null;
  } | null;
}

export interface MergeIntoResult {
  status: "up_to_date" | "merged" | "conflict";
  sourceLabel: string;
  targetDisplay: string;
  commit?: string | null;
  fastForward: boolean;
  pushed: boolean;
  localUpdated: boolean;
  notes: string[];
  conflicts: string[];
  mergeSession?: MergeSession | null;
  session?: ChatSession | null;
  workspace?: Workspace | null;
}

export interface MergePullResult {
  status: "up_to_date" | "updated" | "conflict";
  conflicts: string[];
}

export interface MergeFinishResult {
  commit: string;
  sourceLabel: string;
  targetDisplay: string;
  pushed: boolean;
  localUpdated: boolean;
  notes: string[];
}
