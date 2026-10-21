// @vitest-environment node
import { describe, expect, test } from "vitest";
import { formatAgentPrompt, isPastedImage, parseUserPromptNodes, visiblePromptText, type ComposerNode } from "./composer-content";

const chip: ComposerNode = {
  type: "chip",
  id: "src/App.vue:1-2",
  path: "src/App.vue",
  startLine: 1,
  endLine: 2,
  text: "const a = 1;",
};

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
});
