import type { Agent, ChatMessage, ChatSession, Squad } from "../../domain/types";
import { t } from "../../i18n";

export type SpeakerRole = "lead" | "member" | "solo";

export interface SpeakerInfo {
  agentId: string;
  name: string;
  avatar: string;
  roleLabel: string;
  roleKind: SpeakerRole;
  internal: boolean;
  audienceName: string | null;
  audienceRole: string | null;
}

export interface SentTargetInfo {
  value: string;
  label: string;
  avatar: string;
}

/** 这一轮对话发给谁：同一会话里可先对智能体、再对小队。 */
export type TurnTarget = { type: "agent" | "squad"; id: string };

function agentById(agents: Agent[], id: string | null | undefined) {
  if (!id) return undefined;
  return agents.find((item) => item.id === id);
}

function squadById(squads: Squad[], id: string | null | undefined) {
  if (!id) return undefined;
  return squads.find((item) => item.id === id);
}

/**
 * 只在「这一轮目标是该小队」时标队长/队员；单独聊智能体一律「智能体」。
 * 不再看全局有没有进过别的小队。
 */
function roleInSquad(agentId: string, squads: Squad[], squadId: string | null | undefined): SpeakerRole {
  if (!squadId) return "solo";
  const squad = squads.find((item) => item.id === squadId);
  if (!squad) return "solo";
  if (squad.leaderAgentId === agentId) return "lead";
  if (squad.memberAgentIds.includes(agentId)) return "member";
  return "solo";
}

export interface MentionInfo {
  id: string;
  name: string;
  roleLabel: string;
}

function roleLabel(kind: SpeakerRole): string {
  if (kind === "lead") return t("chat.role.lead");
  if (kind === "member") return t("chat.role.member");
  return t("chat.role.agent");
}

/**
 * 解析这条消息所属轮次的发送目标。
 * 优先消息自身 → 同 turn 用户消息 → 时间线上最近一条带目标的用户消息 → 当前会话目标（直播气泡兜底）。
 */
export function resolveTurnTarget(
  message: ChatMessage,
  messages: ChatMessage[],
  session?: ChatSession | null,
): TurnTarget | null {
  if (message.targetType && message.targetId) {
    return { type: message.targetType, id: message.targetId };
  }
  if (message.turnId) {
    const sameTurn = messages.find(
      (item) =>
        item.turnId === message.turnId
        && item.role === "user"
        && item.targetType
        && item.targetId,
    );
    if (sameTurn?.targetType && sameTurn.targetId) {
      return { type: sameTurn.targetType, id: sameTurn.targetId };
    }
  }
  const index = messages.findIndex((item) => item.id === message.id);
  const before = index >= 0 ? messages.slice(0, index) : messages;
  for (let cursor = before.length - 1; cursor >= 0; cursor -= 1) {
    const item = before[cursor];
    if (item?.role === "user" && item.targetType && item.targetId) {
      return { type: item.targetType, id: item.targetId };
    }
  }
  if (session?.targetType && session.targetId) {
    return { type: session.targetType, id: session.targetId };
  }
  return null;
}

/** 没有 messageKind 的旧派活消息只能按当时写死的中文前缀识别。 */
const LEGACY_SQUAD_ASSIGNMENT_PREFIXES = ["请按刚才的拆分", "请按用户这句话处理"];
const LEGACY_ASSIGNMENT_PREFIXES = [...LEGACY_SQUAD_ASSIGNMENT_PREFIXES, "请处理子任务"];

export function isLeaderAssignment(
  message: ChatMessage,
  squads: Squad[],
  options?: { messages?: ChatMessage[]; session?: ChatSession | null },
): boolean {
  if (message.role !== "assistant" || !message.agentId || !message.audienceId) return false;
  if (message.audienceType !== "agent") return false;
  if (message.messageKind === "assignment") return true;
  const target = resolveTurnTarget(message, options?.messages ?? [], options?.session);
  if (target?.type !== "squad") return false;
  if (roleInSquad(message.agentId, squads, target.id) !== "lead") return false;
  if (roleInSquad(message.audienceId, squads, target.id) !== "member") return false;
  const text = message.content.trim();
  return !text || LEGACY_SQUAD_ASSIGNMENT_PREFIXES.some((prefix) => text.startsWith(prefix));
}

export function assignmentVisibleText(content: string, kind?: ChatMessage["messageKind"]): string {
  const text = content.trim();
  if (!text) return t("chat.assignmentFallback");
  const first = text.split(/\n/)[0]?.trim() || text;
  if (kind === "assignment" || LEGACY_ASSIGNMENT_PREFIXES.some((prefix) => first.startsWith(prefix))) {
    return first;
  }
  return t("chat.assignmentFallback");
}

export function speakerForMessage(
  message: ChatMessage,
  agents: Agent[],
  squads: Squad[],
  fallbackAgentId?: string | null,
  options?: { messages?: ChatMessage[]; session?: ChatSession | null },
): SpeakerInfo | null {
  if (message.role !== "assistant") return null;
  const agentId = message.agentId || fallbackAgentId || "";
  const agent = agentById(agents, agentId);
  const target = resolveTurnTarget(message, options?.messages ?? [], options?.session);
  const squadId = target?.type === "squad" ? target.id : null;
  const kind = roleInSquad(agentId, squads, squadId);
  const audience = agentById(agents, message.audienceId);
  const internal = message.audienceType === "agent" && Boolean(message.audienceId);
  return {
    agentId,
    name: agent?.name || t("chat.role.agent"),
    avatar: agent?.avatar || "M",
    roleLabel: roleLabel(kind),
    roleKind: kind,
    internal,
    audienceName: audience?.name ?? null,
    audienceRole: audience ? roleLabel(roleInSquad(audience.id, squads, squadId)) : null,
  };
}

export function sentTargetForMessage(
  message: ChatMessage,
  agents: Agent[],
  squads: Squad[],
  session?: ChatSession | null,
): SentTargetInfo | null {
  const targetType = message.targetType ?? session?.targetType ?? null;
  const targetId = message.targetId ?? session?.targetId ?? null;
  if (!targetType || !targetId) return null;
  if (targetType === "squad") {
    const squad = squadById(squads, targetId);
    const leader = agentById(agents, squad?.leaderAgentId);
    return {
      value: `squad:${targetId}`,
      label: squad ? t("chat.squadLabel", { name: squad.name }) : t("inbox.squadFallback"),
      avatar: leader?.avatar || "◉",
    };
  }
  const agent = agentById(agents, targetId);
  return {
    value: `agent:${targetId}`,
    label: agent?.name || t("inbox.agentFallback"),
    avatar: agent?.avatar || "M",
  };
}
