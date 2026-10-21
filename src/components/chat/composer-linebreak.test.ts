// @vitest-environment happy-dom
import { describe, expect, test } from "vitest";
import { caretScrollDelta, insertComposerLineBreak, LINE_BREAK_PAD } from "./composer-linebreak";

function placeCaret(root: HTMLElement) {
  const range = document.createRange();
  range.selectNodeContents(root);
  range.collapse(false);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}

describe("caretScrollDelta", () => {
  test("scrolls just enough to bring the caret line back into view", () => {
    expect(caretScrollDelta(0, 100, 80, 120)).toBe(20);
    expect(caretScrollDelta(40, 140, 10, 30)).toBe(-30);
    expect(caretScrollDelta(0, 100, 20, 40)).toBe(0);
  });
});

describe("insertComposerLineBreak", () => {
  test("repeated breaks keep the caret in the new line pad", () => {
    const root = document.createElement("div");
    document.body.append(root);
    placeCaret(root);

    insertComposerLineBreak(root);
    insertComposerLineBreak(root);

    const selection = window.getSelection();
    expect(root.innerHTML).toBe(`<br>${LINE_BREAK_PAD}<br>${LINE_BREAK_PAD}`);
    expect(selection?.anchorNode?.textContent).toBe(LINE_BREAK_PAD);
    expect(selection?.anchorOffset).toBe(LINE_BREAK_PAD.length);
    expect(selection?.anchorNode).toBe(root.lastChild);
  });
});
