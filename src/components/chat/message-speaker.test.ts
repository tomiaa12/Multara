import { beforeEach, describe, expect, it } from "vitest";
import { applyLanguageSetting, t } from "../../i18n";
import type { Agent, ChatMessage, ChatSession, Squad } from "../../domain/types";
import { resolveTurnTarget, speakerForMessage } from "./message-speaker";

const agents: Agent[] = [
  {
    id: "lead",
    name: "队长甲",
    avatar: "L",
    description: "",
    instructions: "",
    runtimeId: "cursor",
    model: "",
    maxConcurrentTasks: 1,
    skillIds: [],
    mcpIds: [],
  },
  {
    id: "member",
    name: "队员乙",
    avatar: "M",
    description: "",
    instructions: "",
    runtimeId: "cursor",
    model: "",
    maxConcurrentTasks: 1,
    skillIds: [],
    mcpIds: [],
  },
];

const squads: Squad[] = [
  {
    id: "sq1",
    name: "产品小队",
    description: "",
    instructions: "",
    leaderAgentId: "lead",
    memberAgentIds: ["member"],
  },
];

function message(partial: Partial<ChatMessage> & Pick<ChatMessage, "id" | "role">): ChatMessage {
  return {
    sessionId: "s1",
    content: "",
    runId: null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: "",
    ...partial,
  };
}

describe("resolveTurnTarget / speaker role", () => {
  beforeEach(() => {
    applyLanguageSetting("zh-CN");
  });
  it("单独对智能体时不显示队长队员，即使该智能体在别的小队里", () => {
    const user = message({
      id: "u1",
      role: "user",
      targetType: "agent",
      targetId: "lead",
      turnId: "t1",
    });
    const assistant = message({
      id: "a1",
      role: "assistant",
      agentId: "lead",
      turnId: "t1",
    });
    const speaker = speakerForMessage(assistant, agents, squads, null, {
      messages: [user, assistant],
    });
    expect(speaker?.roleLabel).toBe(t("chat.role.agent"));
    expect(speaker?.roleKind).toBe("solo");
  });

  it("同一对话里后一轮改成小队时显示队长队员", () => {
    const soloUser = message({
      id: "u1",
      role: "user",
      targetType: "agent",
      targetId: "lead",
      turnId: "t1",
    });
    const soloAssistant = message({
      id: "a1",
      role: "assistant",
      agentId: "lead",
      turnId: "t1",
    });
    const squadUser = message({
      id: "u2",
      role: "user",
      targetType: "squad",
      targetId: "sq1",
      turnId: "t2",
    });
    const leadReply = message({
      id: "a2",
      role: "assistant",
      agentId: "lead",
      turnId: "t2",
    });
    const memberReply = message({
      id: "a3",
      role: "assistant",
      agentId: "member",
      turnId: "t2",
      audienceType: "agent",
      audienceId: "lead",
    });
    const messages = [soloUser, soloAssistant, squadUser, leadReply, memberReply];
    expect(speakerForMessage(soloAssistant, agents, squads, null, { messages })?.roleLabel).toBe(t("chat.role.agent"));
    expect(speakerForMessage(leadReply, agents, squads, null, { messages })?.roleLabel).toBe(t("chat.role.lead"));
    expect(speakerForMessage(memberReply, agents, squads, null, { messages })?.roleLabel).toBe(t("chat.role.member"));
    expect(speakerForMessage(memberReply, agents, squads, null, { messages })?.audienceRole).toBe(t("chat.role.lead"));
  });

  it("助手消息没有 target 时，从同 turn 用户消息解析", () => {
    const user = message({
      id: "u1",
      role: "user",
      targetType: "squad",
      targetId: "sq1",
      turnId: "t1",
    });
    const assistant = message({
      id: "a1",
      role: "assistant",
      agentId: "lead",
      turnId: "t1",
    });
    expect(resolveTurnTarget(assistant, [user, assistant])).toEqual({ type: "squad", id: "sq1" });
  });

  it("直播气泡可回退到当前会话目标", () => {
    const session: ChatSession = {
      id: "s1",
      projectId: null,
      title: "聊",
      targetType: "squad",
      targetId: "sq1",
      createdAt: "",
      updatedAt: "",
    };
    const live = message({ id: "live", role: "assistant", agentId: "member" });
    expect(resolveTurnTarget(live, [], session)).toEqual({ type: "squad", id: "sq1" });
  });
});
