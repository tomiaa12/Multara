// @vitest-environment node
import { describe, expect, test } from "vitest";
import type { TimelineItem } from "../../domain/types";
import { groupTimeline, liveWorkLabel, shellTranscript, shellResultText } from "./timeline-display";

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

describe("liveWorkLabel", () => {
  test("stays on the latest open action and keeps the elapsed clock visible", () => {
    expect(liveWorkLabel([], 3, { status: "sending", language: "zh-CN" })).toBe("等待响应 · 3 秒");
    expect(liveWorkLabel([], 3, { status: "running", language: "zh-CN" })).toBe("思考中… · 3 秒");
    expect(liveWorkLabel([], 3, { status: "queued", language: "zh-CN" })).toBe("排队中 · 3 秒");
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
    expect(blocks).toHaveLength(1);
    const block = blocks[0];
    if (block?.type !== "tool") return;
    expect(block.item.children?.some((child) => child.type === "explore")).toBe(true);
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
