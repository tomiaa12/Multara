import { ref } from "vue";
import type { TerminalTab } from "../components/workbench/TerminalPanel.vue";
import * as backend from "../services/backend";
import { forgetTerminal, trackTerminal } from "./running-terminals";

/** 工作台和收件箱共用的终端清单。卸掉画面不从这里删，只有用户在工作台关掉才删。 */
export const terminalSessions = ref<TerminalTab[]>([]);
export const activeTerminalSessionId = ref("");

const opening = new Map<string, Promise<TerminalTab>>();

export function liveTerminalForWorkspace(workspaceId: string): TerminalTab | undefined {
  return [...terminalSessions.value].reverse().find((item) =>
    item.workspaceId === workspaceId && item.live && !item.pending,
  );
}

/** 这个工作区已有活着的终端就附着最近一个，没有才新建。 */
export async function ensureWorkspaceTerminal(input: {
  workspaceId: string;
  cwd: string;
  title: string;
  projectId: string;
}): Promise<TerminalTab> {
  const existing = liveTerminalForWorkspace(input.workspaceId);
  if (existing) {
    activeTerminalSessionId.value = existing.id;
    return existing;
  }
  const pending = opening.get(input.workspaceId);
  if (pending) return pending;
  const task = openWorkspaceTerminal(input);
  opening.set(input.workspaceId, task);
  try {
    return await task;
  } finally {
    opening.delete(input.workspaceId);
  }
}

async function openWorkspaceTerminal(input: {
  workspaceId: string;
  cwd: string;
  title: string;
  projectId: string;
  profile?: string;
  groupId?: string;
}): Promise<TerminalTab> {
  const profile = input.profile || localStorage.getItem("multara.terminal.defaultProfile") || undefined;
  let id = "";
  let live = true;
  let message = "";
  let profileId = profile;
  let shell = "";
  let pid = 0;
  try {
    const opened = await backend.terminalOpen(input.cwd, profile);
    id = opened.id;
    profileId = opened.profileId;
    shell = opened.shell;
    pid = opened.pid;
  } catch (error) {
    live = false;
    id = `offline-${crypto.randomUUID()}`;
    message = error instanceof Error ? error.message : "";
  }
  if (live && input.projectId) {
    trackTerminal({
      id,
      title: input.title,
      projectId: input.projectId,
      workspaceId: input.workspaceId,
    });
  }
  const created: TerminalTab = {
    id,
    title: input.title,
    cwd: input.cwd,
    workspaceId: input.workspaceId,
    live,
    pending: false,
    message,
    groupId: input.groupId || id,
    profileId,
    shell,
    pid,
  };
  const placeholder = input.groupId
    ? undefined
    : terminalSessions.value.find((item) => item.pending && item.workspaceId === input.workspaceId);
  if (placeholder) {
    terminalSessions.value = terminalSessions.value.map((item) => (item.id === placeholder.id ? created : item));
  } else {
    terminalSessions.value = [...terminalSessions.value, created];
  }
  activeTerminalSessionId.value = created.id;
  return created;
}

export async function openTerminalSession(input: {
  workspaceId: string;
  cwd: string;
  title: string;
  projectId: string;
  profile?: string;
  groupId?: string;
}): Promise<TerminalTab> {
  const created = await openWorkspaceTerminal({
    ...input,
    profile: input.profile,
    groupId: input.groupId,
  });
  return created;
}

export async function killTerminalSession(id: string) {
  const tab = terminalSessions.value.find((item) => item.id === id);
  terminalSessions.value = terminalSessions.value.filter((item) => item.id !== id);
  if (tab?.live) {
    forgetTerminal(id);
    try {
      await backend.terminalClose(id);
    } catch {
      /* 进程已经结束 */
    }
  }
  if (activeTerminalSessionId.value !== id) return;
  activeTerminalSessionId.value = terminalSessions.value.find((item) => !item.pending)?.id
    ?? terminalSessions.value[0]?.id
    ?? "";
}

export function selectTerminalSession(id: string) {
  if (!terminalSessions.value.some((item) => item.id === id)) return;
  activeTerminalSessionId.value = id;
}

export function patchTerminalSession(id: string, patch: Partial<TerminalTab>) {
  const index = terminalSessions.value.findIndex((item) => item.id === id);
  const current = terminalSessions.value[index];
  if (!current) return;
  terminalSessions.value[index] = { ...current, ...patch };
}

export function reorderTerminalSessions(fromId: string, toId: string, after: boolean) {
  const tabs = [...terminalSessions.value];
  const from = tabs.findIndex((item) => item.id === fromId);
  let to = tabs.findIndex((item) => item.id === toId);
  if (from < 0 || to < 0 || from === to) return;
  const [item] = tabs.splice(from, 1);
  if (!item) return;
  if (from < to) to -= 1;
  tabs.splice(after ? to + 1 : to, 0, item);
  terminalSessions.value = tabs;
}
