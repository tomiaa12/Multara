// @vitest-environment happy-dom
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { BootstrapData, ChatMessage, RunEvent, Task } from "../domain/types";
import type { TaskInboxSurfaces } from "../services/backend";

const events = vi.hoisted(() => ({
  handler: undefined as undefined | ((event: { payload: RunEvent }) => Promise<void>),
}));
const backend = vi.hoisted(() => ({
  bootstrap: vi.fn(),
  listTaskInboxSurfaces: vi.fn(),
  listChatMessages: vi.fn(async () => []),
  listAgentsAndSquads: vi.fn(async () => ({ agents: [], squads: [] })),
  sendMessage: vi.fn<(sessionId: string, content: string, attachments: unknown[]) => Promise<ChatMessage[]>>(),
  stopRun: vi.fn(),
  upsertInboxItem: vi.fn(async (input: Record<string, unknown>) => ({ id: "notice", ...input })),
}));

vi.mock("../services/backend", () => backend);
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name: string, handler: (event: { payload: RunEvent }) => Promise<void>) => {
    events.handler = handler;
    return () => undefined;
  }),
}));
vi.mock("../file-icons", () => ({ setFileIconTheme: vi.fn() }));
vi.mock("../i18n", () => ({
  applyLanguageSetting: vi.fn(),
  t: (key: string) => key,
  translate: (key: string) => key,
}));
vi.mock("./diff", () => ({
  useDiffStore: () => ({ loadActive: vi.fn(async () => undefined) }),
}));
vi.mock("./mcp-tools", () => ({
  useMcpToolsStore: () => ({ hydrate: vi.fn(async () => undefined), refreshInBackground: vi.fn() }),
}));
vi.mock("./updater", () => ({
  useUpdaterStore: () => ({ boot: vi.fn(async () => undefined) }),
}));
vi.mock("../services/update", () => ({ setUpdateProxy: vi.fn() }));

import { useAppStore } from "./app";

function initialData(): BootstrapData {
  return {
    projects: [],
    workspaces: [],
    runtimes: [],
    agents: [],
    squads: [],
    sessions: [{
      id: "welcome",
      projectId: null,
      title: "Chat",
      targetType: "agent",
      targetId: "miku",
      createdAt: "now",
      updatedAt: "now",
    }],
    tasks: [],
    resources: [],
    stats: { running: 0, queued: 0, completedToday: 0, failedToday: 0 },
    automations: [],
    inbox: [],
    settings: { theme: "dark" },
  };
}

function surfaces(tasks: Task[] = []): TaskInboxSurfaces {
  return { tasks, sessions: initialData().sessions, inbox: [] };
}

function task(status: Task["status"] = "in_progress"): Task {
  return {
    id: "task-cli",
    sessionId: "welcome",
    title: "CLI task",
    description: "",
    agentId: "miku",
    status,
    createdAt: "now",
    updatedAt: "now",
  };
}

function emit(overrides: Partial<RunEvent> = {}) {
  return events.handler!({ payload: {
    runId: "run-1",
    sessionId: "welcome",
    agentId: "miku",
    status: "running",
    chunk: null,
    error: null,
    timelineItem: null,
    elapsedMs: null,
    ...overrides,
  } });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

async function flush() {
  for (let index = 0; index < 12; index += 1) await Promise.resolve();
}

describe("run-event surface refresh", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    localStorage.clear();
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    window.location.hash = "#/tasks/task-cli";
    events.handler = undefined;
    backend.bootstrap.mockResolvedValue(initialData());
    backend.listTaskInboxSurfaces.mockReset().mockImplementation(async () => surfaces());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  test("ordinary chat streams refresh only at lifecycle boundaries, never through bootstrap", async () => {
    const store = useAppStore();
    await store.initialize();
    expect(store.error).toBe("");
    expect(events.handler).toBeDefined();
    await emit({ status: "queued" });
    await flush();
    await emit();
    await flush();
    const calls = backend.listTaskInboxSurfaces.mock.calls.length;
    for (let index = 0; index < 50; index += 1) await emit({ chunk: "x" });
    await flush();
    expect(backend.listTaskInboxSurfaces).toHaveBeenCalledTimes(calls);
    expect(store.messages.welcome?.[0]?.content).toBe("x".repeat(50));
    await emit({ status: "completed" });
    expect(backend.listTaskInboxSurfaces).toHaveBeenCalledTimes(calls + 1);
    expect(backend.bootstrap).toHaveBeenCalledTimes(1);
  });

  test("initial queued event discovers a CLI task for an already known chat", async () => {
    const store = useAppStore();
    await store.initialize();
    backend.listTaskInboxSurfaces.mockResolvedValueOnce(surfaces([task()]));
    await emit({ status: "queued" });
    await flush();
    expect(store.data?.tasks.map((item) => item.id)).toEqual(["task-cli"]);
    expect(backend.bootstrap).toHaveBeenCalledTimes(1);
  });

  test("a terminal event discovers tasks created during the run", async () => {
    const store = useAppStore();
    await store.initialize();
    await emit();
    await flush();
    expect(store.data?.tasks).toEqual([]);
    backend.listTaskInboxSurfaces.mockResolvedValueOnce(surfaces([task("in_review")]));
    await emit({ status: "completed" });
    expect(store.data?.tasks[0]?.status).toBe("in_review");
    expect(backend.bootstrap).toHaveBeenCalledTimes(1);
  });

  test("concurrent lifecycle requests run serially with one trailing refresh", async () => {
    const store = useAppStore();
    await store.initialize();
    const first = deferred<TaskInboxSurfaces>();
    const second = deferred<TaskInboxSurfaces>();
    backend.listTaskInboxSurfaces
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    await emit({ status: "queued" });
    for (let index = 2; index <= 12; index += 1) {
      await emit({ runId: `run-${index}`, status: "queued" });
    }
    expect(backend.listTaskInboxSurfaces).toHaveBeenCalledTimes(1);
    first.resolve(surfaces());
    await flush();
    expect(backend.listTaskInboxSurfaces).toHaveBeenCalledTimes(2);
    second.resolve(surfaces([task()]));
    await flush();
    expect(backend.listTaskInboxSurfaces).toHaveBeenCalledTimes(2);
    expect(store.data?.tasks[0]?.id).toBe("task-cli");
  });

  test("a failed refresh is reported and does not block the next lifecycle refresh", async () => {
    const store = useAppStore();
    await store.initialize();
    const failure = new Error("read failed");
    const log = vi.spyOn(console, "error").mockImplementation(() => undefined);
    backend.listTaskInboxSurfaces.mockRejectedValueOnce(failure);
    await emit({ status: "queued" });
    await flush();
    expect(log).toHaveBeenCalledWith("[app] task/inbox refresh failed", failure);
    backend.listTaskInboxSurfaces.mockResolvedValueOnce(surfaces([task()]));
    await emit();
    await flush();
    expect(backend.listTaskInboxSurfaces).toHaveBeenCalledTimes(2);
    expect(store.data?.tasks[0]?.id).toBe("task-cli");
  });

  test("a terminal event arriving during refresh waits for a fresh trailing snapshot", async () => {
    const store = useAppStore();
    await store.initialize();
    const first = deferred<TaskInboxSurfaces>();
    const second = deferred<TaskInboxSurfaces>();
    backend.listTaskInboxSurfaces
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    await emit();
    const completion = emit({ status: "completed" });
    first.resolve(surfaces());
    await flush();
    expect(backend.listTaskInboxSurfaces).toHaveBeenCalledTimes(2);
    second.resolve(surfaces([task("in_review")]));
    await completion;
    expect(store.data?.tasks[0]?.status).toBe("in_review");
  });
});

function userMessage(sessionId: string, content: string): ChatMessage {
  return {
    id: `user:${sessionId}:${content}`, sessionId, role: "user", content,
    runId: null, agentId: null, audienceType: null, audienceId: null,
    targetType: "agent", targetId: "miku", failureReason: null, elapsedMs: null,
    messageKind: "message", quickActions: [], timeline: [], attachments: [], createdAt: "now",
  };
}

describe("per-session message queues", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 0));
    setActivePinia(createPinia());
    vi.clearAllMocks();
    localStorage.clear();
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    window.location.hash = "#/tasks/task-cli";
    events.handler = undefined;
    const data = initialData();
    data.sessions[0]!.projectId = "project";
    data.sessions.push({ ...data.sessions[0]!, id: "other", title: "Other chat" });
    backend.bootstrap.mockResolvedValue(data);
    backend.listTaskInboxSurfaces.mockReset().mockResolvedValue({ tasks: [], sessions: data.sessions, inbox: [] });
    backend.listChatMessages.mockResolvedValue([]);
    backend.sendMessage.mockReset().mockImplementation(async (sessionId, content) => [userMessage(sessionId, content)]);
    await useAppStore().initialize();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  test("an active tab does not block another tab in the same project", async () => {
    const store = useAppStore();
    await emit();
    store.send("parallel request", "other");
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledWith("other", "parallel request", []);
    expect(store.sessionProcessing("welcome")).toBe(true);
    expect(store.queuedMessages).toEqual([]);
  });

  test("in-flight sends in different tabs proceed independently", async () => {
    const first = deferred<ChatMessage[]>();
    const second = deferred<ChatMessage[]>();
    backend.sendMessage.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const store = useAppStore();
    store.send("first", "welcome");
    store.send("second", "other");
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledTimes(2);
    expect(store.sessionProcessing("welcome")).toBe(true);
    expect(store.sessionProcessing("other")).toBe(true);
    first.resolve([userMessage("welcome", "first")]);
    second.resolve([userMessage("other", "second")]);
    await flush();
  });

  test("normal messages wait for their own session, not the selected tab", async () => {
    const store = useAppStore();
    await emit();
    store.activeSessionId = "other";
    store.send("wait here", "welcome");
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).not.toHaveBeenCalled();
    expect(store.queuedMessages).toHaveLength(1);
    await emit({ status: "completed" });
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledWith("welcome", "wait here", []);
  });

  test("send now submits while the session is running without stopping it", async () => {
    const store = useAppStore();
    await emit();
    store.send("later", "welcome");
    store.send("send now", "welcome");
    store.promoteQueuedMessage(1);
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledTimes(1);
    expect(backend.sendMessage).toHaveBeenCalledWith("welcome", "send now", []);
    expect(backend.stopRun).not.toHaveBeenCalled();
    expect(store.queuedMessages.map((item) => item.content)).toEqual(["later"]);
  });

  test("send now is not blocked by a run that has not emitted its first event", async () => {
    const store = useAppStore();
    store.send("first", "welcome");
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledTimes(1);
    store.send("second", "welcome");
    expect(store.queuedMessages.map((item) => item.content)).toEqual(["second"]);
    store.promoteQueuedMessage(0);
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledTimes(2);
    expect(backend.sendMessage).toHaveBeenLastCalledWith("welcome", "second", []);
  });

  test("a send now failure is surfaced with the draft, not silently requeued", async () => {
    const store = useAppStore();
    await emit();
    backend.sendMessage.mockRejectedValueOnce(new Error("runtime cannot accept input"));
    store.send("recover me", "welcome");
    store.promoteQueuedMessage(0);
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledTimes(1);
    expect(store.sendFailure?.content).toBe("recover me");
    expect(store.messages.welcome?.some((message) => message.content === "recover me")).toBe(false);
    expect(store.queuedMessages).toEqual([]);
  });

  test("waiting for the first run event keeps a second normal message queued", async () => {
    const store = useAppStore();
    store.send("first", "welcome");
    store.send("second", "welcome");
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledTimes(1);
    expect(store.queuedMessages.map((item) => item.content)).toEqual(["second"]);
    await emit({ status: "queued" });
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledTimes(1);
  });

  test("finishing a session drains its queue while another selected tab is running", async () => {
    const store = useAppStore();
    await emit();
    await emit({ runId: "other-run", sessionId: "other" });
    store.send("next welcome", "welcome");
    store.activeSessionId = "other";
    await emit({ status: "completed" });
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).toHaveBeenCalledWith("welcome", "next welcome", []);
    expect(store.sessionProcessing("other")).toBe(true);
  });

  test("stopping a run keeps the session queue instead of auto-sending", async () => {
    const store = useAppStore();
    await emit();
    store.send("queued after stop", "welcome");
    await vi.runAllTimersAsync();
    expect(store.queuedMessages.map((item) => item.content)).toEqual(["queued after stop"]);
    await emit({ status: "stopped" });
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).not.toHaveBeenCalled();
    expect(store.queuedMessages.map((item) => item.content)).toEqual(["queued after stop"]);
  });

  test("a failed run keeps the session queue instead of auto-sending", async () => {
    const store = useAppStore();
    await emit();
    store.send("queued after fail", "welcome");
    await vi.runAllTimersAsync();
    await emit({ status: "failed", error: "boom" });
    await vi.runAllTimersAsync();
    expect(backend.sendMessage).not.toHaveBeenCalled();
    expect(store.queuedMessages.map((item) => item.content)).toEqual(["queued after fail"]);
  });

  test("a completion refresh preserves a message being submitted immediately", async () => {
    const store = useAppStore();
    await emit();
    const sending = deferred<ChatMessage[]>();
    backend.sendMessage.mockReturnValueOnce(sending.promise);
    store.send("during completion", "welcome");
    store.promoteQueuedMessage(0);
    await vi.runAllTimersAsync();
    await emit({ status: "completed" });
    expect(store.messages.welcome?.some((message) => message.content === "during completion")).toBe(true);
    sending.resolve([userMessage("welcome", "during completion")]);
    await flush();
    expect(store.messages.welcome?.filter((message) => message.content === "during completion")).toHaveLength(1);
  });
});
