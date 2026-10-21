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

function agentById(agents: Agent[], id: string | null | undefined) {
  if (!id) return undefined;
  return agents.find((item) => item.id === id);
}

function squadById(squads: Squad[], id: string | null | undefined) {
  if (!id) return undefined;
  return squads.find((item) => item.id === id);
}

function roleInSquad(agentId: string, squads: Squad[]): SpeakerRole {
  const squad = squads.find((item) =>
    item.leaderAgentId === agentId || item.memberAgentIds.includes(agentId),
  );
  if (!squad) return "solo";
  return squad.leaderAgentId === agentId ? "lead" : "member";
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

export function isLeaderAssignment(message: ChatMessage, squads: Squad[]): boolean {
  if (message.role !== "assistant" || !message.agentId || !message.audienceId) return false;
  if (message.audienceType !== "agent") return false;
  if (message.messageKind === "assignment") return true;
  if (roleInSquad(message.agentId, squads) !== "lead") return false;
  if (roleInSquad(message.audienceId, squads) !== "member") return false;
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
): SpeakerInfo | null {
  if (message.role !== "assistant") return null;
  const agentId = message.agentId || fallbackAgentId || "";
  const agent = agentById(agents, agentId);
  const kind = roleInSquad(agentId, squads);
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
    audienceRole: audience ? roleLabel(roleInSquad(audience.id, squads)) : null,
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
