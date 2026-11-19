import { listen } from "@tauri-apps/api/event";
import * as backend from "../services/backend";
import { useAppStore } from "./app";
import {
  ensureWorkspaceTerminal,
  liveTerminalForWorkspace,
  openTerminalSession,
  selectTerminalSession,
  terminalSessions,
} from "./terminal-sessions";

type TerminalCtlPayload = {
  requestId: string;
  sessionId?: string;
  method: "open" | "run" | "write" | "focus";
  workspaceId?: string;
  projectId?: string;
  cwd?: string;
  title?: string;
  command?: string | null;
  text?: string;
  id?: string;
  newTab?: boolean;
};

type TerminalCtlResult =
  | { ok: true; value: { id: string; cwd?: string; title?: string } }
  | { ok: false; reason: string; message?: string };

let started = false;

/** 监听控制通道发来的 terminal-ctl，在集成终端里开面板 / 跑命令。 */
export function startTerminalCtlWatch() {
  if (started || !("__TAURI_INTERNALS__" in window)) return;
  started = true;
  void listen<TerminalCtlPayload>("terminal-ctl", async (event) => {
    const payload = event.payload;
    if (!payload?.requestId) return;
    const result = await handleTerminalCtl(payload);
    await backend.resolveUiOutbound(payload.requestId, result);
  });
}

async function handleTerminalCtl(payload: TerminalCtlPayload): Promise<TerminalCtlResult> {
  try {
    if (payload.method === "write") {
      const id = payload.id?.trim();
      if (!id) return { ok: false, reason: "not_found", message: "missing id" };
      if (!terminalSessions.value.some((item) => item.id === id && item.live)) {
        return { ok: false, reason: "not_found" };
      }
      await backend.terminalWrite(id, payload.text ?? "");
      selectTerminalSession(id);
      useAppStore().revealTerminal(undefined, id);
      return { ok: true, value: { id } };
    }

    if (payload.method === "focus") {
      const id = payload.id?.trim();
      if (id) {
        if (!terminalSessions.value.some((item) => item.id === id)) {
          return { ok: false, reason: "not_found" };
        }
        selectTerminalSession(id);
        const tab = terminalSessions.value.find((item) => item.id === id);
        useAppStore().revealTerminal(tab?.workspaceId, id);
        return { ok: true, value: { id, cwd: tab?.cwd, title: tab?.title } };
      }
      const opened = await openOrReuse(payload);
      useAppStore().revealTerminal(opened.workspaceId, opened.id);
      return {
        ok: true,
        value: { id: opened.id, cwd: opened.cwd, title: opened.title },
      };
    }

    const opened = await openOrReuse(payload);
    useAppStore().revealTerminal(opened.workspaceId, opened.id);
    const command = payload.command?.trim();
    if (command) {
      // 等 shell 提示符落稳再写入，避免命令被吞进启动输出
      await wait(120);
      const text = command.endsWith("\n") ? command : `${command}\n`;
      await backend.terminalWrite(opened.id, text);
    }
    return {
      ok: true,
      value: { id: opened.id, cwd: opened.cwd, title: opened.title },
    };
  } catch (error) {
    const reason = error && typeof error === "object" && "reason" in error
      ? String((error as { reason?: string }).reason || "failed")
      : "failed";
    return {
      ok: false,
      reason,
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

async function openOrReuse(payload: TerminalCtlPayload) {
  const workspaceId = payload.workspaceId?.trim();
  const cwd = payload.cwd?.trim();
  const title = payload.title?.trim() || "终端";
  const projectId = payload.projectId?.trim() || "";
  if (!workspaceId || !cwd) {
    const error = new Error("no_workspace") as Error & { reason: string };
    error.reason = "no_workspace";
    throw error;
  }
  if (payload.newTab) {
    return openTerminalSession({ workspaceId, cwd, title, projectId });
  }
  const existing = liveTerminalForWorkspace(workspaceId);
  if (existing) {
    selectTerminalSession(existing.id);
    return existing;
  }
  return ensureWorkspaceTerminal({ workspaceId, cwd, title, projectId });
}

function wait(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
}
