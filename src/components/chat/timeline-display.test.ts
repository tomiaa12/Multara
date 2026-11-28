// @vitest-environment node
import { afterEach, beforeEach, describe, expect, test, vi, type MockInstance } from "vitest";
import type { TimelineItem } from "../../domain/types";
import { subAgentFailureLine, subAgentStatus, subAgentSummary, subAgentTallyLabel } from "./subagent-display";
import {
  editDiff,
  formatLiveElapsed,
  groupTimeline,
  liveWorkLabel,
  shellTranscript,
  exploreLine,
  shellCardMode,
  shellFailed,
  shellResultText,
  toolCallLabel,
  workFoldLabel,
  type TimelineBlock,
} from "./timeline-display";

function item(
  eventType: TimelineItem["eventType"],
  tool: string | null = null,
  input: Record<string, unknown> | null = null,
): TimelineItem {
  return {
    seq: 1,
    eventType,
    callId: null,
    tool,
    content: eventType === "thinking" ? "先看环境文件" : null,
    input,
    output: null,
    createdAt: "2026-01-01T00:00:00.000Z",
  };
}

describe("toolCallLabel", () => {
  test("gives unnamed tools a verb and does not repeat the english name", () => {
    expect(toolCallLabel(item("tool_use", "GetMcpTools", { server: "cursor", toolName: "TodoWrite" }), "zh-CN")).toBe("查看工具 TodoWrite");
    expect(toolCallLabel(item("tool_use", "GetMcpTools", { pattern: "multara" }), "zh-CN")).toBe("查看工具 multara");
    expect(toolCallLabel(item("tool_use", "Mcp", { name: "lark-project-search_work_items" }), "zh-CN")).toBe("调用 lark-project-search_work_items");
    expect(toolCallLabel(item("tool_use", "mcp__weather-demo__show_weather", { city: "Paris" }), "zh-CN")).toBe("调用 show_weather");
    expect(toolCallLabel(item("tool_use", "Await", { taskId: "594945" }), "zh-CN")).toBe("等待 594945");
    expect(toolCallLabel(item("tool_use", "ReadLints", { paths: ["apps/a/src/main.ts"] }), "zh-CN")).toBe("检查 main.ts");
    expect(toolCallLabel(item("tool_use", "Skill", { skill: "claude-api" }), "zh-CN")).toBe("技能 claude-api");
    expect(toolCallLabel(item("tool_use", "ExitPlanMode", {}), "zh-CN")).toBe("退出规划");
    expect(toolCallLabel(item("tool_use", "SendMessage", { content: "现场已捕获" }), "zh-CN")).toBe("发送 现场已捕获");
    expect(toolCallLabel(item("tool_use", "TaskCreate", { subject: "梳理设计系统并确定原型布局" }), "zh-CN")).toBe("新建任务 梳理设计系统并确定原型布局");
    expect(toolCallLabel(item("tool_use", "TaskList", {}), "zh-CN")).toBe("列出任务");
    expect(toolCallLabel(item("tool_use", "SwitchMode", { targetModeId: "plan" }), "zh-CN")).toBe("切换模式 规划");
    expect(toolCallLabel(item("tool_use", "CreatePlan", { plan: "# Agent 在终端跑服务\n\n正文" }), "zh-CN")).toBe("编写计划 Agent 在终端跑服务");
    expect(toolCallLabel(item("tool_use", "Delete", { path: "tmp/a.ts" }), "zh-CN")).toBe("删除 a.ts");
    expect(toolCallLabel(item("tool_use", "SomethingNew", {}), "zh-CN")).toBe("SomethingNew");
    expect(exploreLine(item("tool_use", "WebSearch", { searchTerm: "chrome rules" }), "zh-CN")).toBe("搜索 chrome rules");
  });
});

describe("liveWorkLabel", () => {
  test("stays on the latest open action and keeps the elapsed clock visible", () => {
    expect(liveWorkLabel([], 3, { status: "sending", language: "zh-CN" })).toBe("启动中 · 3 秒");
    expect(liveWorkLabel([], 3, { status: "running", language: "zh-CN" })).toBe("启动中 · 3 秒");
    expect(liveWorkLabel([], 3, { status: "queued", language: "zh-CN" })).toBe("排队中 · 3 秒");
    expect(liveWorkLabel([], 20, { status: "running", language: "zh-CN", idleSeconds: 20 })).toBe("等待响应 · 20 秒");
    expect(liveWorkLabel([], 200, { status: "running", language: "zh-CN", idleSeconds: 200 })).toBe("长时间无响应 · 3 分 20 秒");
    expect(liveWorkLabel(
      [item("error")],
      10,
      { status: "running", language: "zh-CN", idleSeconds: 10 },
    )).toBe("等待响应 · 10 秒");
    expect(liveWorkLabel(
      [item("error", null, { kind: "retry", attempt: 2, maxRetries: 10 })],
      30,
      { status: "running", language: "zh-CN" },
    )).toBe("请求超时，正在重试（2/10） · 30 秒");
    expect(liveWorkLabel(
      [item("error", null, { kind: "retry" })],
      30,
      { status: "running", language: "zh-CN" },
    )).toBe("请求超时，正在重试 · 30 秒");
    expect(liveWorkLabel(
      [item("error", null, { kind: "retry", attempt: 1, maxRetries: 5 }), item("text")],
      31,
      { status: "running", language: "zh-CN" },
    )).toBe("输入中 · 31 秒");
    expect(liveWorkLabel(
      [item("tool_use", "Edit", { file_path: "a.ts" }), item("error", null, { kind: "retry", attempt: 1, maxRetries: 5 })],
      31,
      { status: "running", language: "zh-CN" },
    )).toBe("请求超时，正在重试（1/5） · 31 秒");
    expect(liveWorkLabel([item("thinking")], 8, { status: "running", language: "zh-CN" })).toBe("思考中… · 8 秒");
    expect(liveWorkLabel([item("text")], 8, { status: "running", language: "zh-CN" })).toBe("输入中 · 8 秒");
    expect(liveWorkLabel(
      [item("tool_use", "Shell", { command: "multara project list" }), item("tool_result", "Shell")],
      12,
      { status: "running", language: "zh-CN" },
    )).toBe("运行命令 multara project list · 12 秒");
    expect(liveWorkLabel(
      [item("tool_use", "Shell", { command: "pnpm test", description: "跑单测" })],
      12,
      { status: "running", language: "zh-CN" },
    )).toBe("运行命令 跑单测 · 12 秒");
    expect(liveWorkLabel([item("tool_use", "Read", { path: ".env.dev" })], 65, { status: "running", language: "zh-CN" })).toBe("读取文件 .env.dev · 1 分 5 秒");
    expect(liveWorkLabel([item("tool_use", "Read", { path: ".env.dev" })], 65, { status: "running", language: "en" })).toBe("Reading files .env.dev · 1m 5s");
    expect(liveWorkLabel(
      [item("tool_use", "StrReplace", { path: "src/styles.css" })],
      4,
      { status: "running", language: "zh-CN" },
    )).toBe("编辑文件 styles.css · 4 秒");
    expect(liveWorkLabel(
      [
        { ...item("tool_use", "StrReplace", { path: "src/ChatComposerField.vue" }), seq: 1, callId: "a" },
        { ...item("tool_use", "StrReplace", { path: "src/ChatComposer.vue" }), seq: 2, callId: "b" },
      ],
      4,
      { status: "running", language: "zh-CN" },
    )).toBe("编辑文件 ChatComposerField.vue · 4 秒\n编辑文件 ChatComposer.vue · 4 秒");
    expect(liveWorkLabel(
      [
        { ...item("tool_use", "StrReplace", { path: "src/a/index.ts" }), seq: 1, callId: "a" },
        { ...item("tool_use", "StrReplace", { path: "src/b/index.ts" }), seq: 2, callId: "b" },
        { ...item("tool_use", "Read", { path: "README.md" }), seq: 3, callId: "c" },
      ],
      4,
      { status: "running", language: "zh-CN" },
    )).toBe("编辑文件 src/a/index.ts · 4 秒\n编辑文件 src/b/index.ts · 4 秒");
    expect(liveWorkLabel(
      [item("tool_use", "Task", { description: "查代码" })],
      2,
      { status: "running", language: "zh-CN" },
    )).toBe("调用子代理 查代码 · 2 秒");
    expect(liveWorkLabel(
      [item("tool_use", "Delete", { path: "tmp/a.ts" })],
      2,
      { status: "running", language: "zh-CN" },
    )).toBe("删除文件 a.ts · 2 秒");
    expect(liveWorkLabel(
      [item("tool_use", "TodoWrite", {
        todos: [
          { id: "1", content: "读文件", status: "completed" },
          { id: "2", content: "改代码", status: "in_progress" },
        ],
      })],
      2,
      { status: "running", language: "zh-CN" },
    )).toBe("更新计划 改代码 · 2 秒");
    expect(liveWorkLabel(
      [item("tool_use", "mcp_chrome-browser-plugin_browser_navigate", { url: "https://example.com" })],
      2,
      { status: "running", language: "zh-CN" },
    )).toBe("调用 MCP browser_navigate https://example.com · 2 秒");
    expect(liveWorkLabel(
      [item("tool_use", "GenerateImage", { prompt: "一只橘猫坐在窗台" })],
      2,
      { status: "running", language: "zh-CN" },
    )).toBe("生成图片 一只橘猫坐在窗台 · 2 秒");
    expect(liveWorkLabel(
      [item("tool_use", "AskQuestion", { title: "选分支策略" })],
      2,
      { status: "running", language: "zh-CN" },
    )).toBe("提问 选分支策略 · 2 秒");
  });
});

describe("groupTimeline", () => {
  test("keeps tool errors visible when the run itself did not fail", () => {
    const blocks = groupTimeline([
      { ...item("tool_use", "Read", { path: "a.ts" }), seq: 1, callId: "1" },
      { ...item("error"), seq: 2, content: "读取超时" },
    ]);
    expect(blocks.map((block) => block.type)).toEqual(["explore", "error"]);
  });

  test("hides runtime retry notices but keeps ordinary errors", () => {
    const retry = groupTimeline([
      { ...item("error", null, { kind: "retry", attempt: 1, maxRetries: 10 }), seq: 1, content: "Request timed out, retrying (1/10)" },
    ]);
    expect(retry).toEqual([]);
    const ordinary = groupTimeline([
      { ...item("error"), seq: 1, content: "读取超时" },
    ]);
    expect(ordinary.map((block) => block.type)).toEqual(["error"]);
  });

  test("drops unparsed tool-input fragments instead of rendering empty Tool rows", () => {
    const fragment = (seq: number) => ({
      ...item("tool_use", null, { __unparsedToolInput: { len: 3, raw: "pwd" } }),
      seq,
      callId: "call_1",
    });
    const blocks = groupTimeline([fragment(1), fragment(2), fragment(3)]);
    expect(blocks).toEqual([]);
  });

  test("keeps thinking inside the surrounding explore group", () => {
    const blocks = groupTimeline([
      { ...item("thinking"), seq: 1, content: "先看代码" },
      { ...item("tool_use", "Read", { path: "internal/job/service/client/client.go" }), seq: 2, callId: "2" },
      { ...item("thinking"), seq: 3, content: "这里要删二次写入" },
      { ...item("tool_use", "Grep", { pattern: "proofOfPersonInfo" }), seq: 4, callId: "4" },
      { ...item("thinking"), seq: 5, content: "再确认一处" },
    ]);
    expect(blocks.map((block) => block.type)).toEqual(["explore"]);
    const group = blocks[0];
    if (group?.type !== "explore") return;
    expect(group.items.map((entry) => entry.kind)).toEqual(["thinking", "tool", "thinking", "tool", "thinking"]);
  });

  test("nests child tool calls under the parent agent call", () => {
    const blocks = groupTimeline([
      { ...item("tool_use", "Task", { description: "查代码" }), seq: 1, callId: "parent" },
      { ...item("tool_use", "Read", { path: "a.ts" }), seq: 2, callId: "child", parentCallId: "parent" },
      { ...item("tool_result", "Read"), seq: 3, callId: "child", parentCallId: "parent", output: "ok" },
    ]);
    expect(blocks.map((block) => block.type)).toEqual(["subagent"]);
    expect(childrenOf(blocks[0]).map((child) => child.type)).toEqual(["explore"]);
  });

  test("turns TodoWrite into a pinned plan", () => {
    const blocks = groupTimeline([
      {
        ...item("tool_use", "TodoWrite", {
          todos: [
            { id: "1", content: "读文件", status: "completed" },
            { id: "2", content: "改代码", status: "in_progress" },
          ],
        }),
        seq: 1,
        callId: "todo",
      },
    ]);
    expect(blocks[0]).toMatchObject({ type: "plan" });
  });
});

describe("editDiff", () => {
  test("does not parse truncated Cursor Edit JSON as a unified diff", () => {
    const truncated = [
      '{"success":{"path":"/tmp/zh-HK.json","linesAdded":1,"linesRemoved":0,"diffString":"--- a/zh-HK.json\\n+++ b/zh-HK.json\\n@@ -1,3 +1,4 @@\\n {\\n',
      '  \\"missingInfoMessage\\": \\"未獲取「智方便」的賬戶數據',
      "\n…（内容已截断）",
    ].join("");
    const lines = editDiff({
      use: { ...item("tool_use", "Edit", { path: "/tmp/zh-HK.json" }), callId: "e1" },
      result: { ...item("tool_result", "Edit"), callId: "e1", output: truncated, status: "ok" },
    });
    expect(lines).toEqual([]);
  });

  test("still reads a valid Cursor Edit diffString", () => {
    const output = JSON.stringify({
      success: {
        path: "/tmp/a.ts",
        linesAdded: 1,
        linesRemoved: 0,
        diffString: "--- a/a.ts\n+++ b/a.ts\n@@ -1,2 +1,3 @@\n keep\n+added\n end\n",
      },
    });
    const lines = editDiff({
      use: { ...item("tool_use", "Edit", { path: "/tmp/a.ts" }), callId: "e2" },
      result: { ...item("tool_result", "Edit"), callId: "e2", output, status: "ok" },
    });
    expect(lines.some((line) => line.kind === "add" && line.text === "added")).toBe(true);
  });
});

function use(seq: number, callId: string, tool: string, parentCallId: string | null = null, input: Record<string, unknown> = {}) {
  return { ...item("tool_use", tool, input), seq, callId, parentCallId };
}

function result(seq: number, callId: string, parentCallId: string | null = null, output = "ok") {
  return { ...item("tool_result"), seq, callId, parentCallId, output, status: "ok" };
}

function childrenOf(block: TimelineBlock | undefined): TimelineBlock[] {
  if (block?.type === "subagent" || block?.type === "tool" || block?.type === "edit" || block?.type === "shell") {
    return block.item.children ?? [];
  }
  return [];
}

function exploreCallIds(block: TimelineBlock | undefined): Array<string | null> {
  if (block?.type !== "explore") return [];
  return block.items.flatMap((entry) => (entry.kind === "tool" ? [entry.item.use.callId] : []));
}

describe("groupTimeline event tree", () => {
  let warn: MockInstance<typeof console.warn>;
  beforeEach(() => {
    warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
  });
  afterEach(() => {
    warn.mockRestore();
  });

  test("keeps grandchildren of a three-level agent chain", () => {
    const blocks = groupTimeline([
      use(1, "t3-a", "Task"),
      use(2, "t3-b", "Task", "t3-a"),
      use(3, "t3-c", "Read", "t3-b", { path: "c.ts" }),
      result(4, "t3-c", "t3-b"),
    ]);
    expect(blocks.map((block) => block.type)).toEqual(["subagent"]);
    const second = childrenOf(blocks[0]);
    expect(second.map((block) => block.type)).toEqual(["subagent"]);
    expect(exploreCallIds(childrenOf(second[0])[0])).toEqual(["t3-c"]);
    expect(warn).not.toHaveBeenCalled();
  });

  test("keeps every level of a four-level chain", () => {
    const blocks = groupTimeline([
      use(1, "t4-a", "Task"),
      use(2, "t4-b", "Agent", "t4-a"),
      use(3, "t4-c", "Task", "t4-b"),
      use(4, "t4-d", "Bash", "t4-c", { command: "ls" }),
      result(5, "t4-d", "t4-c"),
    ]);
    const levelC = childrenOf(childrenOf(blocks[0])[0]);
    expect(levelC.map((block) => block.type)).toEqual(["subagent"]);
    const levelD = childrenOf(levelC[0]);
    expect(levelD.map((block) => block.type)).toEqual(["shell"]);
    expect(levelD[0]).toMatchObject({ item: { use: { callId: "t4-d" }, incomplete: false } });
  });

  test("puts events with a missing parent at the root and warns once", () => {
    const events = [
      use(1, "miss-a", "Task"),
      use(2, "miss-b", "Read", "miss-ghost", { path: "b.ts" }),
    ];
    const blocks = groupTimeline(events);
    expect(blocks.map((block) => block.type)).toEqual(["subagent", "explore"]);
    groupTimeline(events);
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls[0]?.[1]).toEqual([
      expect.objectContaining({ kind: "missing-parent", callId: "miss-b", parentCallId: "miss-ghost" }),
    ]);
  });

  test("puts a self-referencing event at the root", () => {
    const blocks = groupTimeline([
      use(1, "self-a", "Task", "self-a"),
      use(2, "self-b", "Read", "self-a", { path: "b.ts" }),
    ]);
    expect(blocks.map((block) => block.type)).toEqual(["subagent"]);
    expect(exploreCallIds(childrenOf(blocks[0])[0])).toEqual(["self-b"]);
    expect(warn.mock.calls[0]?.[1]).toEqual([expect.objectContaining({ kind: "self-parent", callId: "self-a" })]);
  });

  test("breaks an A→B→A cycle at the earliest event", () => {
    const blocks = groupTimeline([
      use(1, "cyc-a", "Task", "cyc-b"),
      use(2, "cyc-b", "Task", "cyc-a"),
    ]);
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({ type: "subagent", item: { use: { callId: "cyc-a" } } });
    expect(childrenOf(blocks[0])).toEqual([
      expect.objectContaining({ type: "subagent", item: expect.objectContaining({ use: expect.objectContaining({ callId: "cyc-b" }) }) }),
    ]);
    expect(warn.mock.calls[0]?.[1]).toEqual([expect.objectContaining({ kind: "cycle", callId: "cyc-a" })]);
  });

  test("keeps thinking, tools and permissions under the same parent in order", () => {
    const blocks = groupTimeline([
      use(1, "mix-a", "Task"),
      { ...item("thinking"), seq: 2, content: "子代理在想", parentCallId: "mix-a" },
      use(3, "mix-read", "Read", "mix-a", { path: "a.ts" }),
      result(4, "mix-read", "mix-a"),
      { ...item("permission"), seq: 5, callId: "mix-perm", parentCallId: "mix-a" },
      use(6, "mix-sh", "Bash", "mix-a", { command: "pnpm test" }),
    ]);
    const children = childrenOf(blocks[0]);
    expect(children.map((block) => block.type)).toEqual(["explore", "permission", "shell"]);
    expect(children[2]).toMatchObject({ item: { incomplete: true } });
  });

  test("keeps interleaved parallel subagents separate and sorted by seq", () => {
    const blocks = groupTimeline([
      use(1, "par-a", "Task", null, { description: "查前端" }),
      use(2, "par-b", "Task", null, { description: "查后端" }),
      use(5, "par-a2", "Read", "par-a", { path: "a2.ts" }),
      use(3, "par-a1", "Read", "par-a", { path: "a1.ts" }),
      use(4, "par-b1", "Grep", "par-b", { pattern: "foo" }),
      result(6, "par-a"),
    ]);
    expect(blocks.map((block) => block.type)).toEqual(["subagent", "subagent"]);
    expect(blocks[0]).toMatchObject({ item: { use: { callId: "par-a" }, incomplete: false } });
    expect(blocks[1]).toMatchObject({ item: { use: { callId: "par-b" }, incomplete: true } });
    expect(exploreCallIds(childrenOf(blocks[0])[0])).toEqual(["par-a1", "par-a2"]);
    expect(exploreCallIds(childrenOf(blocks[1])[0])).toEqual(["par-b1"]);
  });
});

describe("subagent display", () => {
  const events = [
    { ...use(1, "sum-a", "Task", null, { description: "查前端", subagent_type: "explorer" }), createdAt: "2026-01-01T00:00:00.000Z" },
    use(2, "sum-a1", "Read", "sum-a", { path: "a.ts" }),
    use(3, "sum-a2", "Bash", "sum-a", { command: "ls" }),
    { ...result(4, "sum-a"), createdAt: "2026-01-01T00:00:34.000Z" },
    use(5, "sum-b", "Task", null, { description: "查后端" }),
    { ...result(6, "sum-b", null, "权限不足\n详细堆栈"), status: "error" },
    use(7, "sum-c", "Task", null, { description: "跑测试" }),
  ];

  test("summarizes a finished subagent with tool count and duration", () => {
    const [first] = groupTimeline(events);
    if (first?.type !== "subagent") throw new Error("expected subagent");
    expect(subAgentSummary(first.item, subAgentStatus(first.item), "zh-CN")).toBe("子代理：查前端 · 完成 · 2 次工具调用 · 34 秒");
  });

  test("shows the first line of the failure reason", () => {
    const second = groupTimeline(events)[1];
    if (second?.type !== "subagent") throw new Error("expected subagent");
    expect(subAgentStatus(second.item)).toBe("failed");
    expect(subAgentFailureLine(second.item)).toBe("权限不足");
  });

  test("tallies subagents for the collapsed replay summary", () => {
    expect(subAgentTallyLabel(groupTimeline(events), false, "zh-CN")).toBe("3 个子代理，1 个完成，1 个失败，1 个未完成");
    expect(subAgentTallyLabel(groupTimeline(events), true, "zh-CN")).toBe("3 个子代理，1 个完成，1 个失败，1 个运行中");
  });

  test("counts running subagents in the live work line", () => {
    expect(liveWorkLabel(
      [use(1, "live-a", "Task"), use(2, "live-b", "Task"), use(3, "live-r", "Read", "live-a", { path: "a.ts" })],
      5,
      { status: "running", language: "zh-CN" },
    )).toBe("2 个子代理运行中 · 读取文件 a.ts · 5 秒");
    expect(liveWorkLabel(
      [use(1, "live-a", "Task"), result(2, "live-a"), use(3, "live-r", "Read", null, { path: "a.ts" })],
      5,
      { status: "running", language: "zh-CN" },
    )).toBe("读取文件 a.ts · 5 秒");
  });
});

describe("shellTranscript", () => {
  test("uses the terminal text inside a tool result and keeps the command at the first line", () => {
    const output = shellResultText(JSON.stringify({
      isBackground: false,
      success: {
        command: "go build ./internal/job/service/client/",
        interleavedOutput: "command not found: -e\n",
      },
    }));
    expect(output).toBe("command not found: -e");
    expect(shellTranscript("go build ./internal/job/service/client/", output)).toBe(
      "$ go build ./internal/job/service/client/\ncommand not found: -e",
    );
  });
});

describe("shell card visibility", () => {
  const use = { seq: 1, eventType: "tool_use" as const, callId: "s", tool: "Shell", content: null, input: null, output: null, createdAt: "" };

  test("stays a title while the command runs, then follows the result", () => {
    expect(shellCardMode({ settled: false, failed: false, hasOutput: false, user: null })).toBe("hidden");
    expect(shellCardMode({ settled: true, failed: false, hasOutput: true, user: null })).toBe("peek");
    expect(shellCardMode({ settled: true, failed: false, hasOutput: false, user: null })).toBe("hidden");
    expect(shellCardMode({ settled: true, failed: true, hasOutput: true, user: null })).toBe("full");
    expect(shellCardMode({ settled: true, failed: true, hasOutput: true, user: "peek" })).toBe("peek");
  });

  test("treats an error status or a non-zero exit as a failure", () => {
    expect(shellFailed({
      use,
      result: { ...use, eventType: "tool_result", status: "error", output: "" },
    })).toBe(true);
    expect(shellFailed({
      use,
      result: { ...use, eventType: "tool_result", status: "ok", output: JSON.stringify({ exitCode: 1, stdout: "no" }) },
    })).toBe(true);
    expect(shellFailed({
      use,
      result: { ...use, eventType: "tool_result", status: "ok", output: JSON.stringify({ exitCode: 0, stdout: "ok" }) },
    })).toBe(false);
  });
});

describe("work duration", () => {
  test("keeps seconds, then folds in minutes and hours", () => {
    expect(formatLiveElapsed(45, "zh-CN")).toBe("45 秒");
    expect(formatLiveElapsed(125, "zh-CN")).toBe("2 分 5 秒");
    expect(formatLiveElapsed(120, "zh-CN")).toBe("2 分");
    expect(formatLiveElapsed(3600, "zh-CN")).toBe("1 小时");
    expect(formatLiveElapsed(3665, "zh-CN")).toBe("1 小时 1 分 5 秒");
    expect(formatLiveElapsed(3605, "zh-CN")).toBe("1 小时 5 秒");
    expect(workFoldLabel(45_400, "zh-CN")).toBe("工作了 45 秒");
    expect(workFoldLabel(125_000, "zh-CN")).toBe("工作了 2 分 5 秒");
    expect(workFoldLabel(3_665_000, "zh-CN")).toBe("工作了 1 小时 1 分 5 秒");
    expect(workFoldLabel(null, "zh-CN")).toBe("查看工作过程");
  });
});
