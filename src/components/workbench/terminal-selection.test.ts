// @vitest-environment node
import { describe, expect, it } from "vitest";
import { placeTerminalAddButton, selectionLines, snapshotLines, terminalBlock, TERMINAL_SNAPSHOT_LINE_CAP } from "./terminal-selection";

describe("terminal selection", () => {
  it("turns buffer rows into a one-based range", () => {
    expect(selectionLines(10, 31)).toEqual({ startLine: 11, endLine: 32 });
    expect(selectionLines(4, 4)).toEqual({ startLine: 5, endLine: 5 });
  });

  it("keeps the tail of a long buffer and drops blank lines at the end", () => {
    const lines = Array.from({ length: TERMINAL_SNAPSHOT_LINE_CAP + 3 }, (_, index) => `L${index + 1}`);
    lines.push("", "");
    const snapshot = snapshotLines(lines);
    expect(snapshot.lineOffset).toBe(6);
    expect(snapshot.text.startsWith("L6\n")).toBe(true);
    expect(snapshot.text.endsWith(`L${TERMINAL_SNAPSHOT_LINE_CAP + 3}`)).toBe(true);
  });

  it("floats the add button above the block, and pins it when the block starts above the view", () => {
    expect(placeTerminalAddButton({ top: 80 }, { width: 400, height: 200 })).toEqual({ x: 298, y: 48 });
    expect(placeTerminalAddButton({ top: -20 }, { width: 400, height: 200 })).toEqual({ x: 298, y: 6 });
  });

  it("treats output between prompts as one block", () => {
    const lines = [
      "tomiaa@Mac Multara % s",
      "✖ 请选择要运行的 npm 命令",
      "Node.js v22.23.2",
      "tomiaa@Mac Multara % a",
      "zsh: command not found: a",
      "tomiaa@Mac Multara %",
    ];
    expect(terminalBlock(lines, 2)).toEqual({ start: 0, end: 2 });
    expect(terminalBlock(lines, 4)).toEqual({ start: 3, end: 4 });
    expect(terminalBlock(lines, 5)).toBeNull();
  });

  it("recognises PowerShell and cmd prompts", () => {
    const lines = ["PS C:\\work> dir", "a.txt", "C:\\work>echo hi", "hi", "PS C:\\work>"];
    expect(terminalBlock(lines, 1)).toEqual({ start: 0, end: 1 });
    expect(terminalBlock(lines, 3)).toEqual({ start: 2, end: 3 });
  });
});
