import type { ChatMessage, RunEvent } from "../domain/types";

export interface LiveRun {
  runId: string;
  sessionId: string;
  agentId: string;
  status: "queued" | "running";
}

/**
 * 这些会话里正在排队 / 运行的 run，每个智能体只留一条（排到的 running 优先于 queued）。
 * `pending:` 开头的是前端占位，不算。
 */
export function collectLiveRuns(
  runStates: Record<string, RunEvent>,
  sessionIds: Iterable<string>,
): LiveRun[] {
  const wanted = new Set(sessionIds);
  if (!wanted.size) return [];
  const byAgent = new Map<string, LiveRun>();
  for (const event of Object.values(runStates)) {
    if (!wanted.has(event.sessionId) || !event.agentId) continue;
    if (event.status !== "queued" && event.status !== "running") continue;
    if (event.runId.startsWith("pending:")) continue;
    const previous = byAgent.get(event.agentId);
    if (previous?.status === "running" && event.status === "queued") continue;
    byAgent.set(event.agentId, {
      runId: event.runId,
      sessionId: event.sessionId,
      agentId: event.agentId,
      status: event.status,
    });
  }
  return [...byAgent.values()];
}

/** run 对应的助手消息（store 按 `run:<runId>` 建的那条），里面有实时 timeline。 */
export function liveRunMessage(
  messages: Record<string, ChatMessage[]>,
  run: Pick<LiveRun, "runId" | "sessionId">,
): ChatMessage | undefined {
  return messages[run.sessionId]?.find((message) => message.id === `run:${run.runId}`);
}
