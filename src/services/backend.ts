import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  ActiveDiffData,
  SessionDiffData,
  SnapshotEntry,
} from "../domain/diff";
import type {
  BootstrapData,
  CatalogItem,
  ChatMessage,
  ChatSession,
  ChannelInstallation,
  ChannelInstallationInput,
  ChannelUser,
  MessageAttachmentInput,
  CustomRuntimeForm,
  CustomRuntimeInput,
  FileContent,
  FileEntry,
  Project,
  Agent,
  Automation,
  InboxInput,
  InboxItem,
  AutomationInput,
  AutomationRun,
  GlobalResource,
  McpForm,
  McpInput,
  McpInputs,
  McpProbe,
  McpToolsCacheEntry,
  PromptFile,
  ResourceKind,
  ResourceSource,
  GitCommitInput,
  GitIndexFile,
  GitLineBlame,
  GitCommitFile,
  GitHistoryPage,
  GitRepositoryStatus,
  MergeFinishResult,
  MergePullResult,
  MergeIntoResult,
  MergePreview,
  Squad,
  Task,
  TaskInput,
  AgentActivityRecord,
  TaskRunRecord,
  TaskStatus,
  RunStatistics,
  Workspace,
  WorkspaceCandidate,
  WebhookDelivery,
} from "../domain/types";
import { CLI_CATALOG } from "../domain/cli-catalog";

const now = new Date().toISOString();

const previewData: BootstrapData = {
  projects: [],
  workspaces: [],
  runtimes: [
    { id: "cursor", name: "Cursor", provider: "cursor", command: "cursor-agent", args: [], path: "/usr/local/bin/cursor-agent", version: "latest", status: "online", maxConcurrent: 3 },
    { id: "claude", name: "Claude Code", provider: "claude", command: "claude", args: [], path: null, version: null, status: "not_found", maxConcurrent: 2 },
    { id: "codex", name: "Codex", provider: "codex", command: "codex", args: [], path: null, version: null, status: "not_found", maxConcurrent: 2 },
  ],
  agents: [
    {
      id: "miku",
      name: "Miku",
      avatar: "/miku-avatar.jpg",
      description: "",
      instructions: "",
      runtimeId: "cursor",
      model: "auto",
      maxConcurrentTasks: 3,
      skillIds: ["security-review"],
      mcpIds: [],
      systemKey: "miku",
      systemInstructions: "你是 Miku，Multara 的默认总管（Miku）。",
    },
  ],
  squads: [
    {
      id: "preview-squad",
      name: "产品开发小队",
      description: "把需求拆给界面和实现。",
      instructions: "优先确认范围，再分给对应智能体。",
      leaderAgentId: "miku",
      memberAgentIds: ["miku"],
    },
  ],
  sessions: [
    { id: "welcome", projectId: null, title: "开始使用 Multara", targetType: "agent", targetId: "miku", createdAt: now, updatedAt: now },
  ],
  tasks: [],
  resources: [
    { id: "figma-to-code", kind: "skill", owner: "cursor", scope: "global", sourceLabel: "Cursor", name: "figma-to-code", description: "从设计稿实现界面。对照 Figma 节点、现有组件和设计 token，把页面还原成可维护的代码，并检查间距、颜色和交互状态是否对齐。", sourcePath: "~/.cursor/skills/figma-to-code", enabled: true, updatedAt: now, transport: "", summary: "", needsAuth: false, configStatus: "" },
    { id: "browser-test", kind: "skill", owner: "cursor", scope: "global", sourceLabel: "Cursor", name: "browser-test", description: "浏览器回归测试", sourcePath: "~/.cursor/skills/browser-test", enabled: true, updatedAt: now, transport: "", summary: "", needsAuth: false, configStatus: "" },
    { id: "security-review", kind: "skill", owner: "multara", scope: "global", sourceLabel: "Multara", name: "security-review", description: "代码安全审查", sourcePath: "~/.multara/skills/security-review", enabled: true, updatedAt: now, transport: "", summary: "", needsAuth: false, configStatus: "" },
    { id: "figma", kind: "mcp", owner: "cursor", scope: "global", sourceLabel: "Cursor", name: "figma", description: "https://mcp.figma.com/mcp", sourcePath: "~/.cursor/mcp.json", enabled: true, updatedAt: now, transport: "http", summary: "https://mcp.figma.com/mcp", needsAuth: true, configStatus: "needs_auth" },
    { id: "browser", kind: "mcp", owner: "cursor", scope: "global", sourceLabel: "Cursor", name: "browser", description: "npx @playwright/mcp", sourcePath: "~/.cursor/mcp.json", enabled: true, updatedAt: now, transport: "stdio", summary: "npx @playwright/mcp", needsAuth: false, configStatus: "ready" },
    { id: "project-files", kind: "mcp", owner: "multara", scope: "global", sourceLabel: "Multara", name: "project-files", description: "npx @modelcontextprotocol/server-filesystem", sourcePath: "~/.multara/mcp.json", enabled: true, updatedAt: now, transport: "stdio", summary: "npx @modelcontextprotocol/server-filesystem", needsAuth: false, configStatus: "ready" },
  ],
  stats: { running: 0, queued: 0, completedToday: 0, failedToday: 0 },
  automations: [],
  inbox: [],
  settings: {
    language: "system",
    theme: "dark",
    fileIconTheme: "material-icon-theme",
    gitExecutable: "git",
    isolationRoot: "~/.multara",
    autoUpdate: "true",
    runTimeoutMinutes: "0",
    webhookPort: "43127",
    formatOnSave: "false",
    stickyScroll: "true",
    branchPrefix: "feat",
    branchPrompt: "",
    commitPrompt: "",
    branchGenMode: "agent",
    branchAgentId: "",
    commitGenMode: "agent",
    commitAgentId: "",
    genApiUrl: "",
    genApiModel: "",
    genApiKey: "",
  },
};

function inTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function bootstrap(): Promise<BootstrapData> {
  if (!inTauri()) return structuredClone(previewData);
  return invoke<BootstrapData>("bootstrap");
}

export type TaskInboxSurfaces = Pick<BootstrapData, "tasks" | "sessions" | "inbox">;

export async function listTaskInboxSurfaces(): Promise<TaskInboxSurfaces> {
  if (!inTauri()) {
    return structuredClone({ tasks: previewData.tasks, sessions: previewData.sessions, inbox: previewData.inbox });
  }
  return invoke<TaskInboxSurfaces>("list_task_inbox_surfaces");
}

/** 智能体/小队页轻量刷新，避免整包 bootstrap（含 detect）卡住首屏。 */
export async function listAgentsAndSquads(): Promise<{
  agents: BootstrapData["agents"];
  squads: BootstrapData["squads"];
}> {
  if (!inTauri()) {
    return {
      agents: structuredClone(previewData.agents),
      squads: structuredClone(previewData.squads),
    };
  }
  const [agents, squads] = await invoke<[BootstrapData["agents"], BootstrapData["squads"]]>(
    "list_agents_and_squads",
  );
  return { agents, squads };
}

export async function detectRuntimes(): Promise<BootstrapData["runtimes"]> {
  if (!inTauri()) return structuredClone(previewData.runtimes);
  return invoke<BootstrapData["runtimes"]>("detect_runtimes");
}

export async function configureRuntime(
  runtimeId: string,
  maxConcurrent: number,
): Promise<BootstrapData["runtimes"]> {
  if (!inTauri()) {
    const runtime = previewData.runtimes.find((item) => item.id === runtimeId);
    if (runtime) runtime.maxConcurrent = maxConcurrent;
    return structuredClone(previewData.runtimes);
  }
  return invoke<BootstrapData["runtimes"]>("configure_runtime", { runtimeId, maxConcurrent });
}

export async function saveCustomRuntime(input: CustomRuntimeInput): Promise<BootstrapData["runtimes"]> {
  if (!inTauri()) throw new Error("请在桌面应用中配置自定义运行时");
  return invoke<BootstrapData["runtimes"]>("save_custom_runtime", { input });
}

export async function getCustomRuntime(runtimeId: string): Promise<CustomRuntimeForm> {
  if (!inTauri()) {
    return {
      id: runtimeId,
      name: "Custom",
      command: "claude",
      args: [],
      maxConcurrent: 2,
      provider: "claude",
      protocol: "anthropic",
      apiUrl: "",
      apiKey: "",
      models: [],
      env: {},
      kind: "profile",
      description: "",
      wireApi: "responses",
      apiFormat: "",
      keyStyle: "bearer",
      gatewayOptions: {
        compatibilityMode: "auto",
        fullUrl: false,
        customUserAgent: "",
        customHeaders: {},
        reasoningEffort: "auto",
        promptCache: true,
      },
      hasApiKey: false,
    };
  }
  return invoke<CustomRuntimeForm>("get_custom_runtime", { runtimeId });
}

export async function probeRuntimeCommand(command: string, provider: string): Promise<string | null> {
  if (!inTauri()) return command ? `/usr/local/bin/${command}` : null;
  return invoke<string | null>("probe_runtime_command", { command, provider });
}

export async function testRuntimeConnection(input: {
  id?: string;
  provider: string;
  command: string;
  args: string[];
  apiUrl: string;
  apiKey: string;
  model: string;
  wireApi: string;
  apiFormat: string;
  keyStyle: string;
  gatewayOptions: import("../domain/types").GatewayOptions;
  env: Record<string, string>;
}): Promise<string> {
  if (!inTauri()) throw new Error("请在桌面应用中测试连接");
  return invoke<string>("test_runtime_connection", { input });
}

export async function deleteCustomRuntime(runtimeId: string): Promise<BootstrapData["runtimes"]> {
  if (!inTauri()) return structuredClone(previewData.runtimes);
  return invoke<BootstrapData["runtimes"]>("delete_custom_runtime", { runtimeId });
}

export async function scanResources(): Promise<GlobalResource[]> {
  if (!inTauri()) return structuredClone(previewData.resources);
  return invoke<GlobalResource[]>("scan_resources");
}

function previewSkill(name: string, description: string, owner = "multara"): GlobalResource {
  return {
    id: `skill:${owner}:${name}`,
    kind: "skill",
    owner,
    scope: owner.startsWith("ws/") ? "workspace" : "global",
    sourceLabel: owner === "multara" ? "Multara" : owner,
    name,
    description: description || "用户创建的 Skill。",
    sourcePath: `~/.multara/skills/${name}`,
    enabled: true,
    updatedAt: new Date().toISOString(),
    transport: "",
    summary: "",
    needsAuth: false,
    configStatus: "",
  };
}

export async function listResourceSources(kind: ResourceKind): Promise<ResourceSource[]> {
  if (!inTauri()) {
    return CLI_CATALOG
      .filter((item) => (kind === "skill" ? item.skillsHome : item.mcpHome || item.mcpSpecial))
      .map((item) => ({
        owner: item.id,
        kind,
        label: item.label,
        scope: "global" as const,
        path: kind === "skill" ? `~/${item.skillsHome}` : (item.mcpHome ? `~/${item.mcpHome}` : "vscode"),
        exists: true,
      }));
  }
  return invoke<ResourceSource[]>("list_resource_sources", { kind });
}

export async function searchCatalog(kind: ResourceKind, query: string): Promise<CatalogItem[]> {
  if (!inTauri()) return [];
  return invoke<CatalogItem[]>("search_catalog", { kind, query });
}

export async function revealResource(resourceId: string): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能打开本机路径");
  await invoke("reveal_resource", { resourceId });
}

export async function createSkill(name: string, description: string, body: string, owner = "multara"): Promise<GlobalResource> {
  if (!inTauri()) {
    const folder = name.trim().replace(/\s+/g, "-") || "skill";
    const created = previewSkill(folder, description.trim(), owner);
    previewData.resources.push(created);
    return created;
  }
  return invoke<GlobalResource>("create_skill", { name, description, body, owner });
}

export async function generateSkill(requirement: string, owner = "multara"): Promise<GlobalResource> {
  if (!inTauri()) {
    const created = previewSkill("generated-skill", requirement.trim(), owner);
    previewData.resources.push(created);
    return created;
  }
  return invoke<GlobalResource>("generate_skill", { requirement, owner });
}

export async function importSkillFromFolder(path: string, owner = "multara"): Promise<GlobalResource> {
  if (!inTauri()) throw new Error("浏览器预览不能导入本机文件夹");
  return invoke<GlobalResource>("import_skill_from_folder", { path, owner });
}

export async function importSkillFromUrl(url: string, owner = "multara"): Promise<GlobalResource> {
  if (!inTauri()) throw new Error("浏览器预览不能从 URL 安装 Skill");
  return invoke<GlobalResource>("import_skill_from_url", { url, owner });
}

export async function deleteSkill(resourceId: string): Promise<GlobalResource[]> {
  if (!inTauri()) {
    previewData.resources = previewData.resources.filter((item) => item.id !== resourceId);
    return [...previewData.resources];
  }
  return invoke<GlobalResource[]>("delete_skill", { resourceId });
}

export async function setSkillEnabled(resourceId: string, enabled: boolean): Promise<GlobalResource[]> {
  if (!inTauri()) {
    const item = previewData.resources.find((entry) => entry.id === resourceId);
    if (item) item.enabled = enabled;
    return [...previewData.resources];
  }
  return invoke<GlobalResource[]>("set_skill_enabled", { resourceId, enabled });
}

export async function listPromptFiles(): Promise<PromptFile[]> {
  if (!inTauri()) return [];
  return invoke<PromptFile[]>("list_prompt_files");
}

export async function convertPromptToSkill(path: string, owner = "multara"): Promise<GlobalResource> {
  if (!inTauri()) throw new Error("浏览器预览不能转换提示文件");
  return invoke<GlobalResource>("convert_prompt_to_skill", { path, owner });
}

const previewSkillFiles: FileEntry[] = [
  { name: "SKILL.md", path: "SKILL.md", isDirectory: false, depth: 0 },
  { name: "references", path: "references", isDirectory: true, depth: 0 },
];

export async function listSkillDirectory(resourceId: string, path?: string): Promise<FileEntry[]> {
  if (!inTauri()) return path ? [{ name: "checklist.md", path: "references/checklist.md", isDirectory: false, depth: 0 }] : previewSkillFiles;
  return invoke<FileEntry[]>("list_skill_directory", { resourceId, path: path || null });
}

export async function readSkillFile(resourceId: string, path: string): Promise<FileContent> {
  if (!inTauri()) {
    return {
      workspaceId: resourceId,
      path,
      content: path.endsWith(".md") ? "# 代码安全审查\n\n检查权限、输入和依赖。\n" : "",
      language: path.endsWith(".md") ? "markdown" : "plaintext",
      size: 24,
      readOnly: true,
    };
  }
  return invoke<FileContent>("read_skill_file", { resourceId, path });
}

export async function writeSkillFile(resourceId: string, path: string, content: string): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能写入本机文件");
  await invoke("write_skill_file", { resourceId, path, content });
}

export async function skillFileCount(resourceId: string): Promise<number> {
  if (!inTauri()) return resourceId ? 2 : 0;
  return invoke<number>("skill_file_count", { resourceId });
}

function previewMcpStatus(item: GlobalResource): GlobalResource["configStatus"] {
  if (!item.enabled) return "disabled";
  if (!item.summary) return "incomplete";
  if (item.needsAuth) return "needs_auth";
  return "ready";
}

export async function saveMcp(input: McpInput): Promise<GlobalResource[]> {
  if (!inTauri()) {
    const owner = input.owner || "multara";
    const summary = input.transport === "stdio" ? [input.command.trim(), ...input.args].filter(Boolean).join(" ") : input.url.trim();
    const existing = previewData.resources.find((item) => item.kind === "mcp" && item.owner === owner && item.name === (input.previousName || input.name));
    const next: GlobalResource = {
      id: `mcp:${owner}:${input.name}`,
      kind: "mcp",
      owner,
      scope: owner.startsWith("ws/") ? "workspace" : "global",
      sourceLabel: owner === "multara" ? "Multara" : owner,
      name: input.name,
      description: summary,
      sourcePath: "~/.multara/mcp.json",
      enabled: input.enabled,
      updatedAt: new Date().toISOString(),
      transport: input.transport,
      summary,
      needsAuth: input.needsAuth,
      configStatus: "ready",
    };
    next.configStatus = previewMcpStatus(next);
    if (existing) Object.assign(existing, next);
    else previewData.resources.push(next);
    return [...previewData.resources];
  }
  return invoke<GlobalResource[]>("save_mcp", { input });
}

export async function installMcp(name: string, entryJson: string, owner = "multara"): Promise<GlobalResource[]> {
  if (!inTauri()) throw new Error("浏览器预览不能安装 MCP");
  return invoke<GlobalResource[]>("install_mcp", { name, entryJson, owner });
}

export async function deleteMcp(name: string, owner = "multara"): Promise<GlobalResource[]> {
  if (!inTauri()) {
    previewData.resources = previewData.resources.filter((item) => !(item.kind === "mcp" && item.owner === owner && item.name === name));
    return [...previewData.resources];
  }
  return invoke<GlobalResource[]>("delete_mcp", { name, owner });
}

export async function setMcpEnabled(name: string, enabled: boolean, owner = "multara"): Promise<GlobalResource[]> {
  if (!inTauri()) {
    const item = previewData.resources.find((entry) => entry.kind === "mcp" && entry.owner === owner && entry.name === name);
    if (item) {
      item.enabled = enabled;
      item.configStatus = previewMcpStatus(item);
    }
    return [...previewData.resources];
  }
  return invoke<GlobalResource[]>("set_mcp_enabled", { name, enabled, owner });
}

export async function authorizeMcp(name: string, owner = "multara"): Promise<GlobalResource[]> {
  if (!inTauri()) throw new Error("浏览器预览不能授权 MCP");
  return invoke<GlobalResource[]>("authorize_mcp", { name, owner });
}

export async function signOutMcp(name: string): Promise<GlobalResource[]> {
  if (!inTauri()) return [...previewData.resources];
  return invoke<GlobalResource[]>("sign_out_mcp", { name });
}

export async function getMcp(name: string, owner = "multara"): Promise<McpForm> {
  if (!inTauri()) {
    return {
      name,
      owner,
      transport: "stdio",
      command: "npx",
      cwd: "",
      envFile: "",
      args: ["@modelcontextprotocol/server-filesystem"],
      url: "",
      envText: "",
      headersText: "",
      needsAuth: false,
      enabled: true,
    };
  }
  return invoke<McpForm>("get_mcp", { name, owner });
}

export async function getMcpJson(name: string, owner = "multara"): Promise<string> {
  if (!inTauri()) return "{}";
  return invoke<string>("get_mcp_json", { name, owner });
}

export async function getMcpInputs(name: string, owner = "multara"): Promise<McpInputs> {
  if (!inTauri()) return { ids: [], values: {} };
  return invoke<McpInputs>("get_mcp_inputs", { name, owner });
}

export async function saveMcpInputs(name: string, values: Record<string, string>): Promise<void> {
  if (!inTauri()) return;
  await invoke("save_mcp_inputs", { name, values });
}

export async function probeMcp(name: string, owner = "multara"): Promise<McpProbe> {
  if (!inTauri()) {
    return {
      ok: true,
      status: "running",
      serverName: name,
      serverVersion: "preview",
      protocolVersion: "2025-06-18",
      instructions: "",
      tools: [{ name: "list", title: "List", description: "预览工具", uri: "" }],
      resources: [],
      prompts: [],
      log: "preview probe",
      error: "",
      durationMs: 12,
      missingInputs: [],
    };
  }
  return invoke<McpProbe>("probe_mcp", { name, owner });
}

export async function listMcpToolCache(): Promise<McpToolsCacheEntry[]> {
  if (!inTauri()) return [];
  return invoke<McpToolsCacheEntry[]>("list_mcp_tool_cache");
}

/** 后台串行刷新工具缓存，立即返回，不阻塞 UI。 */
export async function scheduleMcpToolsRefresh(resourceIds: string[], force = false): Promise<void> {
  if (!inTauri()) return;
  await invoke("schedule_mcp_tools_refresh", { resourceIds, force });
}

export async function transferResource(
  resourceId: string,
  targetOwner: string,
  copy: boolean,
): Promise<GlobalResource[]> {
  if (!inTauri()) throw new Error("浏览器预览不能迁移本机资源");
  return invoke<GlobalResource[]>("transfer_resource", { resourceId, targetOwner, copy });
}

export async function saveAgent(input: Omit<Agent, "id"> & { id?: string }): Promise<Agent[]> {
  if (!inTauri()) throw new Error("请在桌面应用中配置智能体");
  return invoke<Agent[]>("save_agent", { input });
}

export async function deleteAgent(agentId: string): Promise<Agent[]> {
  if (!inTauri()) return [];
  return invoke<Agent[]>("delete_agent", { agentId });
}

export async function saveSquad(input: Omit<Squad, "id"> & { id?: string }): Promise<Squad[]> {
  if (!inTauri()) throw new Error("请在桌面应用中配置小队");
  return invoke<Squad[]>("save_squad", { input });
}

export async function deleteSquad(squadId: string): Promise<Squad[]> {
  if (!inTauri()) return [];
  return invoke<Squad[]>("delete_squad", { squadId });
}

export async function createTask(input: TaskInput): Promise<Task> {
  if (!inTauri()) {
    const created: Task = {
      id: crypto.randomUUID(),
      sessionId: crypto.randomUUID(),
      title: input.title,
      description: input.description,
      agentId: input.agentId,
      status: input.status ?? "todo",
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      parentTaskId: input.parentTaskId ?? null,
    };
    previewData.sessions.unshift({
      id: created.sessionId,
      projectId: input.projectIds?.[0] ?? input.projectId ?? null,
      projectIds: input.projectIds ?? (input.projectId ? [input.projectId] : []),
      title: created.title,
      targetType: "agent",
      targetId: created.agentId,
      createdAt: created.createdAt,
      updatedAt: created.updatedAt,
    });
    previewData.tasks.unshift(created);
    return structuredClone(created);
  }
  return invoke<Task>("create_task", { input });
}

export async function updateTaskMeta(input: {
  taskId: string;
  priority?: Task["priority"];
  labels?: string[];
  checklist?: Task["checklist"];
  prUrl?: string;
  parentTaskId?: string | null;
}): Promise<Task> {
  if (!inTauri()) {
    const task = previewData.tasks.find((item) => item.id === input.taskId);
    if (!task) throw new Error("任务不存在");
    if (input.priority) task.priority = input.priority;
    if (input.labels) task.labels = input.labels;
    if (input.checklist) task.checklist = input.checklist;
    if (input.prUrl != null) task.prUrl = input.prUrl;
    if (input.parentTaskId !== undefined) task.parentTaskId = input.parentTaskId;
    task.updatedAt = new Date().toISOString();
    return structuredClone(task);
  }
  return invoke<Task>("update_task_meta", {
    taskId: input.taskId,
    input: {
      priority: input.priority,
      labels: input.labels,
      checklist: input.checklist,
      prUrl: input.prUrl,
      parentTaskId: input.parentTaskId === undefined ? undefined : input.parentTaskId,
    },
  });
}

export async function updateTask(input: {
  taskId: string;
  title?: string;
  description?: string;
  status?: TaskStatus;
  agentId?: string;
}): Promise<Task> {
  if (!inTauri()) {
    const task = previewData.tasks.find((item) => item.id === input.taskId);
    if (!task) throw new Error("任务不存在");
    if (input.title?.trim()) task.title = input.title.trim();
    if (input.description != null) task.description = input.description;
    if (input.status) task.status = input.status;
    if (input.agentId) task.agentId = input.agentId;
    task.updatedAt = new Date().toISOString();
    const session = previewData.sessions.find((item) => item.id === task.sessionId);
    if (session) {
      session.title = task.title;
      session.targetId = task.agentId;
      session.updatedAt = task.updatedAt;
    }
    return structuredClone(task);
  }
  return invoke<Task>("update_task", {
    taskId: input.taskId,
    title: input.title,
    description: input.description,
    status: input.status,
    agentId: input.agentId,
  });
}

export async function listAgentRuns(agentId: string): Promise<AgentActivityRecord[]> {
  if (!inTauri()) return [];
  return invoke<AgentActivityRecord[]>("list_agent_runs", { agentId });
}

export interface RuntimeModels {
  /** false：该运行时不接收 --model，填了也会被忽略 */
  supported: boolean;
  /** cli：CLI 实时查询；preset：官方别名；manual：只能手填 */
  source: "cli" | "preset" | "manual";
  models: Array<{ id: string; label: string }>;
  error?: string | null;
}

export async function listRuntimeModels(runtimeId: string, refresh = false): Promise<RuntimeModels> {
  if (!inTauri()) {
    // 浏览器预览没有 CLI，给一份示例数据便于看界面
    if (runtimeId === "cursor") {
      return {
        supported: true,
        source: "cli",
        models: [
          { id: "auto", label: "Auto" },
          { id: "composer-2.5", label: "Composer 2.5" },
          { id: "claude-opus-5-5-medium", label: "Claude Opus 5.5 1M" },
          { id: "gpt-5.6-sol-high", label: "GPT-5.6 Sol 1M High" },
        ],
      };
    }
    if (runtimeId === "claude") {
      return { supported: true, source: "preset", models: [{ id: "sonnet", label: "Sonnet（最新）" }, { id: "opus", label: "Opus（最新）" }] };
    }
    return { supported: runtimeId === "codex", source: "manual", models: [] };
  }
  return invoke<RuntimeModels>("list_runtime_models", { runtimeId, refresh });
}

export interface AgentSystemContext {
  /** 每次运行附加在最前面的平台说明（运行时简报），工作区等运行期才确定的内容是占位 */
  runtimeBrief: string;
  /** 所有智能体都会拿到的内置 skill，不能关闭 */
  builtinSkills: Array<{ name: string; description: string }>;
}

export async function agentSystemContext(agentId: string): Promise<AgentSystemContext> {
  if (!inTauri()) {
    return {
      runtimeBrief: `# Multara Agent Runtime\n你是 Multara 里的 coding agent。平台对象只能通过 \`multara\` CLI 操作…\n\n## Agent Identity\n**You are: ${agentId}**\n\n（浏览器预览：完整内容在桌面端生成）`,
      builtinSkills: [
        { name: "multara-onboarding", description: "欢迎会话的引导。" },
        { name: "multara-working-on-tasks", description: "创建、更新任务，并列出任务与运行。" },
        { name: "multara-creating-agents", description: "创建、更新智能体。" },
      ],
    };
  }
  return invoke<AgentSystemContext>("agent_system_context", { agentId });
}

export async function listTaskRuns(taskId: string): Promise<TaskRunRecord[]> {
  if (!inTauri()) return [];
  return invoke("list_task_runs", { taskId });
}

export async function updateTaskStatus(taskId: string, status: TaskStatus): Promise<Task[]> {
  if (!inTauri()) {
    const task = previewData.tasks.find((item) => item.id === taskId);
    if (task) {
      task.status = status;
      task.updatedAt = new Date().toISOString();
    }
    return structuredClone(previewData.tasks);
  }
  return invoke<Task[]>("update_task_status", { taskId, status });
}

export async function deleteTask(taskId: string, force = false): Promise<Task[]> {
  if (!inTauri()) {
    const task = previewData.tasks.find((item) => item.id === taskId);
    previewData.tasks = previewData.tasks.filter((item) => item.id !== taskId);
    if (task) previewData.sessions = previewData.sessions.filter((item) => item.id !== task.sessionId);
    return structuredClone(previewData.tasks);
  }
  return invoke<Task[]>("delete_task", { taskId, force });
}

export async function saveAutomation(input: AutomationInput): Promise<Automation[]> {
  if (!inTauri()) throw new Error("请在桌面应用中配置自动化");
  return invoke<Automation[]>("save_automation", { input });
}

export async function deleteAutomation(automationId: string): Promise<Automation[]> {
  if (!inTauri()) return [];
  return invoke<Automation[]>("delete_automation", { automationId });
}

export async function runAutomation(automationId: string): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能运行自动化");
  await invoke("run_automation", { automationId });
}

export async function createChatSession(input: {
  projectId: string | null;
  targetType: "agent" | "squad";
  targetId: string;
  title?: string;
}): Promise<BootstrapData["sessions"][number]> {
  if (!inTauri()) {
    const created = {
      id: crypto.randomUUID(),
      projectId: input.projectId,
      title: input.title || "新对话",
      targetType: input.targetType,
      targetId: input.targetId,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    previewData.sessions.unshift(created);
    return structuredClone(created);
  }
  return invoke<BootstrapData["sessions"][number]>("create_chat_session", { input });
}

export async function updateChatSessionTitle(sessionId: string, title: string): Promise<string> {
  if (!inTauri()) {
    const session = previewData.sessions.find((item) => item.id === sessionId);
    if (!session) throw new Error("聊天不存在");
    session.title = title.trim().slice(0, 24) || "新对话";
    return session.title;
  }
  return invoke<string>("update_chat_session_title", { sessionId, title });
}

export async function generateChatSessionTitle(prompt: string): Promise<string> {
  if (!inTauri()) {
    return titleFromPrompt(prompt);
  }
  return invoke<string>("generate_chat_session_title", { prompt });
}

export function titleFromPrompt(content: string) {
  const text = content
    .replace(/\[(代码引用|文件|文件夹)\][\s\S]*?(```[\s\S]*?```|$)/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  return [...text].slice(0, 18).join("") || "新对话";
}

export async function updateChatSessionTarget(
  sessionId: string,
  targetType: "agent" | "squad",
  targetId: string,
): Promise<void> {
  if (!inTauri()) return;
  await invoke("update_chat_session_target", { sessionId, targetType, targetId });
}

export async function listChatMessages(sessionId: string): Promise<ChatMessage[]> {
  if (!inTauri()) {
    if (sessionId !== "welcome") return [];
    return [
      {
        id: "welcome-opening",
        sessionId: "welcome",
        role: "assistant",
        content: [
          "你好，欢迎来到 Multara。这里是人和 AI 智能体通过任务一起把事情做完的工作区。",
          "",
          "我是 Miku，这里的总管。我负责把事情理清楚、找到合适的智能体接手，也是你随时可以开口的第一站。",
          "",
          "接下来是这样：你说一个目标，我把它变成一个任务，交给合适的智能体开始跑，你能看着它推进。",
          "",
          "从下面选一个开始，或者直接告诉我你现在想做成什么。",
        ].join("\n"),
        runId: null,
        agentId: "miku",
        audienceType: "user",
        failureReason: null,
        elapsedMs: null,
        messageKind: "message",
        quickActions: [
          { label: "理清当前目标", prompt: "把我们手头的目标整理成一组任务，指派给我自己。先给出预览，等我确认后再创建。", primary: true },
          { label: "帮我做一件小事", prompt: "从我现在说的事情里挑一件你能马上做的，建成任务并开始。先给我看任务标题和交付物。", primary: false },
          { label: "每天进度摘要", prompt: "建一个每天早上的工作摘要自动化。先预览默认时间，等我确认后再创建。", primary: false },
        ],
        timeline: [],
        attachments: [],
        createdAt: now,
      },
    ];
  }
  return invoke<ChatMessage[]>("list_chat_messages", { sessionId });
}

export async function loadAttachmentPreview(attachmentId: string): Promise<string | null> {
  if (!inTauri() || attachmentId.startsWith("pending:")) return null;
  try {
    return await invoke<string>("load_attachment_preview", { attachmentId });
  } catch {
    return null;
  }
}

export async function openMessageAttachment(attachmentId: string): Promise<void> {
  if (!inTauri() || attachmentId.startsWith("pending:")) return;
  await invoke("open_message_attachment", { attachmentId });
}

export async function openExternalUrl(url: string): Promise<void> {
  if (!/^https?:\/\//i.test(url)) return;
  if (!inTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await invoke("open_external_url", { url });
}

export async function upsertInboxItem(input: InboxInput): Promise<InboxItem> {
  if (!inTauri()) {
    const now = new Date().toISOString();
    const existing = previewData.inbox.find((item) => item.sessionId === input.sessionId);
    const next: InboxItem = {
      id: existing?.id ?? `inbox-${input.sessionId}`,
      sessionId: input.sessionId,
      projectId: input.projectId ?? null,
      taskId: input.taskId ?? null,
      title: input.title,
      preview: input.preview || "对话已结束",
      status: input.status,
      read: false,
      readMessageId: existing?.readMessageId ?? null,
      unreadCount: existing?.unreadCount ?? 1,
      createdAt: now,
    };
    previewData.inbox = [
      next,
      ...previewData.inbox.filter((item) => item.sessionId !== input.sessionId),
    ];
    return structuredClone(next);
  }
  return invoke<InboxItem>("upsert_inbox_item", { input });
}

export async function markInboxRead(id: string, messageId?: string | null): Promise<void> {
  if (!inTauri()) {
    const item = previewData.inbox.find((entry) => entry.id === id);
    if (item) {
      item.read = true;
      item.unreadCount = 0;
      if (messageId) item.readMessageId = messageId;
    }
    return;
  }
  await invoke("mark_inbox_read", { id, messageId: messageId || null });
}

export async function markInboxUnread(id: string): Promise<void> {
  if (!inTauri()) {
    const item = previewData.inbox.find((entry) => entry.id === id);
    if (item) {
      item.read = false;
      item.unreadCount = Math.max(1, item.unreadCount);
    }
    return;
  }
  await invoke("mark_inbox_unread", { id });
}

export async function markInboxAllRead(): Promise<void> {
  if (!inTauri()) {
    for (const item of previewData.inbox) {
      item.read = true;
      item.unreadCount = 0;
    }
    return;
  }
  await invoke("mark_inbox_all_read");
}

export async function archiveInboxItem(id: string): Promise<void> {
  if (!inTauri()) {
    previewData.inbox = previewData.inbox.filter((item) => item.id !== id);
    return;
  }
  await invoke("archive_inbox_item", { id });
}

export async function deleteInboxItem(id: string): Promise<void> {
  if (!inTauri()) {
    previewData.inbox = previewData.inbox.filter((item) => item.id !== id);
    return;
  }
  await invoke("delete_inbox_item", { id });
}

export async function forkChatSession(sessionId: string): Promise<ChatSession> {
  if (!inTauri()) throw new Error("请在桌面应用中复制会话");
  return invoke<ChatSession>("fork_chat_session", { sessionId });
}

export async function setMessageFeedback(messageId: string, feedback: "up" | "down" | null): Promise<void> {
  if (!inTauri()) return;
  await invoke("set_message_feedback", { messageId, feedback });
}

export async function compactChatSession(sessionId: string): Promise<string> {
  if (!inTauri()) return "本地预览不会压缩对话。";
  return invoke<string>("compact_chat_session", { sessionId });
}

export async function resolvePermission(requestId: string, optionId: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("resolve_permission", { requestId, optionId });
}

export async function listAutomationRuns(automationId: string): Promise<AutomationRun[]> {
  if (!inTauri()) return [];
  return invoke<AutomationRun[]>("list_automation_runs", { automationId });
}

export async function listWebhookDeliveries(automationId: string): Promise<WebhookDelivery[]> {
  if (!inTauri()) return [];
  return invoke<WebhookDelivery[]>("list_webhook_deliveries", { automationId });
}

export async function getRunStatistics(days = 30, projectId = "", agentId = ""): Promise<RunStatistics> {
  if (!inTauri()) return previewStatistics(days, projectId, agentId);
  return invoke<RunStatistics>("get_run_statistics", {
    days,
    projectId: projectId || null,
    agentId: agentId || null,
  });
}

/** 浏览器预览没有本机运行记录，用固定样本把用量页的图表和排行画出来。 */
function previewUsage(total: number, failed: number, seconds: number, share = 1) {
  const inputTokens = Math.round(total * 1800 * share);
  const outputTokens = Math.round(total * 420 * share);
  const cacheReadTokens = Math.round(total * 260 * share);
  return {
    total,
    failed,
    seconds,
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheWriteTokens: 0,
    costUsd: Number(((inputTokens * 3 + outputTokens * 15 + cacheReadTokens * 0.3) / 1_000_000).toFixed(4)),
  };
}

function previewStatistics(days: number, projectId: string, agentId = ""): RunStatistics {
  const scale = projectId || agentId ? 0.4 : 1;
  const daily = [];
  const today = new Date();
  for (let offset = days - 1; offset >= 0; offset -= 1) {
    const date = new Date(today.getFullYear(), today.getMonth(), today.getDate() - offset);
    const key = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
    const quiet = offset % 4 === 0;
    const total = quiet ? 0 : Math.max(1, Math.round((2 + (offset % 5)) * scale));
    const failed = !quiet && offset % 6 === 0 ? 1 : 0;
    daily.push({ date: key, ...previewUsage(total, failed, total * 210) });
  }
  const total = daily.reduce((sum, item) => sum + item.total, 0);
  const failed = daily.reduce((sum, item) => sum + item.failed, 0);
  const stopped = projectId || agentId ? 0 : 1;
  const completed = Math.max(0, total - failed - stopped);
  const totalSeconds = daily.reduce((sum, item) => sum + item.seconds, 0);
  const inputTokens = daily.reduce((sum, item) => sum + item.inputTokens, 0);
  const outputTokens = daily.reduce((sum, item) => sum + item.outputTokens, 0);
  const cacheReadTokens = daily.reduce((sum, item) => sum + item.cacheReadTokens, 0);
  const costUsd = daily.reduce((sum, item) => sum + item.costUsd, 0);
  const mikuTotal = Math.max(1, Math.round(total * 0.7));
  const mikuFailed = Math.min(failed, Math.max(0, failed - 1));
  const mikuUsage = previewUsage(mikuTotal, mikuFailed, totalSeconds * 0.7, 1);
  const reviewTotal = Math.max(0, total - mikuTotal);
  const reviewUsage = previewUsage(reviewTotal, failed - mikuFailed, totalSeconds * 0.3, 1);
  const agents = [
    { id: "miku", name: "Miku", completed: mikuTotal - mikuFailed, ...mikuUsage },
    { id: "reviewer", name: "审查员", completed: Math.max(0, completed - (mikuTotal - mikuFailed)), ...reviewUsage },
  ].filter((item) => item.total > 0);
  return {
    total,
    completed,
    failed,
    stopped,
    successRate: completed + failed === 0 ? 0 : (completed / (completed + failed)) * 100,
    averageDurationSeconds: total === 0 ? 0 : totalSeconds / total,
    totalSeconds,
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheWriteTokens: 0,
    costUsd,
    daily,
    byRuntime: [
      { name: "cursor", total, completed, failed, seconds: totalSeconds, inputTokens, outputTokens, cacheReadTokens, cacheWriteTokens: 0, costUsd },
    ],
    byAgent: agentId ? agents.filter((item) => item.id === agentId) : agents,
  };
}

export async function saveSettings(settings: Record<string, string>): Promise<Record<string, string>> {
  if (!inTauri()) {
    previewData.settings = { ...previewData.settings, ...settings };
    return structuredClone(previewData.settings);
  }
  return invoke<Record<string, string>>("save_settings", { settings });
}

export async function appVersion(): Promise<string> {
  return inTauri() ? getVersion() : "0.1.0";
}

export async function checkGitExecutable(path: string): Promise<string> {
  if (!inTauri()) return "git version preview";
  return invoke<string>("check_git_executable", { path });
}

export async function generateBranchName(requirementName: string): Promise<string> {
  if (!inTauri()) {
    await new Promise((resolve) => setTimeout(resolve, 480));
    const prefix = previewData.settings.branchPrefix || "feat";
    return `${prefix}/short-branch`;
  }
  return invoke<string>("generate_branch_name", { requirementName });
}

export async function createProject(name: string, note: string): Promise<Project> {
  if (!inTauri()) {
    const now = new Date().toISOString();
    const project: Project = {
      id: crypto.randomUUID(),
      name,
      note,
      workspaceCount: 0,
      createdAt: now,
      openedAt: now,
    };
    previewData.projects.unshift(project);
    return structuredClone(project);
  }
  return invoke<Project>("create_project", { name, note });
}

export async function updateProject(id: string, name: string, note: string): Promise<Project> {
  if (!inTauri()) {
    const project = previewData.projects.find((item) => item.id === id);
    if (!project) throw new Error("需求不存在");
    project.name = name;
    project.note = note;
    return structuredClone(project);
  }
  return invoke<Project>("update_project", { id, name, note });
}

export async function detachWorkspace(workspaceId: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("detach_workspace", { workspaceId });
}

export async function setProjectArchived(projectId: string, archived: boolean): Promise<string | null> {
  if (!inTauri()) {
    const archivedAt = archived ? new Date().toISOString() : null;
    const project = previewData.projects.find((item) => item.id === projectId);
    if (project) project.archivedAt = archivedAt;
    return archivedAt;
  }
  return invoke<string | null>("set_project_archived", { projectId, archived });
}

export async function deleteProject(projectId: string): Promise<void> {
  if (!inTauri()) {
    previewData.projects = previewData.projects.filter((project) => project.id !== projectId);
    previewData.workspaces = previewData.workspaces.filter((workspace) => workspace.projectId !== projectId);
    return;
  }
  await invoke("delete_project", { projectId });
}

export async function selectFolder(): Promise<string | null> {
  const selected = await selectFolders();
  return selected[0] ?? null;
}

export async function selectPaths(directory: boolean): Promise<string[]> {
  if (!inTauri()) return [];
  const selected = await open({ directory, multiple: true });
  if (!selected) return [];
  return Array.isArray(selected) ? selected : [selected];
}

export type PickedPath = { path: string; isDirectory: boolean };

/** 系统选择器：一次可多选文件和/或文件夹（macOS 支持混选）。 */
export async function selectFilesOrFolders(): Promise<PickedPath[]> {
  if (!inTauri()) return [];
  return invoke<PickedPath[]>("select_files_or_folders");
}

export async function selectFolders(): Promise<string[]> {
  if (!inTauri()) return [];
  const selected = await open({ directory: true, multiple: true });
  if (Array.isArray(selected)) return selected;
  return typeof selected === "string" ? [selected] : [];
}

export async function addWorkspace(
  projectId: string,
  sourcePath: string,
  mode: "isolated" | "direct",
  baseRef?: string | null,
  checkoutName?: string | null,
  createBranch?: boolean,
): Promise<Workspace> {
  if (!inTauri()) throw new Error("请在 Tauri 应用中选择并添加本机目录");
  return invoke<Workspace>("add_workspace", {
    projectId,
    sourcePath,
    mode,
    baseRef,
    checkoutName,
    createBranch,
  });
}

export async function scanWorkspaceCandidates(sourcePath: string): Promise<WorkspaceCandidate[]> {
  if (!inTauri()) return [];
  return invoke<WorkspaceCandidate[]>("scan_workspace_candidates", { sourcePath });
}

export async function searchWorkspace(
  workspaceId: string,
  options: {
    query: string;
    caseSensitive: boolean;
    wholeWord: boolean;
    regex: boolean;
    include: string;
    exclude: string;
    useExcludeSettings?: boolean;
  },
): Promise<{
  matches: Array<{
    path: string;
    line: number;
    column: number;
    length: number;
    preview: string;
    previewColumn: number;
  }>;
  truncated: boolean;
  error: string;
}> {
  if (!inTauri()) return { matches: [], truncated: false, error: "" };
  return invoke("search_workspace", {
    workspaceId,
    query: options.query,
    caseSensitive: options.caseSensitive,
    wholeWord: options.wholeWord,
    useRegex: options.regex,
    include: options.include,
    exclude: options.exclude,
    useExcludeSettings: options.useExcludeSettings !== false,
  });
}

export interface TerminalProfile {
  id: string;
  name: string;
  path: string;
  args: string[];
  /** pwsh / bash / git-bash / zsh / fish / cmd / other */
  kind: string;
  isDefault: boolean;
}

export interface OpenedTerminal {
  id: string;
  pid: number;
  profileId: string;
  shell: string;
  kind: string;
  /** 是否注入了 shell 集成脚本（OSC 633）。 */
  integration: boolean;
}

export async function terminalOpen(cwd: string, profile?: string): Promise<OpenedTerminal> {
  if (!inTauri()) throw new Error("浏览器预览不能启动终端");
  return invoke<OpenedTerminal>("terminal_open", { cwd, profile: profile || null });
}

export async function terminalProfiles(): Promise<TerminalProfile[]> {
  if (!inTauri()) return [];
  return invoke<TerminalProfile[]>("terminal_profiles");
}

export interface ResolvedTerminalPath {
  path: string;
  isDirectory: boolean;
}

export async function terminalResolvePaths(cwd: string, candidates: string[]): Promise<Array<ResolvedTerminalPath | null>> {
  if (!inTauri() || !candidates.length) return candidates.map(() => null);
  return invoke<Array<ResolvedTerminalPath | null>>("terminal_resolve_paths", { cwd, candidates });
}

export async function terminalClipboardText(): Promise<string> {
  if (!inTauri()) return navigator.clipboard.readText();
  return invoke<string>("terminal_clipboard_text");
}

export async function terminalFreePort(port: number): Promise<number> {
  if (!inTauri()) return 0;
  return invoke<number>("terminal_free_port", { port });
}

export async function terminalCommandNames(): Promise<string[]> {
  if (!inTauri()) return [];
  return invoke<string[]>("terminal_command_names");
}

export async function terminalWrite(id: string, data: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("terminal_write", { id, data });
}

export async function terminalResize(id: string, cols: number, rows: number): Promise<void> {
  if (!inTauri()) return;
  await invoke("terminal_resize", { id, cols, rows });
}

export async function terminalClose(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("terminal_close", { id });
}

export async function terminalPublish(input: {
  id: string;
  title: string;
  cwd: string;
  lineOffset: number;
  text: string;
  active: boolean;
}): Promise<void> {
  if (!inTauri()) return;
  await invoke("terminal_publish", input);
}

export interface TerminalService {
  id: string;
  ports: number[];
}

export async function terminalServices(): Promise<TerminalService[]> {
  if (!inTauri()) return [];
  return invoke<TerminalService[]>("terminal_services");
}

export async function listQuickOpenFiles(workspaceId: string): Promise<FileEntry[]> {
  if (!inTauri()) return [];
  return invoke<FileEntry[]>("list_quick_open_files", { workspaceId });
}

export interface SourceFileRecord {
  path: string;
  mtime: number;
  content?: string | null;
}

export async function collectSourceFiles(
  workspaceId: string,
  extensions: string[],
  known: Record<string, number>,
): Promise<{ files: SourceFileRecord[]; truncated: boolean }> {
  if (!inTauri()) return { files: [], truncated: false };
  return invoke("collect_source_files", { workspaceId, extensions, known });
}

export interface TypingFileRecord {
  path: string;
  diskPath?: string | null;
  content: string;
}

export async function collectWorkspaceTypings(workspaceId: string): Promise<TypingFileRecord[]> {
  if (!inTauri()) return [];
  return invoke<TypingFileRecord[]>("collect_workspace_typings", { workspaceId });
}

export interface LspServerInfo {
  id: string;
  command: string;
  rootPath: string;
  initializationOptions?: unknown;
}

export async function lspStart(workspaceId: string, language: string): Promise<LspServerInfo | null> {
  if (!inTauri()) return null;
  return invoke<LspServerInfo | null>("lsp_start", { workspaceId, language });
}

export async function lspSend(id: string, message: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("lsp_send", { id, message });
}

export async function lspStop(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("lsp_stop", { id });
}

export function isTauriRuntime(): boolean {
  return inTauri();
}

export async function listWorkspaceTree(workspaceId: string): Promise<FileEntry[]> {
  if (!inTauri()) return [];
  return invoke<FileEntry[]>("list_workspace_tree", { workspaceId });
}

export async function listWorkspaceDirectory(
  workspaceId: string,
  path?: string,
): Promise<FileEntry[]> {
  if (!inTauri()) return [];
  return invoke<FileEntry[]>("list_workspace_directory", { workspaceId, path: path || null });
}

export async function readWorkspaceFile(workspaceId: string, path: string): Promise<FileContent> {
  if (!inTauri()) throw new Error("浏览器预览不能读取本机文件");
  return invoke<FileContent>("read_workspace_file", { workspaceId, path });
}

export async function readExternalFile(path: string): Promise<FileContent> {
  if (!inTauri()) throw new Error("浏览器预览不能读取本机文件");
  return invoke<FileContent>("read_external_file", { path });
}

export async function listExternalDirectory(path: string): Promise<FileEntry[]> {
  if (!inTauri()) return [];
  return invoke<FileEntry[]>("list_external_directory", { path });
}

export async function writeWorkspaceFile(
  workspaceId: string,
  path: string,
  content: string,
): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能写入本机文件");
  await invoke("write_workspace_file", { workspaceId, path, content });
}

export async function createWorkspaceFile(
  workspaceId: string,
  path: string,
  content = "",
): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能创建本机文件");
  await invoke("create_workspace_file", { workspaceId, path, content });
}

export async function createWorkspaceDirectory(workspaceId: string, path: string): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能创建本机文件夹");
  await invoke("create_workspace_directory", { workspaceId, path });
}

export async function renameWorkspaceEntry(
  workspaceId: string,
  from: string,
  to: string,
): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能重命名本机文件");
  await invoke("rename_workspace_entry", { workspaceId, from, to });
}

export async function deleteWorkspaceEntry(workspaceId: string, path: string): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能删除本机文件");
  await invoke("delete_workspace_entry", { workspaceId, path });
}

export async function revealWorkspacePath(workspaceId: string, path?: string): Promise<void> {
  if (!inTauri()) throw new Error("浏览器预览不能打开本机资源管理器");
  await invoke("reveal_workspace_path", { workspaceId, path: path || null });
}

export async function listProjectGitStatus(projectId: string): Promise<GitRepositoryStatus[]> {
  if (!inTauri()) return [];
  return invoke<GitRepositoryStatus[]>("list_project_git_status", { projectId });
}

export async function gitIndexFile(workspaceId: string, path: string): Promise<GitIndexFile> {
  if (!inTauri()) return { inRepo: false, tracked: false, binary: false, content: "" };
  return invoke<GitIndexFile>("git_index_file", { workspaceId, path });
}

export async function gitHeadFile(workspaceId: string, path: string): Promise<GitIndexFile> {
  if (!inTauri()) return { inRepo: false, tracked: false, binary: false, content: "" };
  return invoke<GitIndexFile>("git_head_file", { workspaceId, path });
}

export async function gitBlameLine(
  workspaceId: string,
  path: string,
  line: number,
  revision?: string,
): Promise<GitLineBlame | null> {
  if (!inTauri()) return null;
  return invoke<GitLineBlame | null>("git_blame_line", {
    workspaceId,
    path,
    line,
    revision: revision ?? null,
  });
}

/** 整文件 blame：`null` 表示不在仓库里；空数组表示整个文件都没有历史（尚未提交）。 */
export async function gitBlameFile(workspaceId: string, path: string): Promise<GitLineBlame[] | null> {
  if (!inTauri()) return null;
  return invoke<GitLineBlame[] | null>("git_blame_file", { workspaceId, path });
}

export async function gitRevisionFile(
  workspaceId: string,
  revision: string,
  path: string,
): Promise<GitIndexFile> {
  if (!inTauri()) return { inRepo: false, tracked: false, binary: false, content: "" };
  return invoke<GitIndexFile>("git_revision_file", { workspaceId, revision, path });
}

export async function gitLog(
  workspaceId: string,
  skip: number,
  limit: number,
  path?: string,
): Promise<GitHistoryPage> {
  if (!inTauri()) return { items: [], hasMore: false };
  return invoke<GitHistoryPage>("git_log_workspace", { workspaceId, skip, limit, path: path ?? null });
}

export async function gitCommitFiles(workspaceId: string, hash: string): Promise<GitCommitFile[]> {
  if (!inTauri()) return [];
  return invoke<GitCommitFile[]>("git_commit_files", { workspaceId, hash });
}

export async function gitStageFiles(
  workspaceId: string,
  paths: string[],
  staged: boolean,
): Promise<void> {
  if (!inTauri()) return;
  await invoke("git_stage_files", { workspaceId, paths, staged });
}

export async function gitDiscardFiles(workspaceId: string, paths: string[]): Promise<void> {
  if (!inTauri()) return;
  await invoke("git_discard_files", { workspaceId, paths });
}

export async function gitStageRepository(workspaceId: string, staged: boolean): Promise<void> {
  if (!inTauri()) return;
  await invoke("git_stage_repository", { workspaceId, staged });
}

export async function generateGitCommitMessage(workspaceId: string): Promise<string> {
  if (!inTauri()) return "feat: update workspace changes";
  return invoke<string>("generate_git_commit_message", { workspaceId });
}

export async function gitCommitWorkspace(
  workspaceId: string,
  message: string,
  skipVerify = false,
): Promise<GitRepositoryStatus> {
  return invoke<GitRepositoryStatus>("git_commit_workspace", { workspaceId, message, skipVerify });
}

export async function gitCommitMultiple(inputs: GitCommitInput[]): Promise<GitRepositoryStatus[]> {
  return invoke<GitRepositoryStatus[]>("git_commit_multiple", { inputs });
}

export async function gitSwitchWorkspaceBranch(
  workspaceId: string,
  branch: string,
  create: boolean,
  from?: string | null,
  detach = false,
): Promise<GitRepositoryStatus> {
  return invoke<GitRepositoryStatus>("git_switch_workspace_branch", {
    workspaceId,
    branch,
    create,
    from: from || null,
    detach,
  });
}

export async function gitGraphCommitAction(
  workspaceId: string,
  action: string,
  hash: string,
  name = "",
): Promise<void> {
  await invoke("git_graph_commit_action", { workspaceId, action, hash, name });
}

export interface BranchRenameReport {
  previousBranch: string;
  localRenamed: boolean;
  localBranch: string;
  remoteAttempted: boolean;
  remoteOk: boolean;
  remoteError: string;
  hadUpstream: boolean;
  upstream: string;
}

export async function gitRenameWorkspaceBranch(
  workspaceId: string,
  branch: string,
  renameRemote = false,
): Promise<BranchRenameReport> {
  return invoke<BranchRenameReport>("git_rename_workspace_branch", { workspaceId, branch, renameRemote });
}

export async function gitSetBranchUpstream(workspaceId: string, upstream: string): Promise<string> {
  return invoke<string>("git_set_branch_upstream", { workspaceId, upstream });
}

export async function gitMergeWorkspaceBranch(
  workspaceId: string,
  branch: string,
): Promise<GitRepositoryStatus> {
  return invoke<GitRepositoryStatus>("git_merge_workspace_branch", { workspaceId, branch });
}

export async function gitAbortWorkspaceMerge(workspaceId: string): Promise<GitRepositoryStatus> {
  return invoke<GitRepositoryStatus>("git_abort_workspace_merge", { workspaceId });
}

function previewMergeTarget(target: string): NonNullable<MergePreview["target"]> {
  const branch = target.replace(/^origin\//, "");
  const display = target.startsWith("origin/") ? target : `origin/${branch}`;
  const base = {
    branch,
    remote: "origin",
    display,
    exists: true,
    hasRemoteBranch: true,
    commits: 5,
    files: 12,
    targetAhead: 0,
    conflictFiles: [] as string[],
    checkedOutAt: branch === "main" ? "~/dev/app" : null,
    plan: "fast_forward",
  };
  if (branch.includes("release")) {
    return {
      ...base,
      plan: "conflict",
      commits: 2,
      files: 2,
      targetAhead: 1,
      conflictFiles: ["src/auth.ts", "src/login.vue"],
    };
  }
  if (branch.includes("develop")) {
    return { ...base, plan: "merge", commits: 4, files: 6, targetAhead: 2 };
  }
  return base;
}

export async function gitMergePreview(workspaceId: string, target: string | null): Promise<MergePreview> {
  if (!inTauri()) {
    // 浏览器预览：给出一份固定数据，便于调样式
    return {
      sourceLabel: "feature/login",
      sourceCommit: "a3f9c21",
      dirty: true,
      hasRemote: true,
      origin: { baseRef: "main", baseCommit: "7be41d0", commits: 5, files: 12 },
      target: target ? previewMergeTarget(target) : null,
    };
  }
  return invoke<MergePreview>("git_merge_preview", { workspaceId, target });
}

export async function gitMergeIntoBranch(input: {
  workspaceId: string;
  target: string;
  push: boolean;
  chatTargetType: "agent" | "squad";
  chatTargetId: string;
}): Promise<MergeIntoResult> {
  if (!inTauri()) throw new Error("浏览器预览不能执行合并");
  return invoke<MergeIntoResult>("git_merge_into_branch", { input });
}

export async function gitPullMergeSession(mergeId: string): Promise<MergePullResult> {
  if (!inTauri()) throw new Error("浏览器预览不能拉取远程");
  return invoke<MergePullResult>("git_pull_merge_session", { mergeId });
}

export async function gitFinishMergeSession(mergeId: string): Promise<MergeFinishResult> {
  return invoke<MergeFinishResult>("git_finish_merge_session", { mergeId });
}

export async function gitAbortMergeSession(mergeId: string): Promise<void> {
  await invoke("git_abort_merge_session", { mergeId });
}

export async function gitFetchWorkspace(workspaceId: string): Promise<GitRepositoryStatus> {
  if (!inTauri()) throw new Error("浏览器预览不能刷新远程");
  return invoke<GitRepositoryStatus>("git_fetch_workspace", { workspaceId });
}

export async function gitPullWorkspace(workspaceId: string, skipVerify = false): Promise<GitRepositoryStatus> {
  if (!inTauri()) throw new Error("浏览器预览不能拉取远程");
  return invoke<GitRepositoryStatus>("git_pull_workspace", { workspaceId, skipVerify });
}

export async function gitPushWorkspace(workspaceId: string, skipVerify = false): Promise<GitRepositoryStatus> {
  if (!inTauri()) throw new Error("浏览器预览不能推送远程");
  return invoke<GitRepositoryStatus>("git_push_workspace", { workspaceId, skipVerify });
}

export async function gitSyncWorkspace(workspaceId: string, skipVerify = false): Promise<GitRepositoryStatus> {
  if (!inTauri()) throw new Error("浏览器预览不能同步远程");
  return invoke<GitRepositoryStatus>("git_sync_workspace", { workspaceId, skipVerify });
}

export async function gitRepairWorktreeRuntime(workspaceId: string): Promise<string> {
  if (!inTauri()) throw new Error("浏览器预览不能补钩子运行时");
  return invoke<string>("git_repair_worktree_runtime", { workspaceId });
}

export async function savePastedAttachment(
  bytes: number[],
  mimeType: string,
  name?: string,
): Promise<MessageAttachmentInput> {
  if (!inTauri()) {
    throw new Error("当前环境不支持粘贴图片");
  }
  return invoke<MessageAttachmentInput>("save_pasted_attachment", { bytes, mimeType, name });
}

export async function selectMessageAttachments(): Promise<MessageAttachmentInput[]> {
  if (!inTauri()) return [];
  const selected = await open({
    directory: false,
    multiple: true,
    filters: [{ name: "支持的附件", extensions: ["png", "jpg", "jpeg", "gif", "webp", "pdf", "txt", "md", "json", "csv", "log", "zip"] }],
  });
  const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
  return paths.map((path) => {
    const name = path.split(/[\\/]/).pop() || "附件";
    const extension = name.split(".").pop()?.toLowerCase() ?? "";
    const mimeType = ({ png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp", pdf: "application/pdf" } as Record<string, string>)[extension] ?? "application/octet-stream";
    return { name, path, mimeType, size: 0 };
  });
}

export async function sendMessage(
  sessionId: string,
  content: string,
  attachments: MessageAttachmentInput[] = [],
): Promise<ChatMessage[]> {
  if (!inTauri()) {
    const createdAt = new Date().toISOString();
    const details = {
      runId: null,
      failureReason: null,
      elapsedMs: null,
      messageKind: "message" as const,
      quickActions: [],
      timeline: [],
      attachments: [],
    };
    return [
      { id: crypto.randomUUID(), sessionId, role: "user", content, createdAt, ...details },
      {
        id: crypto.randomUUID(),
        sessionId,
        role: "assistant",
        content: "消息已进入本地调度队列。接入运行时后，将由所选智能体或小队队长执行。",
        createdAt,
        ...details,
      },
    ];
  }
  return invoke<ChatMessage[]>("send_message", { sessionId, content, attachments });
}

export async function stopRun(runId: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("stop_run", { runId });
}

export async function loadSessionDiffData(sessionId: string): Promise<SessionDiffData> {
  if (!inTauri()) return { snapshots: [], chunkStates: [], messageIds: [] };
  return invoke<SessionDiffData>("load_session_diff_data", { sessionId });
}

export async function loadActiveDiffData(): Promise<ActiveDiffData> {
  if (!inTauri()) return { snapshots: [], chunkStates: [], baselines: [] };
  return invoke<ActiveDiffData>("load_active_diff_data");
}

export async function saveFileBaseline(
  workspaceId: string,
  filePath: string,
  content: string,
): Promise<void> {
  if (!inTauri()) return;
  await invoke("save_file_baseline", { workspaceId, filePath, content });
}

export async function deleteFileBaseline(workspaceId: string, filePath: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("delete_file_baseline", { workspaceId, filePath });
}

export async function getSnapshotContent(
  messageId: string,
  workspaceId: string,
  filePath: string,
): Promise<string> {
  if (!inTauri()) return "";
  return invoke<string>("get_snapshot_content", { messageId, workspaceId, filePath });
}

export async function getTurnSnapshots(
  messageId: string,
  workspaceId: string,
): Promise<SnapshotEntry[]> {
  if (!inTauri()) return [];
  return invoke<SnapshotEntry[]>("get_turn_snapshots", { messageId, workspaceId });
}

export async function updateChunkState(input: {
  messageId: string;
  workspaceId: string;
  filePath: string;
  chunkKey: string;
  status: "accepted" | "undone" | "pending";
}): Promise<void> {
  if (!inTauri()) return;
  await invoke("update_chunk_state", input);
}

export async function deleteSnapshotsFrom(
  messageId: string,
  workspaceId: string,
  sessionId: string,
): Promise<void> {
  if (!inTauri()) return;
  await invoke("delete_snapshots_from", { messageId, workspaceId, sessionId });
}

export async function truncateSessionFromMessage(
  sessionId: string,
  messageId: string,
  alsoClearSnapshots = true,
): Promise<void> {
  if (!inTauri()) return;
  await invoke("truncate_session_from_message", {
    sessionId,
    messageId,
    alsoClearSnapshots,
  });
}

export async function listChannelInstallations(): Promise<ChannelInstallation[]> {
  if (!inTauri()) return [];
  return invoke<ChannelInstallation[]>("list_channel_installations");
}

export async function saveChannelInstallation(
  input: ChannelInstallationInput,
): Promise<ChannelInstallation> {
  if (!inTauri()) {
    return { ...input, id: input.id ?? "preview", botName: null, botOpenId: null, pairingCode: "000000", status: "disconnected", lastError: null, hasSecret: true } as ChannelInstallation;
  }
  return invoke<ChannelInstallation>("save_channel_installation", { input });
}

export async function deleteChannelInstallation(id: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("delete_channel_installation", { id });
}

export async function setChannelEnabled(id: string, enabled: boolean): Promise<void> {
  if (!inTauri()) return;
  await invoke("set_channel_enabled", { id, enabled });
}

export async function testChannelInstallation(
  id: string,
): Promise<Record<string, string>> {
  if (!inTauri()) return { name: "预览机器人", openId: "ou_preview" };
  return invoke<Record<string, string>>("test_channel_installation", { id });
}

export async function listChannelAllowedUsers(id: string): Promise<ChannelUser[]> {
  if (!inTauri()) return [];
  return invoke<ChannelUser[]>("list_channel_allowed_users", { id });
}

export async function removeChannelAllowedUser(id: string, userId: string): Promise<void> {
  if (!inTauri()) return;
  await invoke("remove_channel_allowed_user", { id, userId });
}

export async function rotateChannelPairingCode(id: string): Promise<string> {
  if (!inTauri()) return "000000";
  return invoke<string>("rotate_channel_pairing_code", { id });
}
