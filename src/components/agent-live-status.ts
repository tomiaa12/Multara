import type { Agent, ChatMessage, RunEvent, Runtime } from "../domain/types";

export interface LiveRun {
  runId: string;
  sessionId: string;
  agentId: string;
  status: "queued" | "running";
}

/** 排队名额：只看智能体绑定的运行时，按该运行时上全局正在跑的数量计。 */
export interface LiveCapacity {
  agents: Array<Pick<Agent, "id" | "runtimeId" | "maxConcurrentTasks">>;
  runtimes: Array<Pick<Runtime, "id" | "maxConcurrent">>;
}

const EMPTY_CAPACITY: LiveCapacity = { agents: [], runtimes: [] };

export function liveCapacity(data: {
  agents?: LiveCapacity["agents"];
  runtimes?: LiveCapacity["runtimes"];
} | null | undefined): LiveCapacity {
  return {
    agents: data?.agents ?? [],
    runtimes: data?.runtimes ?? [],
  };
}

/** 给 liveWorkParts 的展示态：与对话内工作行共用，避免列表「排队中」/ 对话「启动中」不一致 */
export type LiveWorkStatus = "queued" | "running" | "sending" | null;

function isLiveEvent(
  event: Pick<RunEvent, "status" | "runId">,
): event is Pick<RunEvent, "runId"> & { status: "queued" | "running" } {
  return (event.status === "queued" || event.status === "running")
    && !event.runId.startsWith("pending:");
}

function slotLimit(value: number | undefined): number {
  return value && value > 0 ? value : 1;
}

/**
 * 这条还在 queued，是因为绑定的运行时名额满了，而不是 CLI 还没拉起。
 * 名额只看运行时，跨会话、跨智能体一起数。
 */
export function isQueuedForCapacity(
  run: Pick<RunEvent, "status" | "agentId" | "runId">,
  runStates: Record<string, RunEvent>,
  capacity: LiveCapacity = EMPTY_CAPACITY,
): boolean {
  if (run.status !== "queued" || run.runId.startsWith("pending:")) return false;
  const agent = capacity.agents.find((item) => item.id === run.agentId);
  const runtimeId = agent?.runtimeId ?? "";
  if (!runtimeId) return false;
  const runtimeLimit = slotLimit(
    capacity.runtimes.find((item) => item.id === runtimeId)?.maxConcurrent,
  );
  let runtimeRunning = 0;
  for (const event of Object.values(runStates)) {
    if (event.status !== "running" || event.runId.startsWith("pending:") || !event.agentId) continue;
    const eventRuntime = capacity.agents.find((item) => item.id === event.agentId)?.runtimeId;
    if (eventRuntime === runtimeId) runtimeRunning += 1;
  }
  return runtimeRunning >= runtimeLimit;
}

/**
 * 会话级展示态（对话内工作行）。
 * 本会话已有 running 时跟那条走。只剩 queued 时：绑定运行时的全局名额满了才是「排队中」，
 * 名额还在则是「启动中」。
 */
export function resolveSessionLiveWorkStatus(options: {
  runStates: Record<string, RunEvent>;
  sessionId: string | null | undefined;
  processing?: boolean;
  capacity?: LiveCapacity;
}): LiveWorkStatus {
  const sessionId = options.sessionId;
  if (!sessionId) return options.processing ? "sending" : null;
  const capacity = options.capacity ?? EMPTY_CAPACITY;
  const live = Object.values(options.runStates).filter(
    (event) => event.sessionId === sessionId && isLiveEvent(event),
  );
  if (live.some((event) => event.status === "running")) return "running";
  const queued = live.filter((event) => event.status === "queued");
  if (queued.length) {
    const waiting = queued.every((event) => isQueuedForCapacity(event, options.runStates, capacity));
    return waiting ? "queued" : "sending";
  }
  if (options.processing) return "sending";
  return null;
}

/**
 * 单条 run 在需求列表等处的展示态。
 * queued 且绑定运行时的全局名额已满 → 排队中；否则是启动占位。
 */
export function resolveRunLiveWorkStatus(
  run: Pick<LiveRun, "status" | "runId" | "agentId">,
  runStates: Record<string, RunEvent>,
  capacity: LiveCapacity = EMPTY_CAPACITY,
): Exclude<LiveWorkStatus, null> {
  if (run.status === "running") return "running";
  return isQueuedForCapacity(
    { status: "queued", agentId: run.agentId, runId: run.runId },
    runStates,
    capacity,
  ) ? "queued" : "sending";
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
