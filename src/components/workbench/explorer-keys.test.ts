// @vitest-environment node
import { describe, expect, test } from "vitest";
import { explorerKey } from "./explorer-keys";

const plain = { metaKey: false, ctrlKey: false, altKey: false, shiftKey: false };
const win = { isMac: false };
const mac = { isMac: true };

const action = (event: Partial<typeof plain> & { key: string }, options: { isMac: boolean; chordK?: boolean }) =>
  explorerKey({ ...plain, ...event }, options)?.action ?? null;

describe("explorerKey", () => {
  test("renames with Enter on Mac and F2 elsewhere", () => {
    expect(action({ key: "Enter" }, mac)).toBe("rename");
    expect(action({ key: "F2" }, win)).toBe("rename");
    expect(action({ key: "Enter" }, win)).toBe("open");
  });

  test("moves to trash or deletes permanently", () => {
    expect(action({ key: "Delete" }, win)).toBe("delete");
    expect(action({ key: "Delete", shiftKey: true }, win)).toBe("delete-permanent");
    expect(action({ key: "Backspace", metaKey: true }, mac)).toBe("delete");
    expect(action({ key: "Backspace", metaKey: true, altKey: true }, mac)).toBe("delete-permanent");
  });

  test("copies paths with the platform chord", () => {
    expect(action({ key: "C", altKey: true, shiftKey: true }, win)).toBe("copy-path");
    expect(action({ key: "c", altKey: true, metaKey: true }, mac)).toBe("copy-path");
    expect(action({ key: "c", altKey: true, metaKey: true, shiftKey: true }, mac)).toBe("copy-relative");
    expect(action({ key: "k", ctrlKey: true }, win)).toBe("chord-k");
    expect(action({ key: "C", ctrlKey: true, shiftKey: true }, { isMac: false, chordK: true })).toBe("copy-relative");
  });

  test("clipboard, undo and selection shortcuts use the primary modifier", () => {
    expect(action({ key: "c", ctrlKey: true }, win)).toBe("copy");
    expect(action({ key: "x", metaKey: true }, mac)).toBe("cut");
    expect(action({ key: "v", ctrlKey: true }, win)).toBe("paste");
    expect(action({ key: "z", ctrlKey: true }, win)).toBe("undo");
    expect(action({ key: "y", ctrlKey: true }, win)).toBe("redo");
    expect(action({ key: "Z", metaKey: true, shiftKey: true }, mac)).toBe("redo");
    expect(action({ key: "a", ctrlKey: true }, win)).toBe("select-all");
    expect(action({ key: "f", ctrlKey: true, altKey: true }, win)).toBe("find");
    expect(action({ key: "c", metaKey: true }, win)).toBeNull();
  });

  test("navigates with arrows and extends selection with Shift", () => {
    expect(action({ key: "ArrowDown" }, win)).toBe("next");
    expect(explorerKey({ ...plain, key: "ArrowUp", shiftKey: true }, win)).toEqual({ action: "previous", extend: true });
    expect(action({ key: "Home" }, win)).toBe("first");
    expect(action({ key: "PageDown" }, mac)).toBe("page-down");
    expect(action({ key: "ArrowLeft" }, win)).toBe("left");
    expect(action({ key: "ArrowRight" }, mac)).toBe("right");
    expect(action({ key: "ArrowLeft", ctrlKey: true }, win)).toBe("collapse-all");
  });

  test("opens to the side and keeps focus with Space", () => {
    expect(action({ key: "Enter", ctrlKey: true }, win)).toBe("open-side");
    expect(action({ key: " " }, win)).toBe("open-preserve-focus");
    expect(action({ key: "ArrowDown", metaKey: true }, mac)).toBe("open");
    expect(action({ key: "Escape" }, win)).toBe("cancel-cut");
  });
});
