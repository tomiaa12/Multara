// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  matchesCascadeQuery,
  navigateCascade,
  nextCascadeVisibleCount,
  type CascadeNavigationState,
} from "./composer-cascade";

const initial: CascadeNavigationState = {
  pane: "categories",
  categoryIndex: 0,
  resultIndex: 0,
};

describe("composer cascade", () => {
  it("uses up/down in the current pane and left/right to switch panes", () => {
    const category = navigateCascade(initial, "ArrowDown", 4, 8);
    expect(category.state).toEqual({ pane: "categories", categoryIndex: 1, resultIndex: 0 });

    const results = navigateCascade(category.state, "ArrowRight", 4, 8);
    expect(results.state).toEqual({ pane: "results", categoryIndex: 1, resultIndex: 0 });
    expect(navigateCascade(results.state, "ArrowDown", 4, 8).state.resultIndex).toBe(1);
    expect(navigateCascade(results.state, "ArrowLeft", 4, 8).state.pane).toBe("categories");
  });

  it("enters results before activating the highlighted result", () => {
    const enter = navigateCascade(initial, "Enter", 4, 2);
    expect(enter.intent).toBe("enter-results");
    expect(navigateCascade(enter.state, "Enter", 4, 2).intent).toBe("activate-result");
  });

  it("filters labels and descriptions without case sensitivity", () => {
    expect(matchesCascadeQuery("Miku", "Auto agent", "mIK")).toBe(true);
    expect(matchesCascadeQuery("产品开发小队", "把需求拆给界面和实现", "界面")).toBe(true);
    expect(matchesCascadeQuery("Miku", undefined, "Claude")).toBe(false);
  });

  it("loads one bounded page at a time", () => {
    expect(nextCascadeVisibleCount(undefined, 13, 5)).toBe(10);
    expect(nextCascadeVisibleCount(10, 13, 5)).toBe(13);
  });
});
