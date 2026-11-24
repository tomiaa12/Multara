// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest";
import { INBOX_SURFACES_KEY, INBOX_TOOL_TABS_KEY, readInboxSurfaces } from "./inbox-tool";

const memory = new Map<string, string>();

beforeEach(() => {
  memory.clear();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => memory.get(key) ?? null,
    setItem: (key: string, value: string) => {
      memory.set(key, value);
    },
    removeItem: (key: string) => {
      memory.delete(key);
    },
  });
});

describe("readInboxSurfaces", () => {
  it("drops saved browser tabs so the rail does not open with them", () => {
    memory.set(INBOX_SURFACES_KEY, JSON.stringify(["browser", "changes", "browser", "browser"]));
    expect(readInboxSurfaces().map((tab) => tab.kind)).toEqual(["changes"]);
  });

  it("does not migrate a browser tool tab into a surface", () => {
    memory.set(INBOX_TOOL_TABS_KEY, JSON.stringify(["browser", "changes"]));
    expect(readInboxSurfaces().map((tab) => tab.kind)).toEqual(["changes"]);
  });

  it("starts empty when nothing was saved", () => {
    expect(readInboxSurfaces()).toEqual([]);
  });
});
