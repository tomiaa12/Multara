import { describe, expect, it } from "vitest";
import { pickTooltipPlacement } from "./tooltip-placement";

const tip = { width: 120, height: 32 };
const viewport = { width: 800, height: 600 };

describe("pickTooltipPlacement", () => {
  it("prefers top when there is room", () => {
    const result = pickTooltipPlacement(
      { top: 200, left: 300, width: 80, height: 24 },
      tip,
      "top",
      6,
      8,
      viewport,
    );
    expect(result.placement).toBe("top");
    expect(result.top).toBeLessThan(200);
  });

  it("flips to bottom when top is tight", () => {
    const result = pickTooltipPlacement(
      { top: 20, left: 300, width: 80, height: 24 },
      tip,
      "top",
      6,
      8,
      viewport,
    );
    expect(result.placement).toBe("bottom");
  });

  it("uses right when horizontal preference fits better", () => {
    const result = pickTooltipPlacement(
      { top: 20, left: 20, width: 40, height: 560 },
      tip,
      "left",
      6,
      8,
      viewport,
    );
    expect(result.placement).toBe("right");
  });

  it("keeps the tip inside the viewport", () => {
    const result = pickTooltipPlacement(
      { top: 40, left: 780, width: 16, height: 16 },
      tip,
      "top",
      6,
      8,
      viewport,
    );
    expect(result.left).toBeGreaterThanOrEqual(8);
    expect(result.left + tip.width).toBeLessThanOrEqual(viewport.width - 8);
  });
});
