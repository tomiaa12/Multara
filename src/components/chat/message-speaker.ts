import type { Agent, ChatMessage, ChatSession, Squad, Task } from "../../domain/types";
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

export function isLeaderAssignment(message: ChatMessage): boolean {
  return message.role === "assistant"
    && message.messageKind === "assignment"
    && message.audienceType === "agent"
    && Boolean(message.agentId)
    && Boolean(message.audienceId);
}

/** 派活消息的可见文字：整段指令，去掉残留的 mention 链接标记。 */
export function assignmentVisibleText(content: string): string {
  const text = content.trim();
  if (!text) return t("chat.assignmentFallback");
  return text.replace(/\[@?([^\]]+)\]\(mention:\/\/[^)]+\)/g, "@$1").trim();
}

export interface AssignmentSubtaskRef {
  title: string;
  id: string;
  /** 样式预览等调试场景：不读 store，直接喂嵌套卡内容。 */
  preview?: {
    status?: Task["status"];
    agent?: { id: string; name: string; avatar: string };
    reply?: string;
    pending?: boolean;
  };
}

/** 从派活正文里抽出子任务标题与 id（「标题」（`id`））。 */
export function parseAssignmentSubtask(content: string): AssignmentSubtaskRef | null {
  const text = content.trim();
  if (!text) return null;
  const quoted = text.match(/[「"](.+?)[」"]\s*[（(]`([0-9a-fA-F-]{8,})`[）)]/);
  if (quoted?.[1] && quoted[2]) return { title: quoted[1], id: quoted[2] };
  return null;
}

/** 同一 exchange 里连续两条对用户的同智能体回复时，后一条可藏头像行。 */
export function shouldHideAssistantSpeaker(previous: ChatMessage | undefined, current: ChatMessage) {
  if (!previous || previous.role !== "assistant" || current.role !== "assistant") return false;
  if (current.messageKind === "assignment" || previous.messageKind === "assignment") return false;
  if (current.messageKind === "channel_outbound" || previous.messageKind === "channel_outbound") return false;
  if (previous.audienceType === "agent" || current.audienceType === "agent") return false;
  if (!previous.agentId || previous.agentId !== current.agentId) return false;
  return true;
}

/**
 * 派活后第一条对用户的助手回复：在其下方嵌子任务回复卡片。
 * 后续汇总回复不再重复嵌套。
 * 若时间线里已有被指派方 @ 回来的桥接消息，则不再嵌套（避免双份）。
 */
export function subtaskEmbedForMessage(
  messages: ChatMessage[],
  index: number,
): AssignmentSubtaskRef | null {
  const current = messages[index];
  if (!current || current.role !== "assistant") return null;
  if (current.messageKind === "assignment" || current.audienceType === "agent") return null;

  for (let cursor = index - 1; cursor >= 0; cursor -= 1) {
    const previous = messages[cursor];
    if (!previous) continue;
    if (previous.role === "user") break;
    if (!isLeaderAssignment(previous)) continue;
    const afterAssignment: ChatMessage[] = [];
    for (let scan = cursor + 1; scan < messages.length; scan += 1) {
      const item = messages[scan];
      if (!item || item.role === "user") break;
      afterAssignment.push(item);
    }
    const assigneeId = previous.audienceId;
    // 父会话已有 B@A 桥接时不再嵌套，避免与真实对话双份
    if (assigneeId && afterAssignment.some((item) =>
      item.role === "assistant"
      && item.audienceType === "agent"
      && item.agentId === assigneeId
      && item.messageKind !== "assignment"
    )) {
      return null;
    }
    const gap = messages.slice(cursor + 1, index);
    if (gap.some((item) =>
      item.role === "assistant"
      && item.messageKind !== "assignment"
      && item.audienceType !== "agent"
    )) {
      return null;
    }
    return parseAssignmentSubtask(previous.content);
  }
  return null;
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

/** 已经从飞书 / Lark 发进来的用户消息，收件箱里不再当输入框改。 */
export function sentFromChannel(message: ChatMessage) {
  return message.role === "user" && (message.sourceType === "feishu" || message.sourceType === "lark");
}
