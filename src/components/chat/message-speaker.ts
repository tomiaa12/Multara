import type { Agent, ChatMessage, ChatSession, Squad } from "../../domain/types";

export type SpeakerRole = "lead" | "member" | "solo";

export interface SpeakerInfo {
  agentId: string;
  name: string;
  avatar: string;
  roleLabel: "队长" | "队员" | "智能体";
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
  roleLabel: "队长" | "队员" | "智能体";
}

function roleLabel(kind: SpeakerRole): MentionInfo["roleLabel"] {
  if (kind === "lead") return "队长";
  if (kind === "member") return "队员";
  return "智能体";
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
  return !text
    || text.startsWith("请按刚才的拆分")
    || text.startsWith("请按用户这句话处理");
}

export function assignmentVisibleText(content: string): string {
  const text = content.trim();
  if (!text) return "请按刚才的拆分处理你负责的部分，完成后向我汇报。";
  const first = text.split(/\n/)[0]?.trim() || text;
  if (first.startsWith("请按刚才的拆分") || first.startsWith("请按用户这句话处理")) return first;
  return "请按刚才的拆分处理你负责的部分，完成后向我汇报。";
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
    name: agent?.name || "智能体",
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
      label: squad ? `小队 · ${squad.name}` : "小队",
      avatar: leader?.avatar || "◉",
    };
  }
  const agent = agentById(agents, targetId);
  return {
    value: `agent:${targetId}`,
    label: agent?.name || "智能体",
    avatar: agent?.avatar || "M",
  };
}
