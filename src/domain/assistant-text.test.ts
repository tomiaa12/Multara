// @vitest-environment node
import { describe, expect, test } from "vitest";
import { applyAssistantChunk, liveAssistantText } from "./assistant-text";

describe("applyAssistantChunk", () => {
  test("replaces a complete assistant message instead of stacking it", () => {
    const first = applyAssistantChunk(
      "",
      "先查看当前 Windows 扫描与索引相关实现，再扩展对商店应用和 PWA 的支持。",
      { replace: true },
    );
    const next = applyAssistantChunk(
      first,
      "开始实现：扩展 StartApps 扫描、修复 PWA 快捷方式，并支持 shell:AppsFolder 启动。",
      { replace: true },
    );
    expect(next).toBe(
      "开始实现：扩展 StartApps 扫描、修复 PWA 快捷方式，并支持 shell:AppsFolder 启动。",
    );
    expect(next).not.toContain("先查看当前 Windows 扫描");
  });

  test("appends streaming deltas onto the current message", () => {
    expect(applyAssistantChunk("开始", "实现", {})).toBe("开始实现");
  });

  test("completed payload always becomes the visible text", () => {
    expect(
      applyAssistantChunk("旧计划", "最终回答", { completed: true }),
    ).toBe("最终回答");
  });
});

describe("liveAssistantText", () => {
  test("keeps only the last replacing assistant message", () => {
    expect(
      liveAssistantText([
        { eventType: "thinking", content: "先看扫描" },
        {
          eventType: "text",
          content: "先查看当前 Windows 扫描与索引相关实现，再扩展对商店应用和 PWA 的支持。",
          replacesFinalText: true,
        },
        { eventType: "tool_use", content: null },
        {
          eventType: "text",
          content: "开始实现：扩展 StartApps 扫描。",
          replacesFinalText: true,
        },
      ]),
    ).toBe("开始实现：扩展 StartApps 扫描。");
  });

  test("joins unmarked text when no replacement marker exists", () => {
    expect(
      liveAssistantText([
        { eventType: "text", content: "Hello" },
        { eventType: "text", content: " world" },
      ]),
    ).toBe("Hello world");
  });
});
