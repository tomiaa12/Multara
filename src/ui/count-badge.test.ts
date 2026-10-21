import { describe, expect, it } from "vitest";
import { countBadgeDigitsAttr, formatCountBadge, resolveCountBadgeLabel } from "./count-badge";

describe("formatCountBadge", () => {
  it("keeps counts at or below the limit", () => {
    expect(formatCountBadge(0)).toBe("0");
    expect(formatCountBadge(9)).toBe("9");
    expect(formatCountBadge(99)).toBe("99");
  });

  it("caps above the limit as 99+", () => {
    expect(formatCountBadge(100)).toBe("99+");
    expect(formatCountBadge(1000)).toBe("99+");
  });

  it("rejects non-finite values", () => {
    expect(formatCountBadge(Number.NaN)).toBe("");
  });
});

describe("resolveCountBadgeLabel", () => {
  it("formats numbers and keeps strings", () => {
    expect(resolveCountBadgeLabel(120)).toBe("99+");
    expect(resolveCountBadgeLabel("3")).toBe("3");
  });
});

describe("countBadgeDigitsAttr", () => {
  it("marks single-digit labels for a circular badge", () => {
    expect(countBadgeDigitsAttr("5")).toBe("1");
    expect(countBadgeDigitsAttr("12")).toBeUndefined();
    expect(countBadgeDigitsAttr("99+")).toBeUndefined();
  });
});
