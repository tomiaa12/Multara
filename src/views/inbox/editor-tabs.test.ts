// @vitest-environment node
import { describe, expect, test } from "vitest";
import { placeEditorTab } from "./editor-tabs";

type Tab = { id: string; pinned: boolean; dirty?: boolean };

describe("placeEditorTab", () => {
  test("replaces the italic preview tab", () => {
    const next = placeEditorTab(
      [{ id: "a", pinned: true }, { id: "b", pinned: false }],
      { id: "c", pinned: false },
      () => false,
    );
    expect(next.map((tab) => tab.id)).toEqual(["a", "c"]);
  });

  test("pins a dirty preview before opening another file", () => {
    const next = placeEditorTab(
      [{ id: "b", pinned: false, dirty: true }],
      { id: "c", pinned: false },
      (tab) => Boolean(tab.dirty),
    );
    expect(next).toEqual([
      { id: "b", pinned: true, dirty: true },
      { id: "c", pinned: false },
    ]);
  });

  test("appends a pinned tab beside the preview", () => {
    const next = placeEditorTab(
      [{ id: "b", pinned: false }],
      { id: "c", pinned: true },
      () => false,
    );
    expect(next.map((tab) => tab.id)).toEqual(["b", "c"]);
  });
});
