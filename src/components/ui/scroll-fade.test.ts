// @vitest-environment node

import { describe, expect, test } from "vitest";
import { hasMoreBelow } from "./scroll-fade";

describe("hasMoreBelow", () => {
  test("false when null or fully scrolled", () => {
    expect(hasMoreBelow(null)).toBe(false);
    expect(
      hasMoreBelow({
        scrollHeight: 100,
        scrollTop: 40,
        clientHeight: 60,
      } as HTMLElement),
    ).toBe(false);
  });

  test("true when content remains below", () => {
    expect(
      hasMoreBelow({
        scrollHeight: 200,
        scrollTop: 0,
        clientHeight: 80,
      } as HTMLElement),
    ).toBe(true);
  });
});
