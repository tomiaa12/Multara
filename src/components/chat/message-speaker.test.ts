import { beforeEach, describe, expect, it } from "vitest";
import { applyLanguageSetting, t } from "../../i18n";
import type { Agent, ChatMessage, ChatSession, Squad } from "../../domain/types";
import {
  parseAssignmentSubtask,
  resolveTurnTarget,
  shouldHideAssistantSpeaker,
  speakerForMessage,
  subtaskEmbedForMessage,
} from "./message-speaker";

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

describe("assignment / speaker collapse", () => {
  it("parses subtask title and id from assignment text", () => {
    expect(parseAssignmentSubtask(
      "请处理子任务「实现：交收审驳回后认证审自动驳回」（`39187a5c-aaaa-bbbb-cccc-ddddeeeeffff`），完成后向我汇报。",
    )).toEqual({
      title: "实现：交收审驳回后认证审自动驳回",
      id: "39187a5c-aaaa-bbbb-cccc-ddddeeeeffff",
    });
  });

  it("hides repeated speaker only for consecutive user-facing replies", () => {
    const first = message({ id: "a1", role: "assistant", agentId: "lead" });
    const second = message({ id: "a2", role: "assistant", agentId: "lead" });
    const assignment = message({
      id: "asg",
      role: "assistant",
      agentId: "lead",
      messageKind: "assignment",
      audienceType: "agent",
      audienceId: "member",
    });
    expect(shouldHideAssistantSpeaker(first, second)).toBe(true);
    expect(shouldHideAssistantSpeaker(assignment, second)).toBe(false);
    expect(shouldHideAssistantSpeaker(undefined, second)).toBe(false);
  });

  it("embeds subtask under the first parent reply after assignment", () => {
    const assignment = message({
      id: "asg",
      role: "assistant",
      agentId: "lead",
      messageKind: "assignment",
      audienceType: "agent",
      audienceId: "member",
      content: "请处理子任务「实现：交收审驳回后认证审自动驳回」（`39187a5c-aaaa-bbbb-cccc-ddddeeeeffff`），完成后向我汇报。",
    });
    const first = message({ id: "a1", role: "assistant", agentId: "lead", content: "子任务已建好" });
    const second = message({ id: "a2", role: "assistant", agentId: "lead", content: "汇总结果" });
    const messages = [assignment, first, second];
    expect(subtaskEmbedForMessage(messages, 1)).toEqual({
      title: "实现：交收审驳回后认证审自动驳回",
      id: "39187a5c-aaaa-bbbb-cccc-ddddeeeeffff",
    });
    expect(subtaskEmbedForMessage(messages, 2)).toBeNull();
  });

  it("skips embed when assignee already bridged a reply into the parent timeline", () => {
    const assignment = message({
      id: "asg",
      role: "assistant",
      agentId: "lead",
      messageKind: "assignment",
      audienceType: "agent",
      audienceId: "member",
      content: "请处理子任务「实现：交收审驳回后认证审自动驳回」（`39187a5c-aaaa-bbbb-cccc-ddddeeeeffff`），完成后向我汇报。",
    });
    const note = message({ id: "a1", role: "assistant", agentId: "lead", content: "子任务已建好" });
    const bridge = message({
      id: "bridge:child",
      role: "assistant",
      agentId: "member",
      audienceType: "agent",
      audienceId: "lead",
      content: "补丁已应用，测试通过。",
    });
    const summary = message({ id: "a2", role: "assistant", agentId: "lead", content: "汇总结果" });
    expect(subtaskEmbedForMessage([assignment, note], 1)).not.toBeNull();
    expect(subtaskEmbedForMessage([assignment, note, bridge, summary], 1)).toBeNull();
    expect(subtaskEmbedForMessage([assignment, bridge, summary], 2)).toBeNull();
  });

  it("工作流总览消息没有 agentId 时用会话负责人的名字和头像", () => {
    const note = message({ id: "w1", role: "assistant", content: "工作流开始运行" });
    const session = {
      id: "wf-run:run-1",
      targetType: "agent",
      targetId: "lead",
    } as ChatSession;
    const speaker = speakerForMessage(note, agents, squads, null, { session });
    expect(speaker?.name).toBe("队长甲");
    expect(speaker?.avatar).toBe("L");
  });

  it("任务评论显示写下它的智能体，没有作者时标成评论", () => {
    const authored = message({ id: "c1", role: "system", agentId: "member", content: "结论" });
    const anonymous = message({ id: "c2", role: "system", content: "结论" });
    expect(speakerForMessage(authored, agents, squads, "lead")?.name).toBe("队员乙");
    expect(speakerForMessage(authored, agents, squads, "lead")?.avatar).toBe("M");
    expect(speakerForMessage(anonymous, agents, squads, "lead")?.name).toBe(t("chat.commentAuthor"));
  });
});
