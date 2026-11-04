import { describe, expect, it } from "vitest";
import type { Agent, Squad } from "../../domain/types";
import { buildChatTargetOptions, groupChatTargetOptions } from "./chat-target-options";

const agents: Agent[] = [
  {
    id: "leader",
    name: "队长",
    avatar: "👑",
    description: "带队",
    instructions: "",
    runtimeId: "rt",
    model: "auto",
    maxConcurrentTasks: 1,
    skillIds: [],
    mcpIds: [],
  },
  {
    id: "member",
    name: "队员",
    avatar: "🛠",
    description: "",
    instructions: "",
    runtimeId: "rt",
    model: "auto",
    maxConcurrentTasks: 1,
    skillIds: [],
    mcpIds: [],
  },
];

const squads: Squad[] = [
  {
    id: "sq1",
    name: "产品小队",
    description: "拆需求",
    instructions: "",
    leaderAgentId: "leader",
    memberAgentIds: ["leader", "member"],
  },
];

describe("buildChatTargetOptions", () => {
  it("小队用小队标识头像，不用队长头像", () => {
    const options = buildChatTargetOptions(agents, squads, (name) => `小队 · ${name}`);
    const squad = options.find((item) => item.value === "squad:sq1");
    expect(squad?.avatar).toBe("队");
    expect(squad?.avatar).not.toBe("👑");
    expect(squad?.label).toBe("小队 · 产品小队");
  });

  it("智能体仍用自身头像", () => {
    const options = buildChatTargetOptions(agents, squads, (name) => name);
    expect(options.find((item) => item.value === "agent:leader")?.avatar).toBe("👑");
  });
});

describe("groupChatTargetOptions", () => {
  it("小队分组去掉「小队 · 」前缀", () => {
    const options = buildChatTargetOptions(agents, squads, (name) => `小队 · ${name}`);
    const groups = groupChatTargetOptions(options, { agents: "智能体", squads: "小队" });
    expect(groups.map((item) => item.label)).toEqual(["智能体", "小队"]);
    expect(groups[1]?.options[0]?.label).toBe("产品小队");
  });
});
