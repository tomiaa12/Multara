// @vitest-environment node
import { describe, expect, test } from "vitest";
import { terminalCellSize } from "./terminal-geometry";
import type { Terminal } from "@xterm/xterm";

function fakeTerm(partial: {
  rows?: number;
  cols?: number;
  cell?: { width: number; height: number };
}): Terminal {
  return {
    rows: partial.rows ?? 24,
    cols: partial.cols ?? 80,
    _core: partial.cell
      ? { _renderService: { dimensions: { css: { cell: partial.cell } } } }
      : undefined,
  } as unknown as Terminal;
}

describe("terminalCellSize", () => {
  test("prefers xterm renderService cell dimensions", () => {
    const element = { querySelector: () => null } as unknown as HTMLElement;
    expect(terminalCellSize(fakeTerm({ cell: { width: 9.5, height: 18 } }), element)).toEqual({
      width: 9.5,
      height: 18,
    });
  });

  test("falls back to dividing the screen element", () => {
    const screen = { clientWidth: 800, clientHeight: 480 };
    const element = {
      querySelector: (selector: string) => (selector === ".xterm-screen" ? screen : null),
    } as unknown as HTMLElement;
    expect(terminalCellSize(fakeTerm({ rows: 24, cols: 80 }), element)).toEqual({
      width: 10,
      height: 20,
    });
  });

  test("returns null when neither renderService nor screen is usable", () => {
    const element = { querySelector: () => null } as unknown as HTMLElement;
    expect(terminalCellSize(fakeTerm({}), element)).toBeNull();
  });
});
