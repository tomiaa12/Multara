import { describe, expect, it } from "vitest";
import { syncDraft } from "./draft-sync";

type Draft = { name: string; instructions: string; ids: string[] };

describe("syncDraft", () => {
  it("resets every field when switching objects", () => {
    const draft: Draft = { name: "edited", instructions: "", ids: [] };
    const baseline: Draft = { name: "", instructions: "", ids: [] };
    syncDraft(draft, baseline, { name: "A", instructions: "do", ids: ["x"] }, true);
    expect(draft).toEqual({ name: "A", instructions: "do", ids: ["x"] });
    expect(baseline).toEqual(draft);
    expect(draft.ids).not.toBe(baseline.ids);
  });

  it("keeps unsaved edits and only syncs untouched fields", () => {
    const draft: Draft = { name: "A", instructions: "typing…", ids: ["x"] };
    const baseline: Draft = { name: "A", instructions: "do", ids: ["x"] };
    syncDraft(draft, baseline, { name: "Renamed", instructions: "do", ids: ["x", "y"] }, false);
    expect(draft).toEqual({ name: "Renamed", instructions: "typing…", ids: ["x", "y"] });
    expect(baseline.instructions).toBe("do");
  });
});
