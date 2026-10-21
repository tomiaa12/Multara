import { describe, expect, it } from "vitest";
import { fitMiddleEllipsis, middleEllipsisPath } from "./middle-ellipsis-path";

describe("fitMiddleEllipsis", () => {
  const measure = (value: string) => value.length;

  it("keeps short text", () => {
    expect(fitMiddleEllipsis("abc", 10, measure)).toBe("abc");
  });

  it("inserts ellipsis when overflowing", () => {
    const text = "Users/tomiaa/Documents/github/work_bk/uc";
    const out = fitMiddleEllipsis(text, 18, measure);
    expect(out.includes("...")).toBe(true);
    expect(out.length).toBeLessThanOrEqual(18);
    expect(out.startsWith("U")).toBe(true);
    expect(out.endsWith("uc")).toBe(true);
  });

  it("falls back to ellipsis when width is tiny", () => {
    expect(fitMiddleEllipsis("abcdef", 3, measure)).toBe("...");
  });
});

describe("middleEllipsisPath", () => {
  it("ellipses path segments", () => {
    expect(middleEllipsisPath("/Users/tomiaa/Documents/github/work_bk/uc")).toBe(
      "Users/.../work_bk/uc",
    );
  });
});
