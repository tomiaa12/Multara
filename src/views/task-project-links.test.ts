// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { ComposerNode } from "../components/chat/composer-content";
import { requirementIds, syncRequirementChips } from "./task-project-links";

const requirement = (id: string, label: string): ComposerNode => ({
  type: "chip",
  id: `requirement:${id}`,
  referenceId: id,
  referenceKind: "requirement",
  label,
});

describe("task project links", () => {
  it("reads requirement chips back into the multi-select in chip order", () => {
    expect(requirementIds([
      { type: "text", value: "处理 " },
      requirement("p2", "需求二"),
      requirement("p1", "需求一"),
    ])).toEqual(["p2", "p1"]);
  });

  it("replaces requirement chips from the multi-select without removing other content", () => {
    const nodes: ComposerNode[] = [
      { type: "text", value: "请处理 " },
      {
        type: "chip",
        id: "agent:a1",
        referenceId: "a1",
        referenceKind: "agent",
        label: "Miku",
      },
      requirement("old", "旧需求"),
    ];

    const synced = syncRequirementChips(
      nodes,
      ["p2", "p1"],
      [{ id: "p1", name: "需求一" }, { id: "p2", name: "需求二" }],
    );

    expect(requirementIds(synced)).toEqual(["p2", "p1"]);
    expect(synced).toContainEqual(expect.objectContaining({ referenceKind: "agent", referenceId: "a1" }));
  });
});
