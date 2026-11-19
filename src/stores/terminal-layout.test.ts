// @vitest-environment node
import { afterEach, describe, expect, test, vi } from "vitest";
import { readProjectTerminalOpen, writeProjectTerminalOpen } from "./terminal-layout";

const memory = new Map<string, string>();

describe("terminal-layout", () => {
  afterEach(() => {
    memory.clear();
    vi.unstubAllGlobals();
  });

  test("remembers open state per project and surface", () => {
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => memory.get(key) ?? null,
      setItem: (key: string, value: string) => {
        memory.set(key, value);
      },
    });

    expect(readProjectTerminalOpen("workbench", "p1")).toBe(false);
    writeProjectTerminalOpen("workbench", "p1", true);
    writeProjectTerminalOpen("inbox", "p1", false);
    writeProjectTerminalOpen("workbench", "p2", false);

    expect(readProjectTerminalOpen("workbench", "p1")).toBe(true);
    expect(readProjectTerminalOpen("inbox", "p1")).toBe(false);
    expect(readProjectTerminalOpen("workbench", "p2")).toBe(false);
  });

  test("falls back to legacy inbox key when project key is missing", () => {
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => memory.get(key) ?? null,
      setItem: (key: string, value: string) => {
        memory.set(key, value);
      },
    });
    memory.set("multara.inbox.terminalOpen", "1");
    expect(readProjectTerminalOpen("inbox", "fresh")).toBe(true);
  });
});
