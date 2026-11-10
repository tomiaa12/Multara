// @vitest-environment node
import { describe, expect, test } from "vitest";
import { osDropClientPoint } from "./os-file-drop";

describe("osDropClientPoint", () => {
  test("converts physical pixels into css pixels", () => {
    expect(osDropClientPoint({ x: 200, y: 80 }, 2)).toEqual({ x: 100, y: 40 });
  });

  test("keeps the point when the scale is missing", () => {
    expect(osDropClientPoint({ x: 12, y: 8 }, 0)).toEqual({ x: 12, y: 8 });
  });
});
