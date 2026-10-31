import type { ChatMessage, PlanActionKind, RunMode, RunStatus, TimelineItem } from "../../domain/types";
import { asRecord, isPlanArtifactItem, resolvedInput, stringField } from "./timeline-display";

export type PlanStatus =
  | "awaiting_approval"
  | "approved"
  | "executing"
  | "executed"
  | "failed"
  | "rejected"
  | "superseded";

export interface PlanStep {
  id: string;
  text: string;
  status: string;
}

export interface PlanArtifact {
  planId: string;
  revision: number;
  title: string;
  summary: string;
  markdown: string;
  steps: PlanStep[];
  risks: string[];
  questions: string[];
  structured: boolean;
  changedFiles: string[];
}

export interface PlanEntry {
  key: string;
  artifact: PlanArtifact;
  /** 计划所在助手消息 */
  messageId: string;
  runId: string | null;
  status: PlanStatus;
}

export interface PlanStates {
  byKey: Map<string, PlanEntry>;
  /** 会话里最新一个 awaiting_approval 的计划；只有它的按钮可用 */
  latestAwaitingKey: string | null;
}

type PlanActionRecord = PlanActionKind | "supersede";

const SUPERSEDING_ACTIONS = new Set<PlanActionRecord>(["refine", "supersede"]);
const LIVE_RUN_STATUSES = new Set<RunStatus>(["queued", "running"]);
const BROKEN_RUN_STATUSES = new Set<RunStatus>(["failed", "stopped"]);

function strings(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string" && Boolean(item.trim()))
    : [];
}

function steps(value: unknown): PlanStep[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((entry, index) => {
    const record = asRecord(entry);
    const text = stringField(record, ["text", "content", "title"]);
    if (!text) return [];
    return [{ id: stringField(record, ["id"]) || String(index), text, status: stringField(record, ["status"]) || "pending" }];
  });
}

export function planKey(planId: string, revision: number): string {
  return `${planId}#${revision}`;
}

export function planArtifact(item: TimelineItem): PlanArtifact | null {
  if (!isPlanArtifactItem(item)) return null;
  const input = resolvedInput(item);
  const planId = stringField(input, ["planId"]);
  const revision = Number(input.revision);
  if (!planId || !Number.isFinite(revision)) return null;
  return {
    planId,
    revision,
    title: stringField(input, ["title"]),
    summary: stringField(input, ["summary"]),
    markdown: String(input.markdown),
    steps: steps(input.steps),
    risks: strings(input.risks),
    questions: strings(input.questions),
    structured: input.structured !== false,
    changedFiles: strings(input.changedFiles),
  };
}

function planActionOf(item: TimelineItem) {
  if (item.eventType !== "plan_action") return null;
  const input = resolvedInput(item);
  const planId = stringField(input, ["planId"]);
  const revision = Number(input.revision);
  const action = stringField(input, ["action"]) as PlanActionRecord;
  if (!planId || !Number.isFinite(revision) || !action) return null;
  return { key: planKey(planId, revision), action };
}

function sourcePlanRef(message: ChatMessage) {
  const meta = message.sourceMeta;
  const planId = typeof meta?.planId === "string" ? meta.planId : "";
  const revision = Number(meta?.revision);
  return { action: meta?.planAction, key: planId && Number.isFinite(revision) ? planKey(planId, revision) : "" };
}

function approvedOutcome(
  messages: ChatMessage[],
  key: string,
  runStatus: (message: ChatMessage) => RunStatus | undefined,
): PlanStatus {
  const start = messages.findIndex((message) => {
    const ref = sourcePlanRef(message);
    return message.role === "user" && ref.action === "approve" && ref.key === key;
  });
  if (start < 0) return "approved";
  const replies: ChatMessage[] = [];
  for (const message of messages.slice(start + 1)) {
    if (message.role === "user") break;
    if (message.role === "assistant") replies.push(message);
  }
  if (!replies.length) return "approved";
  const statuses = replies.map((message) => runStatus(message));
  if (statuses.some((status) => status && LIVE_RUN_STATUSES.has(status))) return "executing";
  const broken = replies.some((message, index) => {
    const status = statuses[index];
    return status ? BROKEN_RUN_STATUSES.has(status) : Boolean(message.failureReason);
  });
  return broken ? "failed" : "executed";
}

function statusFor(
  action: PlanActionRecord | undefined,
  superseded: boolean,
  approved: () => PlanStatus,
): PlanStatus {
  if (action === "approve") return approved();
  if (action === "reject") return "rejected";
  if ((action && SUPERSEDING_ACTIONS.has(action)) || superseded) return "superseded";
  return "awaiting_approval";
}

/** 契约第 5 节：计划状态只从事件推导，与后端规则一致 */
export function derivePlanStates(
  messages: ChatMessage[],
  runStatus: (message: ChatMessage) => RunStatus | undefined = () => undefined,
): PlanStates {
  const found: Array<Omit<PlanEntry, "status">> = [];
  const actions = new Map<string, PlanActionRecord>();
  const latestRevision = new Map<string, number>();
  for (const message of messages) {
    for (const item of message.timeline ?? []) {
      const action = planActionOf(item);
      if (action && !actions.has(action.key)) actions.set(action.key, action.action);
      const artifact = planArtifact(item);
      if (!artifact) continue;
      found.push({ key: planKey(artifact.planId, artifact.revision), artifact, messageId: message.id, runId: message.runId });
      latestRevision.set(artifact.planId, Math.max(latestRevision.get(artifact.planId) ?? artifact.revision, artifact.revision));
    }
  }
  const byKey = new Map<string, PlanEntry>();
  let latestAwaitingKey: string | null = null;
  for (const entry of found) {
    const superseded = (latestRevision.get(entry.artifact.planId) ?? entry.artifact.revision) > entry.artifact.revision;
    const status = statusFor(actions.get(entry.key), superseded, () => approvedOutcome(messages, entry.key, runStatus));
    byKey.set(entry.key, { ...entry, status });
    if (status === "awaiting_approval") latestAwaitingKey = entry.key;
  }
  return { byKey, latestAwaitingKey };
}

export function plansInMessage(states: PlanStates, messageId: string): PlanEntry[] {
  return [...states.byKey.values()].filter((entry) => entry.messageId === messageId);
}

export function latestAwaitingPlan(states: PlanStates): PlanEntry | null {
  return states.latestAwaitingKey ? states.byKey.get(states.latestAwaitingKey) ?? null : null;
}

/** 进行中的轮次是不是规划轮：流式消息还没有 runMode，退回最后一条用户消息请求的模式 */
function runningPlanRound(messages: ChatMessage[], runStatus: (message: ChatMessage) => RunStatus | undefined) {
  const running = [...messages].reverse().find((message) => {
    const status = runStatus(message);
    return message.role === "assistant" && status && LIVE_RUN_STATUSES.has(status);
  });
  if (!running) return false;
  const lastUser = [...messages].reverse().find((message) => message.role === "user");
  return (running.runMode ?? lastUser?.requestedMode) === "plan";
}

/** 输入框上方的提示行文案 key；没有需要提醒的情况返回空串 */
export function composerPlanHintKey(
  messages: ChatMessage[],
  states: PlanStates,
  mode: RunMode,
  runStatus: (message: ChatMessage) => RunStatus | undefined,
): string {
  if (runningPlanRound(messages, runStatus)) return "chat.plan.hint.steer";
  if (!states.latestAwaitingKey) return "";
  return mode === "plan" ? "chat.plan.hint.refine" : "chat.plan.hint.supersede";
}
