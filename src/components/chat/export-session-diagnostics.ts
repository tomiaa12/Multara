import { save } from "@tauri-apps/plugin-dialog";
import type { RunEvent } from "../../domain/types";
import { appVersion, exportSessionDiagnostics as loadSessionDiagnostics, saveDiagnosticFile, type SessionDiagnostic } from "../../services/backend";

const CLIENT_TEXT_LIMIT = 4_000;

export function clipText(text: string | null | undefined, limit: number): string | null {
  if (text == null) return null;
  if (text.length <= limit) return text;
  return `${text.slice(0, limit)}\n…[clipped ${text.length - limit} chars]`;
}

/** 只留能判断「还在跑 / 报错 / 停在哪一步」的字段，不带文件快照和整段工具输出。 */
export function slimRunState(event: RunEvent) {
  const item = event.timelineItem;
  return {
    runId: event.runId,
    sessionId: event.sessionId,
    agentId: event.agentId,
    status: event.status,
    error: clipText(event.error, CLIENT_TEXT_LIMIT),
    eventKind: event.eventKind ?? null,
    messageId: event.messageId ?? null,
    elapsedMs: event.elapsedMs,
    filePath: event.filePath ?? null,
    affectedFile: event.affectedFile ?? null,
    workspaceId: event.workspaceId ?? null,
    timelineItem: item
      ? {
          seq: item.seq,
          eventType: item.eventType,
          tool: item.tool,
          status: item.status ?? null,
          createdAt: item.createdAt,
          content: clipText(item.content, 2_000),
        }
      : null,
  };
}

export function diagnosticFileName(title: string, sessionId: string): string {
  const slug = title
    .trim()
    .replace(/[^\p{L}\p{N}._-]+/gu, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
  const short = sessionId.replace(/[^a-zA-Z0-9]/g, "").slice(0, 8) || "chat";
  return `multara-chat-${slug || short}.json`;
}

export function sessionRunStates(runStates: Record<string, RunEvent>, sessionId: string) {
  return Object.values(runStates)
    .filter((event) => event.sessionId === sessionId)
    .map(slimRunState);
}

export async function exportChatDiagnostics(input: {
  sessionId: string;
  title: string;
  sessionProcessing: boolean;
  runStates: Record<string, RunEvent>;
}): Promise<boolean> {
  const recorded: SessionDiagnostic = await loadSessionDiagnostics(input.sessionId);
  const payload = {
    ...recorded,
    client: {
      appVersion: await appVersion(),
      exportedAt: new Date().toISOString(),
      sessionProcessing: input.sessionProcessing,
      runStates: sessionRunStates(input.runStates, input.sessionId),
    },
  };
  const contents = JSON.stringify(payload, null, 2);
  const path = await save({
    title: input.title,
    defaultPath: diagnosticFileName(recorded.session?.title || input.title, input.sessionId),
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (!path) return false;
  await saveDiagnosticFile(path, contents);
  return true;
}
