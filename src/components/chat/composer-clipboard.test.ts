// @vitest-environment node
import { describe, expect, test } from "vitest";
import { composerClipboardHtml, decodeComposerClipboard } from "./composer-clipboard";
import type { ComposerNode } from "./composer-content";

const nodes: ComposerNode[] = [
  { type: "text", value: "看一下 " },
  {
    type: "chip",
    id: "chip-1",
    path: "src/App.vue",
    startLine: 11,
    endLine: 32,
    text: "const a = 1;",
  },
  { type: "text", value: " 这里" },
];

describe("composer clipboard", () => {
  test("round-trips chips and surrounding text", () => {
    const html = composerClipboardHtml(nodes, "看一下 App.vue (11-32) 这里");
    const decoded = decodeComposerClipboard(html);
    expect(decoded).toEqual(nodes);
  });

  test("ignores plain text that is not a composer payload", () => {
    expect(decodeComposerClipboard("App.vue (11-32)")).toBeNull();
  });

  test("keeps entity ids stable when copied", () => {
    const reference: ComposerNode[] = [{
      type: "chip",
      id: "requirement:req-7",
      referenceId: "req-7",
      referenceKind: "requirement",
      label: "级联输入",
    }];
    expect(decodeComposerClipboard(composerClipboardHtml(reference, "需求 · 级联输入"))).toEqual(reference);
  });
});
