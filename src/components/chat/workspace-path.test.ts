import { describe, expect, test } from "vitest";
import { parseCodeFenceInfo, splitWorkspacePaths } from "./workspace-path";

describe("splitWorkspacePaths", () => {
  test("marks a workspace-relative file path and an optional line range", () => {
    const segments = splitWorkspacePaths("已改代码在 internal/job/service/client/client.go:12-18 删了两处");
    expect(segments.filter((segment) => segment.type === "path")).toEqual([
      {
        type: "path",
        text: "internal/job/service/client/client.go:12-18",
        path: "internal/job/service/client/client.go",
        startLine: 12,
        endLine: 18,
      },
    ]);
  });

  test("leaves urls and extension-less words as plain text", () => {
    const segments = splitWorkspacePaths("见 https://example.com/internal/job/client.go 和 internal/job");
    expect(segments.every((segment) => segment.type === "text")).toBe(true);
  });
});

describe("parseCodeFenceInfo", () => {
  test("parses Cursor-style start:end:path fences", () => {
    expect(parseCodeFenceInfo("12:15:src/App.vue")).toEqual({
      language: "xml",
      path: "src/App.vue",
      startLine: 12,
      endLine: 15,
    });
  });

  test("parses language-prefixed citation fences", () => {
    expect(parseCodeFenceInfo("ts:12:15:src/stores/app.ts")).toEqual({
      language: "ts",
      path: "src/stores/app.ts",
      startLine: 12,
      endLine: 15,
    });
  });

  test("parses a bare file path as the fence info", () => {
    expect(parseCodeFenceInfo("src/components/chat/RichContent.vue")).toEqual({
      language: "xml",
      path: "src/components/chat/RichContent.vue",
    });
  });

  test("keeps ordinary language tags", () => {
    expect(parseCodeFenceInfo("typescript")).toEqual({ language: "typescript" });
  });
});
