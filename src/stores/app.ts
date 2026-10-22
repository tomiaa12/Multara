import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import type { Agent, AutomationInput, BootstrapData, ChatMessage, CustomRuntimeInput, EditorSelectionContext, McpInput, MergeSession, MessageAttachmentInput, RunEvent, Squad, TaskInput, TaskStatus } from "../domain/types";
import * as backend from "../services/backend";
import {
  clearLegacyDismissed,
  loadLegacyDismissed,
  loadOpenTabsMap,
  persistOpenTabsMap,
  tabScope,
} from "./chat-open-tabs";
import { clearLastProjectsPath } from "../views/dashboard/last-projects-path";
import { boardStatusFromRun } from "../views/task-board";
import { setFileIconTheme } from "../file-icons";
import { useDiffStore } from "./diff";
import { executeReask } from "./reask";
import { applyAssistantChunk } from "../domain/assistant-text";
import { extractQuestions } from "../components/chat/question-block";
import { currentAppPath, inboxPreview, shouldNotifyInbox } from "../domain/inbox";

// 跳过的提问按消息 id 记在本地，刷新后卡片不会再弹出来；只留最近 200 条
const DISMISSED_QUESTIONS_KEY = "multara.dismissedQuestions";
function readDismissedQuestions(): string[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(DISMISSED_QUESTIONS_KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

export type WorkspaceAddRequest = {
  name: string;
  path: string;
  mode: "isolated" | "direct";
  baseRef?: string | null;
  checkoutName?: string | null;
  createBranch?: boolean;
};

export type WorkspacePrepJob = {
  id: string;
  projectId: string;
  name: string;
  path: string;
  status: "preparing" | "failed";
  error?: string;
};

export const useAppStore = defineStore("app", () => {
  const data = ref<BootstrapData | null>(null);
  const messages = ref<Record<string, ChatMessage[]>>({});
  const loading = ref(false);
  const error = ref("");
  const chatOpen = ref(false);
  const activeSessionId = ref("welcome");
  /** priority：回答提问 / 「立即发送」的消息，排到最前且不受提问暂停影响 */
  const queuedMessages = ref<{ sessionId: string; content: string; attachments: MessageAttachmentInput[]; priority?: boolean }[]>([]);
  const dismissedQuestions = ref(new Set<string>(readDismissedQuestions()));
  const sendingMessage = ref(false);
  const runStates = ref<Record<string, RunEvent>>({});
  const composerContexts = ref<EditorSelectionContext[]>([]);
  const composerFocusNonce = ref(0);
  const composerNotice = ref("");
  const composerNoticeQuery = ref("");
  const quickOpenRequest = ref<{ query: string; nonce: number } | null>(null);
  const sendFailure = ref<{
    nonce: number;
    sessionId: string;
    content: string;
    attachments: MessageAttachmentInput[];
    message: string;
  } | null>(null);
  const fileReveal = ref<{
    path: string;
    workspaceId?: string;
    startLine?: number;
    endLine?: number;
    nonce: number;
  } | null>(null);
  const workspaceJobs = ref<WorkspacePrepJob[]>([]);
  const openSessionTabs = ref<Record<string, string[]>>(loadOpenTabsMap());
  let eventsReady = false;

  const activeSession = computed(() =>
    data.value?.sessions.find((session) => session.id === activeSessionId.value),
  );
  const activeRunIds = computed(() =>
    Object.values(runStates.value)
      .filter((event) =>
        event.sessionId === activeSessionId.value &&
        (event.status === "queued" || event.status === "running") &&
        !event.runId.startsWith("pending:")
      )
      .map((event) => event.runId),
  );
  const processingMessage = computed(() => sendingMessage.value || activeRunIds.value.length > 0);

  async function initialize() {
    loading.value = true;
    error.value = "";
    try {
      data.value = await backend.bootstrap();
      applyStoredTheme(data.value.settings.theme ?? "dark");
      const { applyLanguageSetting } = await import("../i18n");
      applyLanguageSetting(data.value.settings.language);
      setFileIconTheme(data.value.settings.fileIconTheme);
      await registerRunEvents();
      if (!data.value.sessions.some((session) => session.id === activeSessionId.value)) {
        activeSessionId.value = data.value.sessions[0]?.id ?? "";
      }
      if (activeSessionId.value) {
        await loadMessages(activeSessionId.value);
      }
      await useDiffStore().loadActive();
      // MCP 工具列表：先灌缓存，再后台串行刷新，不挡启动
      void import("./mcp-tools").then(async ({ useMcpToolsStore }) => {
        const mcpTools = useMcpToolsStore();
        await mcpTools.hydrate();
        mcpTools.refreshInBackground(data.value?.resources.filter((item) => item.kind === "mcp") ?? []);
      });
      // 自动更新：静默检查/下载，下次启动安装（与 multica 同思路）
      void import("./updater").then(async ({ useUpdaterStore }) => {
        const { setUpdateProxy } = await import("../services/update");
        setUpdateProxy(data.value?.settings.updateProxy ?? "");
        const autoUpdate = data.value?.settings.autoUpdate !== "false";
        void useUpdaterStore().boot(autoUpdate);
      });
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading.value = false;
    }
  }

  function applyStoredTheme(theme: string) {
    const selected = theme === "system"
      ? (window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark")
      : theme;
    document.documentElement.dataset.theme = selected;
  }

  async function registerRunEvents() {
    if (eventsReady || !("__TAURI_INTERNALS__" in window)) return;
    eventsReady = true;
    await listen<RunEvent>("run-event", async ({ payload }) => {
      if (payload.eventKind === "file_snapshot" && payload.messageId && payload.filePath != null) {
        if (!payload.workspaceId) {
          console.error("[diff] file_snapshot 缺少 workspaceId，已丢弃", payload);
          return;
        }
        useDiffStore().onSnapshotReceived(
          payload.messageId,
          payload.filePath,
          payload.snapshotContent ?? "",
          payload.sessionId,
          payload.workspaceId,
        );
        return;
      }
      if (payload.eventKind === "file_deleted" && payload.filePath) {
        useDiffStore().onFileDeleted(payload.filePath, payload.workspaceId ?? "");
        return;
      }
      const previous = runStates.value[payload.runId];
      const terminal = ["completed", "failed", "stopped"];
      if (
        previous &&
        terminal.includes(previous.status) &&
        (payload.status === "running" ||
          (previous.status === "stopped" && payload.status === "completed"))
      ) {
        return;
      }
      runStates.value[payload.runId] = payload;
      if (data.value) {
        if (payload.status === "running" && previous?.status !== "running") {
          data.value.stats.running += 1;
        }
        if (["completed", "failed", "stopped"].includes(payload.status) && previous?.status === "running") {
          data.value.stats.running = Math.max(0, data.value.stats.running - 1);
          if (payload.status === "completed") data.value.stats.completedToday += 1;
          if (payload.status === "failed") data.value.stats.failedToday += 1;
        }
        syncTaskStatusFromRunEvent(payload);
      }
      const sessionMessages = messages.value[payload.sessionId] ?? [];
      const id = `run:${payload.runId}`;
      let existing = sessionMessages.find((message) => message.id === id);
      const hasOutput = Boolean(payload.timelineItem || payload.chunk);
      const isFailure = payload.status === "failed" || payload.status === "stopped";
      if (!existing && (hasOutput || isFailure)) {
        const session = data.value?.sessions.find((item) => item.id === payload.sessionId);
        const squad = session?.targetType === "squad"
          ? data.value?.squads.find((item) => item.id === session.targetId)
          : undefined;
        const internal = Boolean(squad && payload.agentId !== squad.leaderAgentId);
        existing = {
          id,
          sessionId: payload.sessionId,
          role: "assistant",
          content: "",
          runId: payload.runId,
          agentId: payload.agentId,
          audienceType: internal ? "agent" : "user",
          audienceId: internal ? squad?.leaderAgentId ?? null : null,
          targetType: null,
          targetId: null,
          failureReason: null,
          elapsedMs: null,
          messageKind: "message",
          quickActions: [],
          timeline: [],
          attachments: [],
          createdAt: new Date().toISOString(),
        };
        sessionMessages.push(existing);
      }
      if (existing && payload.timelineItem) {
        const index = existing.timeline.findIndex((item) => item.seq === payload.timelineItem?.seq);
        if (index >= 0) existing.timeline[index] = payload.timelineItem;
        else existing.timeline.push(payload.timelineItem);
        existing.timeline.sort((a, b) => a.seq - b.seq);
        if (payload.timelineItem.eventType === "permission" && payload.timelineItem.status !== "resolved") {
          void captureInboxNotice(payload);
        }
      }
      if (payload.chunk) {
        if (existing) {
          existing.content = applyAssistantChunk(existing.content, payload.chunk, {
            replace: Boolean(payload.replacesFinalText || payload.timelineItem?.replacesFinalText),
            completed: payload.status === "completed",
          });
        }
      } else if (payload.status === "failed" || payload.status === "stopped") {
        const content = payload.status === "stopped"
          ? "任务已停止。"
          : `任务执行失败：${payload.error ?? "未知错误"}`;
        if (existing) {
          existing.content = content;
          existing.failureReason = payload.error;
        }
      }
      if (existing && payload.elapsedMs != null) {
        existing.elapsedMs = payload.elapsedMs;
      }
      messages.value[payload.sessionId] = [...sessionMessages];
      if (["completed", "failed", "stopped"].includes(payload.status)) {
        const persisted = await backend.listChatMessages(payload.sessionId);
        const persistedIds = new Set(persisted.map((message) => message.id));
        const live = (messages.value[payload.sessionId] ?? []).filter((message) => {
          if (persistedIds.has(message.id) || !message.runId) return false;
          const status = runStates.value[message.runId]?.status;
          return status === "queued" || status === "running";
        });
        messages.value[payload.sessionId] = [...persisted, ...live];
        if (previous?.status === "running") {
          await captureInboxNotice(payload);
        }
        if (payload.status === "completed") {
          void useDiffStore().loadActive();
        }
        // 对话里可能通过 CLI 增删智能体/小队，结束后同步列表
        void refreshAgentsAndSquads();
        queueMicrotask(() => {
          if (!processingMessage.value) void drainMessageQueue();
        });
      }
    });
  }

  async function refreshRuntimes() {
    if (!data.value) return;
    data.value.runtimes = await backend.detectRuntimes();
  }

  async function configureRuntime(runtimeId: string, maxConcurrent: number) {
    if (!data.value) return;
    data.value.runtimes = await backend.configureRuntime(runtimeId, maxConcurrent);
  }

  async function saveCustomRuntime(input: CustomRuntimeInput) {
    if (!data.value) return;
    data.value.runtimes = await backend.saveCustomRuntime(input);
  }

  async function deleteCustomRuntime(runtimeId: string) {
    if (!data.value) return;
    data.value.runtimes = await backend.deleteCustomRuntime(runtimeId);
  }

  async function refreshResources() {
    if (!data.value) return;
    data.value.resources = await backend.scanResources();
  }

  async function transferResource(resourceId: string, targetOwner: string, copy: boolean) {
    if (!data.value) return;
    data.value.resources = await backend.transferResource(resourceId, targetOwner, copy);
  }

  async function createSkill(name: string, description: string, body: string, owner = "multara") {
    const created = await backend.createSkill(name, description, body, owner);
    await refreshResources();
    return created;
  }

  async function generateSkill(requirement: string, owner = "multara") {
    const created = await backend.generateSkill(requirement, owner);
    await refreshResources();
    return created;
  }

  async function importSkillFromFolder(path: string, owner = "multara") {
    const created = await backend.importSkillFromFolder(path, owner);
    await refreshResources();
    return created;
  }

  async function importSkillFromUrl(url: string, owner = "multara") {
    const created = await backend.importSkillFromUrl(url, owner);
    await refreshResources();
    return created;
  }

  async function deleteSkill(resourceId: string) {
    if (!data.value) return;
    data.value.resources = await backend.deleteSkill(resourceId);
  }

  async function setSkillEnabled(resourceId: string, enabled: boolean) {
    if (!data.value) return;
    data.value.resources = await backend.setSkillEnabled(resourceId, enabled);
  }

  async function convertPromptToSkill(path: string, owner = "multara") {
    const created = await backend.convertPromptToSkill(path, owner);
    await refreshResources();
    return created;
  }

  async function saveMcp(input: McpInput) {
    if (!data.value) return;
    data.value.resources = await backend.saveMcp(input);
  }

  async function installMcp(name: string, entryJson: string, owner = "multara") {
    if (!data.value) return;
    data.value.resources = await backend.installMcp(name, entryJson, owner);
  }

  async function deleteMcp(name: string, owner = "multara") {
    if (!data.value) return;
    data.value.resources = await backend.deleteMcp(name, owner);
  }

  async function setMcpEnabled(name: string, enabled: boolean, owner = "multara") {
    if (!data.value) return;
    data.value.resources = await backend.setMcpEnabled(name, enabled, owner);
  }

  async function authorizeMcp(name: string, owner = "multara") {
    if (!data.value) return;
    data.value.resources = await backend.authorizeMcp(name, owner);
  }

  async function signOutMcp(name: string) {
    if (!data.value) return;
    data.value.resources = await backend.signOutMcp(name);
  }

  async function saveAgent(input: Omit<Agent, "id"> & { id?: string }) {
    if (!data.value) return;
    data.value.agents = await backend.saveAgent(input);
  }

  async function deleteAgent(agentId: string) {
    if (!data.value) return;
    data.value.agents = await backend.deleteAgent(agentId);
  }

  async function saveSquad(input: Omit<Squad, "id"> & { id?: string }) {
    if (!data.value) return;
    data.value.squads = await backend.saveSquad(input);
  }

  async function deleteSquad(squadId: string) {
    if (!data.value) return;
    data.value.squads = await backend.deleteSquad(squadId);
  }

  async function createTask(input: TaskInput) {
    if (!data.value) return null;
    const task = await backend.createTask(input);
    data.value.tasks.unshift(task);
    if (!data.value.sessions.some((session) => session.id === task.sessionId)) {
      data.value.sessions.unshift({
        id: task.sessionId,
        projectId: input.projectId ?? null,
        title: task.title,
        targetType: "agent",
        targetId: task.agentId,
        createdAt: task.createdAt,
        updatedAt: task.updatedAt,
      });
    }
    messages.value[task.sessionId] = [];
    return task;
  }

  async function updateTask(input: {
    taskId: string;
    title?: string;
    description?: string;
    status?: TaskStatus;
    agentId?: string;
  }) {
    if (!data.value) return null;
    const task = await backend.updateTask(input);
    const index = data.value.tasks.findIndex((item) => item.id === task.id);
    if (index >= 0) data.value.tasks[index] = task;
    else data.value.tasks.unshift(task);
    const session = data.value.sessions.find((item) => item.id === task.sessionId);
    if (session) {
      session.title = task.title;
      session.targetId = task.agentId;
      session.updatedAt = task.updatedAt;
    }
    return task;
  }

  async function updateTaskMeta(input: Parameters<typeof backend.updateTaskMeta>[0]) {
    if (!data.value) return null;
    const task = await backend.updateTaskMeta(input);
    const index = data.value.tasks.findIndex((item) => item.id === task.id);
    if (index >= 0) data.value.tasks[index] = task;
    else data.value.tasks.unshift(task);
    return task;
  }

  async function updateTaskStatus(taskId: string, status: TaskStatus) {
    if (!data.value) return;
    data.value.tasks = await backend.updateTaskStatus(taskId, status);
  }

  function syncTaskStatusFromRunEvent(payload: RunEvent) {
    if (!data.value) return;
    const task = data.value.tasks.find((item) => item.sessionId === payload.sessionId);
    if (!task) return;
    const sessionStillActive = Object.values(runStates.value).some((event) =>
      event.sessionId === payload.sessionId
      && (event.status === "queued" || event.status === "running")
      && !event.runId.startsWith("pending:")
    );
    const next = boardStatusFromRun(payload.status, task.status, sessionStillActive);
    if (!next || next === task.status) return;
    // 后端 enqueue/finish 已落库；前端只同步本地看板，避免再打一枪 update_task_status。
    task.status = next;
    task.updatedAt = new Date().toISOString();
  }

  async function startTask(taskId: string) {
    const task = data.value?.tasks.find((item) => item.id === taskId);
    if (!task) return;
    // 挪进「进行中」时后端会按任务描述排队开跑，这里不再额外 send，避免跑两遍
    await updateTaskStatus(taskId, "in_progress");
    await selectSession(task.sessionId);
  }

  async function deleteTask(taskId: string, options?: { force?: boolean }) {
    if (!data.value) return;
    const task = data.value.tasks.find((item) => item.id === taskId);
    data.value.tasks = await backend.deleteTask(taskId, options?.force === true);
    if (task) {
      data.value.sessions = data.value.sessions.filter((session) => session.id !== task.sessionId);
      data.value.inbox = data.value.inbox.filter((item) => item.sessionId !== task.sessionId);
      delete messages.value[task.sessionId];
      for (const [runId, event] of Object.entries(runStates.value)) {
        if (event.sessionId !== task.sessionId) continue;
        if (event.status === "queued" || event.status === "running") {
          runStates.value[runId] = { ...event, status: "stopped", chunk: null };
        }
      }
    }
  }

  function sessionHasActiveRuns(sessionId: string) {
    return Object.values(runStates.value).some((event) =>
      event.sessionId === sessionId
      && (event.status === "queued" || event.status === "running")
      && !event.runId.startsWith("pending:")
    );
  }

  function isTaskRunningDeleteError(cause: unknown) {
    const message = cause instanceof Error ? cause.message : String(cause);
    return message.includes("正在执行") || message.includes("停止运行再删除");
  }

  async function confirmDeleteTask(taskId: string) {
    const task = data.value?.tasks.find((item) => item.id === taskId);
    if (!task) return false;
    const running = sessionHasActiveRuns(task.sessionId);
    const prompt = running
      ? `任务「${task.title}」正在执行，是否停止并删除？`
      : `删除任务「${task.title}」？`;
    if (!window.confirm(prompt)) return false;
    try {
      await deleteTask(taskId, { force: running });
      return true;
    } catch (cause) {
      if (running || !isTaskRunningDeleteError(cause)) throw cause;
      if (!window.confirm(`任务「${task.title}」正在执行，是否停止并删除？`)) return false;
      await deleteTask(taskId, { force: true });
      return true;
    }
  }

  async function saveAutomation(input: AutomationInput) {
    if (!data.value) return;
    data.value.automations = await backend.saveAutomation(input);
  }

  async function deleteAutomation(automationId: string) {
    if (!data.value) return;
    data.value.automations = await backend.deleteAutomation(automationId);
  }

  async function runAutomation(automationId: string) {
    await backend.runAutomation(automationId);
  }

  async function saveSettings(settings: Record<string, string>) {
    if (!data.value) return;
    data.value.settings = await backend.saveSettings(settings);
    const { applyLanguageSetting } = await import("../i18n");
    applyLanguageSetting(data.value.settings.language);
    setFileIconTheme(data.value.settings.fileIconTheme);
  }

  async function createProject(name: string, note: string) {
    if (!data.value) return;
    const project = await backend.createProject(name, note);
    data.value.projects.unshift(project);
    return project;
  }

  async function updateProject(id: string, name: string, note: string) {
    if (!data.value) return;
    const project = await backend.updateProject(id, name, note);
    const index = data.value.projects.findIndex((item) => item.id === id);
    if (index >= 0) data.value.projects[index] = project;
    return project;
  }

  async function detachWorkspace(workspaceId: string) {
    if (!data.value) return;
    await backend.detachWorkspace(workspaceId);
    const workspace = data.value.workspaces.find((item) => item.id === workspaceId);
    data.value.workspaces = data.value.workspaces.filter((item) => item.id !== workspaceId);
    if (!workspace) return;
    const project = data.value.projects.find((item) => item.id === workspace.projectId);
    if (project && project.workspaceCount > 0) project.workspaceCount -= 1;
  }

  async function setProjectArchived(projectId: string, archived: boolean) {
    if (!data.value) return;
    const archivedAt = await backend.setProjectArchived(projectId, archived);
    const project = data.value.projects.find((item) => item.id === projectId);
    if (project) project.archivedAt = archivedAt;
  }

  async function deleteProject(projectId: string) {
    if (!data.value) return;
    await backend.deleteProject(projectId);
    data.value.projects = data.value.projects.filter((project) => project.id !== projectId);
    data.value.workspaces = data.value.workspaces.filter((workspace) => workspace.projectId !== projectId);
    data.value.sessions = data.value.sessions.filter((session) => session.projectId !== projectId);
    data.value.inbox = data.value.inbox.filter((item) => item.projectId !== projectId);
    clearLastProjectsPath(projectId);
  }

  async function refreshTaskInboxSurfaces() {
    const next = await backend.bootstrap();
    if (!data.value) return;
    data.value.tasks = next.tasks;
    data.value.sessions = next.sessions;
    data.value.inbox = next.inbox;
  }

  /** CLI（如对话里 `multara agent create`）只写库，需主动拉回前端缓存。 */
  async function refreshAgentsAndSquads() {
    const next = await backend.bootstrap();
    if (!data.value) return;
    data.value.agents = next.agents;
    data.value.squads = next.squads;
  }

  async function captureInboxNotice(payload: RunEvent) {
    if (!data.value) return;
    // 自动化在后端自己落任务和收件箱；前端把看板/收件箱重新拉一遍即可。
    if (payload.sessionId.startsWith("automation-run:")) {
      await refreshTaskInboxSurfaces();
      return;
    }
    const session = data.value.sessions.find((item) => item.id === payload.sessionId);
    if (!session) return;
    const task = data.value.tasks.find((item) => item.sessionId === session.id) ?? null;
    const transcript = messages.value[payload.sessionId] ?? [];
    const lastAssistant = [...transcript].reverse().find((message) => message.role === "assistant");
    const waitingQuestion = Boolean(lastAssistant && extractQuestions(lastAssistant.content).questions.length);
    const waitingPermission = transcript.some((message) =>
      message.timeline.some((item) => item.eventType === "permission" && item.status !== "resolved"),
    );
    const status = waitingQuestion
      ? "needs_input"
      : waitingPermission
        ? "needs_permission"
        : payload.status;
    if (!shouldNotifyInbox({
      status,
      previousStatus: "running",
      sessionId: session.id,
      projectId: session.projectId,
      taskId: task?.id ?? null,
      currentPath: currentAppPath(),
    })) return;
    const item = await backend.upsertInboxItem({
      sessionId: session.id,
      projectId: session.projectId,
      taskId: task?.id ?? null,
      title: task?.title || session.title,
      preview: inboxPreview(
        transcript.filter((message) => message.role === "assistant").map((message) => message.content),
        payload.error ?? "",
      ),
      status,
    });
    const index = data.value.inbox.findIndex((entry) => entry.sessionId === item.sessionId);
    if (index >= 0) data.value.inbox.splice(index, 1);
    data.value.inbox.unshift(item);
    notifyDesktop(item.title, item.preview);
  }

  function notifyDesktop(title: string, body: string) {
    if (typeof Notification === "undefined") return;
    if (Notification.permission === "default") void Notification.requestPermission();
    if (Notification.permission !== "granted") return;
    try {
      new Notification(title, { body, silent: false });
    } catch {
      // 系统通知不可用时忽略
    }
  }

  async function markInboxRead(id: string, messageId?: string | null) {
    const item = data.value?.inbox.find((entry) => entry.id === id);
    if (!item || item.read) return;
    const previousCount = item.unreadCount;
    const previousCursor = item.readMessageId;
    item.read = true;
    item.unreadCount = 0;
    if (messageId) item.readMessageId = messageId;
    try {
      await backend.markInboxRead(id, messageId);
    } catch {
      item.read = false;
      item.unreadCount = previousCount;
      item.readMessageId = previousCursor;
    }
  }

  async function markInboxUnread(id: string) {
    const item = data.value?.inbox.find((entry) => entry.id === id);
    if (!item) return;
    item.read = false;
    item.unreadCount = Math.max(1, item.unreadCount);
    try {
      await backend.markInboxUnread(id);
    } catch {
      item.read = true;
    }
  }

  async function markInboxAllRead() {
    if (!data.value) return;
    const previous = data.value.inbox.map((item) => ({ id: item.id, read: item.read, unreadCount: item.unreadCount }));
    for (const item of data.value.inbox) {
      item.read = true;
      item.unreadCount = 0;
    }
    try {
      await backend.markInboxAllRead();
    } catch {
      for (const item of data.value.inbox) {
        const found = previous.find((entry) => entry.id === item.id);
        if (found) {
          item.read = found.read;
          item.unreadCount = found.unreadCount;
        }
      }
    }
  }

  async function archiveInboxItem(id: string) {
    if (!data.value) return;
    const index = data.value.inbox.findIndex((item) => item.id === id);
    if (index < 0) return;
    const [removed] = data.value.inbox.splice(index, 1);
    try {
      await backend.archiveInboxItem(id);
    } catch {
      if (removed) data.value.inbox.splice(index, 0, removed);
    }
  }

  async function deleteInboxItem(id: string) {
    if (!data.value) return;
    const index = data.value.inbox.findIndex((item) => item.id === id);
    if (index < 0) return;
    const [removed] = data.value.inbox.splice(index, 1);
    try {
      await backend.deleteInboxItem(id);
    } catch {
      if (removed) data.value.inbox.splice(index, 0, removed);
    }
  }

  async function forkSession(sessionId: string) {
    if (!data.value) return null;
    const session = await backend.forkChatSession(sessionId);
    data.value.sessions.unshift(session);
    messages.value[session.id] = await backend.listChatMessages(session.id);
    await selectSession(session.id);
    return session;
  }

  async function compactSession(sessionId: string) {
    return backend.compactChatSession(sessionId);
  }

  async function setMessageFeedback(messageId: string, feedback: "up" | "down" | null) {
    for (const list of Object.values(messages.value)) {
      const message = list.find((item) => item.id === messageId);
      if (message) message.feedback = feedback;
    }
    await backend.setMessageFeedback(messageId, feedback);
  }

  async function resolvePermission(requestId: string, optionId: string) {
    await backend.resolvePermission(requestId, optionId);
  }

  async function loadMessages(sessionId: string) {
    if (!sessionId || messages.value[sessionId]) return;
    messages.value[sessionId] = await backend.listChatMessages(sessionId);
  }

  async function refreshMessages(sessionId: string) {
    messages.value[sessionId] = await backend.listChatMessages(sessionId);
  }

  function persistOpenSessionTabs() {
    persistOpenTabsMap(openSessionTabs.value);
  }

  function hasOpenSessionTabs(projectId: string | null | undefined) {
    return Object.prototype.hasOwnProperty.call(openSessionTabs.value, tabScope(projectId));
  }

  function openedSessionIds(projectId: string | null | undefined) {
    return openSessionTabs.value[tabScope(projectId)] ?? [];
  }

  function projectSessionIds(projectId: string | null | undefined) {
    return (data.value?.sessions ?? [])
      .filter((session) => session.projectId === projectId)
      .map((session) => session.id);
  }

  function seedOpenSessionTabs(projectId: string | null | undefined, sessionIds?: string[]) {
    if (hasOpenSessionTabs(projectId)) return;
    const ids = sessionIds ?? projectSessionIds(projectId);
    const dismissed = loadLegacyDismissed(projectId);
    openSessionTabs.value = {
      ...openSessionTabs.value,
      [tabScope(projectId)]: dismissed.size ? ids.filter((id) => !dismissed.has(id)) : [...ids],
    };
    persistOpenSessionTabs();
    if (dismissed.size) clearLegacyDismissed(projectId);
  }

  function addOpenSessionTab(projectId: string | null | undefined, sessionId: string) {
    seedOpenSessionTabs(projectId);
    const current = openedSessionIds(projectId);
    if (current.includes(sessionId)) return;
    openSessionTabs.value = { ...openSessionTabs.value, [tabScope(projectId)]: [...current, sessionId] };
    persistOpenSessionTabs();
  }

  function closeOpenSessionTabs(projectId: string | null | undefined, sessionIds: string[]) {
    seedOpenSessionTabs(projectId);
    const closing = new Set(sessionIds);
    openSessionTabs.value = {
      ...openSessionTabs.value,
      [tabScope(projectId)]: openedSessionIds(projectId).filter((id) => !closing.has(id)),
    };
    persistOpenSessionTabs();
  }

  async function selectSession(sessionId: string) {
    activeSessionId.value = sessionId;
    await loadMessages(sessionId);
  }

  async function createChatSession(projectId: string | null, targetValue?: string) {
    if (!data.value) return null;
    const miku = data.value.agents.find((item) => item.systemKey === "miku") ?? data.value.agents[0];
    const fallback = miku
      ? `agent:${miku.id}`
      : data.value.squads[0]
        ? `squad:${data.value.squads[0].id}`
        : "";
    const [targetType, targetId] = (targetValue || fallback).split(":", 2);
    if (!targetId || !["agent", "squad"].includes(targetType)) {
      throw new Error("请先创建智能体或小队");
    }
    const session = await backend.createChatSession({
      projectId,
      targetType: targetType as "agent" | "squad",
      targetId,
      title: "新对话",
    });
    data.value.sessions.unshift(session);
    messages.value[session.id] = [];
    activeSessionId.value = session.id;
    addOpenSessionTab(projectId, session.id);
    return session;
  }

  async function ensureProjectSession(projectId: string) {
    const session = data.value?.sessions.find((item) => item.projectId === projectId);
    if (session) {
      await selectSession(session.id);
      return session;
    }
    return createChatSession(projectId);
  }

  function isUntitledSession(title: string) {
    return !title.trim() || title === "新对话" || /^与 .+ 对话$/.test(title);
  }

  async function updateSessionTitle(sessionId: string, title: string) {
    if (!data.value) return;
    const session = data.value.sessions.find((item) => item.id === sessionId);
    const previous = session?.title;
    const next = title.trim();
    if (!session || !next || session.title === next) return;
    session.title = next;
    try {
      session.title = await backend.updateChatSessionTitle(sessionId, next);
    } catch (error) {
      if (previous !== undefined) session.title = previous;
      throw error;
    }
  }

  async function nameSessionFromFirstPrompt(sessionId: string, content: string) {
    const session = data.value?.sessions.find((item) => item.id === sessionId);
    if (!session || !isUntitledSession(session.title)) return;
    const fallback = backend.titleFromPrompt(content);
    try {
      await updateSessionTitle(sessionId, fallback);
    } catch {
      // Keep the untitled session name when rename fails.
    }
  }

  async function setSessionTarget(sessionId: string, targetValue: string) {
    if (!data.value) return;
    const [targetType, targetId] = targetValue.split(":", 2);
    if (!targetId || !["agent", "squad"].includes(targetType)) return;
    const session = data.value.sessions.find((item) => item.id === sessionId);
    const previous = session ? { targetType: session.targetType, targetId: session.targetId } : null;
    if (session) {
      session.targetType = targetType as "agent" | "squad";
      session.targetId = targetId;
    }
    try {
      await backend.updateChatSessionTarget(
        sessionId,
        targetType as "agent" | "squad",
        targetId,
      );
    } catch (error) {
      if (session && previous) Object.assign(session, previous);
      throw error;
    }
  }

  async function addWorkspace(
    projectId: string,
    sourcePath: string,
    mode: "isolated" | "direct",
    baseRef?: string | null,
    checkoutName?: string | null,
    createBranch?: boolean,
  ) {
    if (!data.value) return;
    const workspace = await backend.addWorkspace(
      projectId,
      sourcePath,
      mode,
      baseRef,
      checkoutName,
      createBranch,
    );
    data.value.workspaces.push(workspace);
    const project = data.value.projects.find((item) => item.id === projectId);
    if (project) project.workspaceCount += 1;
  }

  function isProjectPreparing(projectId: string) {
    return workspaceJobs.value.some(
      (job) => job.projectId === projectId && job.status === "preparing",
    );
  }

  function dismissWorkspaceJob(id: string) {
    workspaceJobs.value = workspaceJobs.value.filter((job) => job.id !== id);
  }

  function queueAddWorkspaces(projectId: string, items: WorkspaceAddRequest[]) {
    const jobs = items.map((item) => ({
      id: crypto.randomUUID(),
      projectId,
      name: item.name,
      path: item.path,
      status: "preparing" as const,
    }));
    workspaceJobs.value = [...workspaceJobs.value, ...jobs];
    void (async () => {
      for (const [index, item] of items.entries()) {
        const job = jobs[index];
        try {
          await addWorkspace(
            projectId,
            item.path,
            item.mode,
            item.baseRef,
            item.checkoutName,
            item.createBranch,
          );
          dismissWorkspaceJob(job.id);
        } catch (cause) {
          const current = workspaceJobs.value.find((entry) => entry.id === job.id);
          if (current) {
            current.status = "failed";
            current.error = cause instanceof Error ? cause.message : String(cause);
          }
        }
      }
    })();
  }

  function send(
    content: string,
    sessionId = activeSessionId.value,
    attachments: MessageAttachmentInput[] = [],
    options: { priority?: boolean } = {},
  ) {
    const text = content.trim();
    if (!text && !attachments.length) return;
    if (!sessionId) return;
    const item = { sessionId, content: text, attachments, priority: options.priority };
    if (options.priority) queuedMessages.value.unshift(item);
    else queuedMessages.value.push(item);
    void drainMessageQueue();
  }

  /**
   * 最后一条用户消息之后，助手还有没回答也没跳过的提问。
   * 这时排队消息先停住，不然会被当成回答发出去，提问卡片直接消失。
   */
  function awaitingAnswer(sessionId: string) {
    const list = messages.value[sessionId] ?? [];
    for (let index = list.length - 1; index >= 0; index -= 1) {
      const message = list[index];
      if (message.role === "user") return false;
      if (message.role !== "assistant" || message.failureReason) continue;
      if (dismissedQuestions.value.has(message.id)) continue;
      if (extractQuestions(message.content).questions.length) return true;
    }
    return false;
  }

  function dismissQuestion(messageId: string) {
    const next = [...dismissedQuestions.value, messageId].slice(-200);
    dismissedQuestions.value = new Set(next);
    try {
      localStorage.setItem(DISMISSED_QUESTIONS_KEY, JSON.stringify(next));
    } catch {
      // 存储不可用时只在本次会话内生效
    }
    // 跳过提问后，被它挡住的排队消息继续发
    void drainMessageQueue();
  }

  function mergeSessionFor(workspaceId: string) {
    return data.value?.mergeSessions?.find((item) => item.workspaceId === workspaceId) ?? null;
  }

  /** 合并会话的执行者：优先沿用当前同需求会话选的智能体，否则用默认智能体。 */
  function mergeChatTarget(projectId: string) {
    const active = data.value?.sessions.find((item) => item.id === activeSessionId.value);
    if (active?.projectId === projectId) {
      return { type: active.targetType, id: active.targetId };
    }
    const miku = data.value?.agents.find((item) => item.systemKey === "miku") ?? data.value?.agents[0];
    if (miku) return { type: "agent" as const, id: miku.id };
    const squad = data.value?.squads[0];
    if (squad) return { type: "squad" as const, id: squad.id };
    throw new Error("请先创建智能体或小队");
  }

  async function mergeIntoBranch(workspaceId: string, target: string, push: boolean) {
    if (!data.value) throw new Error("数据还没有加载");
    const workspace = data.value.workspaces.find((item) => item.id === workspaceId);
    if (!workspace) throw new Error("工作区不存在");
    const chat = mergeChatTarget(workspace.projectId);
    const result = await backend.gitMergeIntoBranch({
      workspaceId,
      target,
      push,
      chatTargetType: chat.type,
      chatTargetId: chat.id,
    });
    if (result.status === "conflict" && result.session && result.workspace && result.mergeSession) {
      data.value.sessions.unshift(result.session);
      data.value.workspaces.push(result.workspace);
      data.value.mergeSessions = [...(data.value.mergeSessions ?? []), result.mergeSession];
      messages.value[result.session.id] = [];
      addOpenSessionTab(result.session.projectId, result.session.id);
    }
    return result;
  }

  async function openMergeSession(mergeId: string) {
    const merge = data.value?.mergeSessions?.find((item) => item.id === mergeId);
    if (!merge) return;
    addOpenSessionTab(merge.projectId, merge.sessionId);
    await selectSession(merge.sessionId);
  }

  function mergeResolvePrompt(merge: MergeSession) {
    const target = merge.targetRemote ? `${merge.targetRemote}/${merge.targetBranch}` : merge.targetBranch;
    return [
      `请帮我解决合并冲突：把 \`${merge.sourceLabel}\` 合并到 \`${target}\`。`,
      "当前工作区是专门为这次合并创建的临时 Worktree，已经处于合并中状态，下面这些文件有冲突：",
      ...merge.conflicts.map((file) => `- ${file}`),
      "",
      "要求：",
      "1. 逐个打开冲突文件，先理解两边改动的意图再合并，去掉所有 <<<<<<< / ======= / >>>>>>> 冲突标记；",
      "2. 尽量同时保留两边的改动，无法兼容时说明你的取舍；",
      "3. 不要执行 git commit、git merge --abort、git checkout / switch、git reset 等改变合并状态的命令，完成合并由我在界面上确认；",
      "4. 改完后逐个文件简要说明是怎么解决的。",
    ].join("\n");
  }

  async function resolveMergeWithAi(mergeId: string) {
    const merge = data.value?.mergeSessions?.find((item) => item.id === mergeId);
    if (!merge) return;
    await openMergeSession(mergeId);
    send(mergeResolvePrompt(merge), merge.sessionId);
  }

  function dropMergeSession(mergeId: string) {
    if (!data.value) return;
    const merge = data.value.mergeSessions?.find((item) => item.id === mergeId);
    data.value.mergeSessions = (data.value.mergeSessions ?? []).filter((item) => item.id !== mergeId);
    if (merge) {
      data.value.workspaces = data.value.workspaces.filter((item) => item.id !== merge.workspaceId);
    }
  }

  async function finishMergeSession(mergeId: string) {
    const result = await backend.gitFinishMergeSession(mergeId);
    dropMergeSession(mergeId);
    return result;
  }

  async function abortMergeSession(mergeId: string) {
    await backend.gitAbortMergeSession(mergeId);
    dropMergeSession(mergeId);
  }

  function sessionWorkspaces(sessionId: string) {
    const session = data.value?.sessions.find((item) => item.id === sessionId);
    const projectWorkspaces =
      data.value?.workspaces.filter((item) => item.projectId === session?.projectId) ?? [];
    const dedicated = projectWorkspaces.filter((item) => item.sessionId === sessionId);
    return dedicated.length
      ? dedicated
      : projectWorkspaces.filter((item) => !item.sessionId);
  }

  async function reaskFrom(
    messageId: string,
    content: string,
    attachments: MessageAttachmentInput[] = [],
    rollback = false,
    confirmRollbackFailure?: (message: string) => Promise<boolean>,
  ) {
    const text = content.trim();
    if (!text && !attachments.length) return;
    const sessionId = Object.entries(messages.value)
      .find(([, items]) => items.some((item) => item.id === messageId))
      ?.[0]
      ?? activeSessionId.value;
    if (!sessionId) return;
    queuedMessages.value = queuedMessages.value.filter((item) => item.sessionId !== sessionId);
    await stopSessionRuns(sessionId);
    const diffStore = useDiffStore();
    const result = await executeReask({
      rollback,
      dirtyPaths: [],
      confirmDirty: async () => true,
      confirmRollbackFailure,
      rollbackToTurn: async () => {
        const failures: string[] = [];
        for (const workspace of sessionWorkspaces(sessionId)) {
          try {
            await diffStore.rollbackToTurn(messageId, workspace.id, sessionId);
          } catch (error) {
            const reason = error instanceof Error ? error.message : String(error);
            failures.push(`${workspace.name}：${reason}`);
          }
        }
        if (failures.length) throw new Error(failures.join("\n"));
      },
      truncateSession: async () => {
        await backend.truncateSessionFromMessage(sessionId, messageId, rollback);
        if (!rollback) diffStore.clearChunkStatesFrom(messageId, sessionId);
        messages.value[sessionId] = await backend.listChatMessages(sessionId);
      },
      reloadDiff: () => diffStore.loadActive(),
      send: () => send(text, sessionId, attachments),
    });
    if (result === "aborted") return;
  }

  async function addWorkspaceFileContext(
    workspaceId: string,
    path: string,
    options?: { isDirectory?: boolean },
  ) {
    if (!workspaceId || !path) return;
    let isDirectory = options?.isDirectory ?? false;
    let text = "";
    let endLine = 1;
    let resolvedPath = path;
    let unreadable = false;
    if (options?.isDirectory === undefined) {
      try {
        const file = await backend.readWorkspaceFile(workspaceId, path);
        resolvedPath = file.path || path;
        text = file.content;
        endLine = Math.max(1, file.content.split(/\r\n|\r|\n/).length);
      } catch {
        try {
          const entries = await backend.listWorkspaceDirectory(workspaceId, path);
          isDirectory = true;
          text = entries.map((entry) => (entry.isDirectory ? `${entry.name}/` : entry.name)).join("\n");
        } catch {
          unreadable = true;
        }
      }
    } else if (isDirectory) {
      try {
        const entries = await backend.listWorkspaceDirectory(workspaceId, path);
        text = entries.map((entry) => (entry.isDirectory ? `${entry.name}/` : entry.name)).join("\n");
      } catch {
        unreadable = true;
      }
    } else {
      try {
        const file = await backend.readWorkspaceFile(workspaceId, path);
        resolvedPath = file.path || path;
        text = file.content;
        endLine = Math.max(1, file.content.split(/\r\n|\r|\n/).length);
      } catch {
        unreadable = true;
      }
    }
    if (unreadable) {
      const name = resolvedPath.split(/[\\/]/).pop() || resolvedPath;
      composerNotice.value = `未能读取 ${name}，已添加空引用`;
    }
    addComposerContext({
      workspaceId,
      path: resolvedPath,
      startLine: 0,
      endLine: isDirectory ? 0 : endLine,
      text,
      isDirectory,
    });
  }

  function addComposerContext(input: Omit<EditorSelectionContext, "id">) {
    const existing = composerContexts.value.find((item) =>
      item.path === input.path && item.startLine === input.startLine && item.endLine === input.endLine,
    );
    if (existing) {
      existing.text = input.text;
      composerContexts.value = [
        ...composerContexts.value.filter((item) => item.id !== existing.id),
        existing,
      ];
      composerFocusNonce.value += 1;
      return existing;
    }
    const created: EditorSelectionContext = { ...input, id: crypto.randomUUID() };
    composerContexts.value = [...composerContexts.value, created].slice(-8);
    composerFocusNonce.value += 1;
    return created;
  }

  function removeComposerContext(id: string) {
    composerContexts.value = composerContexts.value.filter((item) => item.id !== id);
  }

  function clearComposerContexts() {
    composerContexts.value = [];
  }

  function reportComposerNotice(message: string, searchQuery = "") {
    composerNotice.value = message;
    composerNoticeQuery.value = searchQuery;
  }

  function clearComposerNotice() {
    composerNotice.value = "";
    composerNoticeQuery.value = "";
  }

  function requestQuickOpen(query: string) {
    quickOpenRequest.value = { query, nonce: Date.now() };
  }

  function dismissSendFailure() {
    sendFailure.value = null;
  }

  function revealWorkspaceFile(
    path: string,
    startLine?: number,
    endLine?: number,
    workspaceId?: string,
  ) {
    fileReveal.value = { path, startLine, endLine, workspaceId, nonce: Date.now() };
  }

  function removeQueuedMessage(index: number) {
    if (index < 0 || index >= queuedMessages.value.length) return;
    queuedMessages.value.splice(index, 1);
  }

  /** 取出一条排队消息（编辑时放回输入框） */
  function takeQueuedMessage(index: number) {
    if (index < 0 || index >= queuedMessages.value.length) return null;
    return queuedMessages.value.splice(index, 1)[0] ?? null;
  }

  /** 立即发送：挪到最前并越过提问暂停；正在运行时会在这一轮结束后第一个发 */
  function promoteQueuedMessage(index: number) {
    const item = takeQueuedMessage(index);
    if (!item) return;
    queuedMessages.value.unshift({ ...item, priority: true });
    void drainMessageQueue();
  }

  async function drainMessageQueue() {
    if (processingMessage.value) return;
    const nextIndex = queuedMessages.value.findIndex((item) =>
      item.priority || !awaitingAnswer(item.sessionId),
    );
    if (nextIndex < 0) return;
    sendingMessage.value = true;
    try {
      const next = queuedMessages.value.splice(nextIndex, 1)[0];
      if (!next) return;
      const isFirstPrompt = !(messages.value[next.sessionId] ?? []).some((item) => item.role === "user");
      const optimisticId = crypto.randomUUID();
      const session = data.value?.sessions.find((item) => item.id === next.sessionId);
      messages.value[next.sessionId] = [
        ...(messages.value[next.sessionId] ?? []),
        {
          id: optimisticId,
          sessionId: next.sessionId,
          role: "user",
          content: next.content,
          runId: null,
          agentId: null,
          audienceType: null,
          audienceId: null,
          targetType: session?.targetType ?? null,
          targetId: session?.targetId ?? null,
          failureReason: null,
          elapsedMs: null,
          messageKind: "message",
          quickActions: [],
          timeline: [],
          attachments: next.attachments.map((attachment, index) => ({
            ...attachment,
            id: `pending:${optimisticId}:${index}`,
          })),
          createdAt: new Date().toISOString(),
        },
      ];
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
      });
      try {
        const result = await backend.sendMessage(next.sessionId, next.content, next.attachments);
        const returnedUser = result.find((message) => message.role === "user");
        if (returnedUser) {
          const current = messages.value[next.sessionId] ?? [];
          const index = current.findIndex((message) => message.id === optimisticId);
          if (index >= 0) current[index] = returnedUser;
        }
        const assistant = result.filter((message) => message.role === "assistant");
        messages.value[next.sessionId] = [...(messages.value[next.sessionId] ?? []), ...assistant];
        if (isFirstPrompt) void nameSessionFromFirstPrompt(next.sessionId, next.content);
        if (assistant.length) queueMicrotask(() => void drainMessageQueue());
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        messages.value[next.sessionId] = (messages.value[next.sessionId] ?? []).filter(
          (item) => item.id !== optimisticId,
        );
        sendFailure.value = {
          nonce: Date.now(),
          sessionId: next.sessionId,
          content: next.content,
          attachments: next.attachments,
          message: `没有发出：${message}`,
        };
      }
    } finally {
      sendingMessage.value = false;
    }
  }

  async function stopSessionRuns(sessionId: string) {
    const ids = Object.values(runStates.value)
      .filter((event) =>
        event.sessionId === sessionId &&
        (event.status === "queued" || event.status === "running") &&
        !event.runId.startsWith("pending:")
      )
      .map((event) => event.runId);
    if (!ids.length) return;
    await Promise.all(ids.map((runId) => backend.stopRun(runId).catch(() => undefined)));
    for (const runId of ids) {
      const event = runStates.value[runId];
      if (!event || (event.status !== "queued" && event.status !== "running")) continue;
      runStates.value[runId] = { ...event, status: "stopped", chunk: null };
    }
  }

  async function stopActiveRun() {
    await stopSessionRuns(activeSessionId.value);
  }

  return {
    data,
    messages,
    loading,
    error,
    chatOpen,
    activeSessionId,
    queuedMessages,
    composerContexts,
    composerFocusNonce,
    composerNotice,
    composerNoticeQuery,
    quickOpenRequest,
    sendFailure,
    fileReveal,
    runStates,
    processingMessage,
    activeRunIds,
    activeSession,
    initialize,
    refreshRuntimes,
    configureRuntime,
    saveCustomRuntime,
    deleteCustomRuntime,
    refreshResources,
    refreshAgentsAndSquads,
    transferResource,
    createSkill,
    generateSkill,
    importSkillFromFolder,
    importSkillFromUrl,
    deleteSkill,
    setSkillEnabled,
    convertPromptToSkill,
    saveMcp,
    installMcp,
    deleteMcp,
    setMcpEnabled,
    authorizeMcp,
    signOutMcp,
    saveAgent,
    deleteAgent,
    saveSquad,
    deleteSquad,
    createTask,
    updateTask,
    updateTaskMeta,
    updateTaskStatus,
    startTask,
    deleteTask,
    confirmDeleteTask,
    saveAutomation,
    deleteAutomation,
    runAutomation,
    saveSettings,
    createProject,
    updateProject,
    deleteProject,
    setProjectArchived,
    detachWorkspace,
    addWorkspace,
    workspaceJobs,
    isProjectPreparing,
    dismissWorkspaceJob,
    queueAddWorkspaces,
    loadMessages,
    markInboxRead,
    markInboxUnread,
    markInboxAllRead,
    archiveInboxItem,
    deleteInboxItem,
    forkSession,
    compactSession,
    setMessageFeedback,
    resolvePermission,
    selectSession,
    openSessionTabs,
    hasOpenSessionTabs,
    openedSessionIds,
    seedOpenSessionTabs,
    addOpenSessionTab,
    closeOpenSessionTabs,
    createChatSession,
    ensureProjectSession,
    updateSessionTitle,
    setSessionTarget,
    send,
    reaskFrom,
    addComposerContext,
    addWorkspaceFileContext,
    removeComposerContext,
    clearComposerContexts,
    reportComposerNotice,
    requestQuickOpen,
    clearComposerNotice,
    dismissSendFailure,
    revealWorkspaceFile,
    removeQueuedMessage,
    takeQueuedMessage,
    promoteQueuedMessage,
    dismissedQuestions,
    dismissQuestion,
    awaitingAnswer,
    sessionWorkspaces,
    mergeSessionFor,
    mergeIntoBranch,
    openMergeSession,
    resolveMergeWithAi,
    finishMergeSession,
    abortMergeSession,
    stopActiveRun,
    stopSessionRuns,
  };
});
