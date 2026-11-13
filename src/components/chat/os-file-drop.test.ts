// @vitest-environment node
import { describe, expect, test } from "vitest";
import { osDropClientPoint, osDropScale } from "./os-file-drop";

describe("osDropClientPoint", () => {
  test("converts physical pixels into css pixels", () => {
    expect(osDropClientPoint({ x: 200, y: 80 }, 2)).toEqual({ x: 100, y: 40 });
  });

  test("keeps the point when the scale is missing", () => {
    expect(osDropClientPoint({ x: 12, y: 8 }, 0)).toEqual({ x: 12, y: 8 });
  });
});

describe("osDropScale", () => {
  test("keeps macOS AppKit points as css pixels", () => {
    expect(osDropScale("macos", 2)).toBe(1);
  });

  test("converts Windows client pixels with the screen scale", () => {
    expect(osDropScale("windows", 2)).toBe(2);
    expect(osDropScale("other", 0)).toBe(1);
  });
});
