import type { ChatMessage, RunEvent } from "../domain/types";

export interface LiveRun {
  runId: string;
  sessionId: string;
  agentId: string;
  status: "queued" | "running";
}

/** 给 liveWorkParts 的展示态：与对话内工作行共用，避免列表「排队中」/ 对话「启动中」不一致 */
export type LiveWorkStatus = "queued" | "running" | "sending" | null;

function isLiveEvent(
  event: Pick<RunEvent, "status" | "runId">,
): event is Pick<RunEvent, "runId"> & { status: "queued" | "running" } {
  return (event.status === "queued" || event.status === "running")
    && !event.runId.startsWith("pending:");
}

function sessionHasRunning(
  runStates: Record<string, RunEvent>,
  sessionId: string,
  exceptRunId?: string,
) {
  return Object.values(runStates).some((event) =>
    event.sessionId === sessionId
    && event.status === "running"
    && !event.runId.startsWith("pending:")
    && event.runId !== exceptRunId,
  );
}

/**
 * 会话级展示态（对话内工作行）。
 * DB 的 `queued` 在无人 running 时只是调度占位 / CLI 尚未拉起，应显示「启动中」；
 * 仅当同会话已有 running、自己还在 queued 时才是真「排队中」。
 */
export function resolveSessionLiveWorkStatus(options: {
  runStates: Record<string, RunEvent>;
  sessionId: string | null | undefined;
  processing?: boolean;
}): LiveWorkStatus {
  const sessionId = options.sessionId;
  if (!sessionId) return options.processing ? "sending" : null;
  const live = Object.values(options.runStates).filter(
    (event) => event.sessionId === sessionId && isLiveEvent(event),
  );
  if (live.some((event) => event.status === "running")) return "running";
  if (live.some((event) => event.status === "queued")) {
    // 仅有 queued：与 sending 一样走启动中（liveStageKey）
    return "sending";
  }
  if (options.processing) return "sending";
  return null;
}

/**
 * 单条 run 在需求列表等处的展示态。
 * 同会话已有别的 running 时 queued 才是「排队中」，否则是启动占位。
 */
export function resolveRunLiveWorkStatus(
  run: Pick<LiveRun, "status" | "sessionId" | "runId">,
  runStates: Record<string, RunEvent>,
): Exclude<LiveWorkStatus, null> {
  if (run.status === "running") return "running";
  return sessionHasRunning(runStates, run.sessionId, run.runId) ? "queued" : "sending";
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
    if (!isLiveEvent(event)) continue;
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
