// @vitest-environment node
import { afterEach, describe, expect, test, vi } from "vitest";
import { createComposerIme } from "./composer-ime";

const enter = { isComposing: false, keyCode: 13 };

describe("createComposerIme", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  test("Enter after compositionend confirms the candidate and does not submit", () => {
    vi.useFakeTimers();
    const ime = createComposerIme();
    ime.compositionStart();
    expect(ime.classifyEnter({ isComposing: true, keyCode: 229 })).toBe("ime");
    ime.compositionEnd();
    expect(ime.classifyEnter(enter)).toBe("confirm");
    vi.runAllTimers();
    expect(ime.classifyEnter(enter)).toBe("submit");
    ime.dispose();
  });

  test("Enter outside composition submits", () => {
    const ime = createComposerIme();
    expect(ime.classifyEnter(enter)).toBe("submit");
    ime.dispose();
  });
});
