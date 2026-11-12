// @vitest-environment node
import { describe, expect, test } from "vitest";
import {
  carriedMentions,
  formatAgentPrompt,
  locatedChipPath,
  isOversizedInlineSelection,
  isPastedImage,
  MAX_INLINE_SELECTION_LINES,
  parseUserPromptNodes,
  visiblePromptText,
  type ComposerNode,
} from "./composer-content";

const chip: ComposerNode = {
  type: "chip",
  id: "src/App.vue:1-2",
  path: "src/App.vue",
  startLine: 1,
  endLine: 2,
  text: "const a = 1;",
};

describe("carriedMentions", () => {
  const mention = (referenceKind: "agent" | "squad" | "skill", referenceId: string): ComposerNode => ({
    type: "chip",
    id: `${referenceKind}:${referenceId}`,
    referenceId,
    referenceKind,
    label: referenceId,
  });

  test("carries @ agents and squads other than the session target into the next draft", () => {
    const nodes: ComposerNode[] = [
      mention("agent", "a1"),
      { type: "text", value: " 改一下按钮 " },
      mention("agent", "a1"),
      mention("squad", "team"),
      mention("skill", "s1"),
      chip,
    ];
    expect(carriedMentions(nodes, "agent:miku")).toEqual([
      mention("agent", "a1"),
      mention("squad", "team"),
      { type: "text", value: " " },
    ]);
  });

  test("carries nothing when the message only mentions the session target", () => {
    expect(carriedMentions([mention("agent", "miku"), { type: "text", value: "hi" }], "agent:miku")).toEqual([]);
    expect(carriedMentions([{ type: "text", value: "hi" }], "agent:miku")).toEqual([]);
  });
});

describe("isPastedImage", () => {
  test("keeps clipboard images that were saved under their original filename", () => {
    expect(isPastedImage({
      name: "截图.png",
      mimeType: "image/png",
      path: "/tmp/multara-paste/abc.png",
    })).toBe(true);
    expect(isPastedImage({ name: "notes.png", mimeType: "image/png", path: "/Users/a/notes.png" })).toBe(false);
  });
});

describe("parseUserPromptNodes", () => {
  test("keeps a chip in place when the user text contains the same label", () => {
    const nodes: ComposerNode[] = [
      { type: "text", value: "看 App.vue (1-2) 和 " },
      chip,
      { type: "text", value: " 这里" },
    ];
    const parsed = parseUserPromptNodes(formatAgentPrompt(nodes));
    expect(parsed.map((node) => node.type === "chip" ? "chip" : node.value)).toEqual([
      "看 App.vue (1-2) 和 ",
      "chip",
      " 这里",
    ]);
    expect(visiblePromptText(formatAgentPrompt(nodes))).toBe("看 App.vue (1-2) 和 App.vue (1-2) 这里");
  });

  test("still parses older prompts that have no placement marks", () => {
    const content = "[代码引用] src/App.vue:1-2\n```\nconst a = 1;\n```\n\n请看 App.vue (1-2)";
    const parsed = parseUserPromptNodes(content);
    expect(parsed.some((node) => node.type === "chip")).toBe(true);
    expect(visiblePromptText(content)).toContain("App.vue (1-2)");
  });

  test("uses a longer fence when chip text contains backtick fences", () => {
    const text = "```ts\nconst a = 1;\n```\n再来 ```` 四个";
    const nodes: ComposerNode[] = [
      { ...chip, text },
      { type: "text", value: " 看看" },
    ];
    const content = formatAgentPrompt(nodes);
    expect(content).toContain("\n`````\n");
    const parsed = parseUserPromptNodes(content);
    expect(parsed).toEqual([{ ...chip, text }, { type: "text", value: " 看看" }]);
  });

  test("emits path-only refs for folder and whole-file chips", () => {
    const folder: ComposerNode = { type: "chip", id: "docs:folder", path: "docs", startLine: 0, endLine: 0, text: "a.md\n```", isDirectory: true };
    const file: ComposerNode = { type: "chip", id: "README.md:file", path: "README.md", startLine: 0, endLine: 0, text: "# t\n```sh\nls\n```" };
    const content = formatAgentPrompt([folder, file]);
    expect(content).toContain("[文件夹] docs");
    expect(content).toContain("[文件] README.md");
    expect(content).not.toContain("```");
    expect(content).not.toContain("# t");
    expect(content).not.toContain("a.md");
    expect(parseUserPromptNodes(content)).toEqual([
      { ...folder, text: "" },
      { ...file, text: "" },
    ]);
  });

  test("joins a workspace-relative chip to that workspace root", () => {
    const folder: ComposerNode = {
      type: "chip",
      id: "src:folder",
      path: "src",
      startLine: 0,
      endLine: 0,
      text: "",
      isDirectory: true,
      workspaceId: "ws-1",
    };
    const workspaces = [{ id: "ws-1", workPath: "/Users/tomiaa/Documents/work/pages", sourcePath: "/other" }];
    const content = formatAgentPrompt([folder, { type: "text", value: "看这里" }], { workspaces });
    expect(content).toContain("[文件夹] /Users/tomiaa/Documents/work/pages/src");
    expect(locatedChipPath({ path: "/Users/tomiaa/Documents/work", workspaceId: "" }, workspaces))
      .toBe("/Users/tomiaa/Documents/work");
  });

  test("drops a bare name that does not match a workspace", () => {
    const folder: ComposerNode = {
      type: "chip",
      id: "work:folder",
      path: "work",
      startLine: 0,
      endLine: 0,
      text: "",
      isDirectory: true,
    };
    const content = formatAgentPrompt(
      [folder, { type: "text", value: "找一下" }],
      { workspaces: [{ id: "ws-1", sourcePath: "/Users/tomiaa/Documents/work/pages" }] },
    );
    expect(content).not.toContain("[文件夹]");
    expect(content).toContain("找一下");
  });

  test("still parses older whole-file and folder prompts that inlined content", () => {
    const content = "[文件夹] docs\n```\na.md\n```\n\n[文件] README.md\n```\n# t\n```";
    const parsed = parseUserPromptNodes(content);
    expect(parsed).toEqual([
      { type: "chip", id: "docs:folder", path: "docs", startLine: 0, endLine: 0, text: "a.md", isDirectory: true },
      { type: "chip", id: "README.md:file", path: "README.md", startLine: 0, endLine: 0, text: "# t" },
    ]);
  });

  test("oversized code selections emit path-range only", () => {
    const lines = Array.from({ length: MAX_INLINE_SELECTION_LINES + 1 }, (_, i) => `line ${i}`).join("\n");
    const big: ComposerNode = {
      type: "chip",
      id: "big.ts:1-401",
      path: "big.ts",
      startLine: 1,
      endLine: MAX_INLINE_SELECTION_LINES + 1,
      text: lines,
    };
    expect(isOversizedInlineSelection(lines, 1, MAX_INLINE_SELECTION_LINES + 1)).toBe(true);
    const content = formatAgentPrompt([big]);
    expect(content).toContain("[代码引用] big.ts:1-401");
    expect(content).not.toContain("line 0");
    expect(parseUserPromptNodes("[代码引用] big.ts:1-401")).toEqual([
      { type: "chip", id: "big.ts:1-401", path: "big.ts", startLine: 1, endLine: 401, text: "" },
    ]);
  });

  test("round-trips structured references with stable ids", () => {
    const nodes: ComposerNode[] = [
      { type: "text", value: "请让 " },
      {
        type: "chip",
        id: "agent:agent-42",
        referenceId: "agent-42",
        referenceKind: "agent",
        label: "Miku 助手",
      },
      { type: "text", value: " 处理" },
    ];
    const content = formatAgentPrompt(nodes, { directMentions: true });
    expect(content).toContain("[实体引用] agent agent-42 Miku%20%E5%8A%A9%E6%89%8B");
    expect(content).not.toContain("[指派要求]");
    expect(parseUserPromptNodes(content)).toEqual(nodes);
    expect(formatAgentPrompt(nodes)).toContain("[指派要求]");
    // 标题截取应走可见正文，不能以协议头开头
    expect(visiblePromptText(content)).toBe("请让 @Miku 助手 处理");
    expect(visiblePromptText(content).startsWith("[实体引用]")).toBe(false);
  });
});
