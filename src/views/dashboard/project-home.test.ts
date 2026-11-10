import { describe, expect, it } from "vitest";
import type { BootstrapData, Project, RunEvent } from "../../domain/types";
import { buildHomeProject } from "./project-home";

function baseData(overrides: Partial<BootstrapData> = {}): BootstrapData {
  return {
    projects: [],
    workspaces: [],
    runtimes: [],
    agents: [
      {
        id: "miku",
        name: "Miku",
        avatar: "🎤",
        description: "",
        instructions: "",
        runtimeId: "rt",
        model: "auto",
        maxConcurrentTasks: 1,
        skillIds: [],
        mcpIds: [],
      },
      {
        id: "reviewer",
        name: "审稿",
        avatar: "👀",
        description: "",
        instructions: "",
        runtimeId: "rt",
        model: "auto",
        maxConcurrentTasks: 1,
        skillIds: [],
        mcpIds: [],
      },
    ],
    squads: [],
    sessions: [],
    tasks: [],
    resources: [],
    stats: {
      completedToday: 0,
      running: 0,
      queued: 0,
      failedToday: 0,
    },
    automations: [],
    inbox: [],
    settings: {},
    ...overrides,
  };
}

const project: Project = {
  id: "p1",
  name: "需求 A",
  note: "",
  workspaceCount: 1,
  createdAt: "2026-01-01T00:00:00Z",
  openedAt: "2026-01-01T00:00:00Z",
};

describe("buildHomeProject workers", () => {
  it("新对话只选了目标时不展示智能体", () => {
    const data = baseData({
      sessions: [
        {
          id: "s1",
          projectId: "p1",
          title: "新对话",
          targetType: "agent",
          targetId: "reviewer",
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-02T00:00:00Z",
        },
      ],
      workspaces: [
        {
          id: "w1",
          projectId: "p1",
          name: "repo",
          kind: "folder",
          sourcePath: "/tmp/repo",
          workPath: "/tmp/repo",
          branch: null,
          isolationMode: "direct",
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
    });
    expect(buildHomeProject(project, data, {}).workers).toEqual([]);
  });

  it("空闲时展示回复过 / 执行过任务的智能体", () => {
    const data = baseData({
      sessions: [
        {
          id: "s1",
          projectId: "p1",
          title: "聊审稿",
          targetType: "agent",
          targetId: "reviewer",
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-02T00:00:00Z",
        },
        {
          id: "s2",
          projectId: "p1",
          title: "聊 Miku",
          targetType: "agent",
          targetId: "miku",
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-01T00:00:00Z",
        },
      ],
      tasks: [
        {
          id: "t1",
          sessionId: "s1",
          title: "审稿",
          description: "",
          agentId: "reviewer",
          status: "done",
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-02T00:00:00Z",
        },
      ],
      workspaces: [
        {
          id: "w1",
          projectId: "p1",
          name: "repo",
          kind: "folder",
          sourcePath: "/tmp/repo",
          workPath: "/tmp/repo",
          branch: null,
          isolationMode: "direct",
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
    });
    const finished: Record<string, RunEvent> = {
      r1: {
        runId: "r1",
        sessionId: "s2",
        agentId: "miku",
        status: "completed",
        chunk: null,
        error: null,
        timelineItem: null,
        elapsedMs: 1000,
      },
    };
    const home = buildHomeProject(project, data, finished);
    expect(home.workers.map((item) => item.key)).toEqual([
      "agent:miku",
      "agent:reviewer",
    ]);
    expect(home.workers.find((item) => item.key === "agent:reviewer")?.image).toBe("👀");
  });

  it("有进行中时只展示进行中的智能体", () => {
    const data = baseData({
      sessions: [
        {
          id: "s1",
          projectId: "p1",
          title: "聊审稿",
          targetType: "agent",
          targetId: "reviewer",
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-02T00:00:00Z",
        },
        {
          id: "s2",
          projectId: "p1",
          title: "聊 Miku",
          targetType: "agent",
          targetId: "miku",
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-01T00:00:00Z",
        },
      ],
      workspaces: [
        {
          id: "w1",
          projectId: "p1",
          name: "repo",
          kind: "folder",
          sourcePath: "/tmp/repo",
          workPath: "/tmp/repo",
          branch: null,
          isolationMode: "direct",
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
    });
    const runs: Record<string, RunEvent> = {
      r1: {
        runId: "r1",
        sessionId: "s2",
        agentId: "miku",
        status: "running",
        chunk: null,
        error: null,
        timelineItem: null,
        elapsedMs: null,
      },
    };
    const home = buildHomeProject(project, data, runs);
    expect(home.workers.map((item) => item.key)).toEqual(["agent:miku"]);
    expect(home.filters).toContain("running");
  });
});
