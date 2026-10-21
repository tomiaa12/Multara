import { beforeEach, describe, expect, it } from "vitest";
import { nextPopupZIndex, peekPopupZIndex, resetPopupZIndex } from "./popup-z-index";

beforeEach(() => {
  resetPopupZIndex();
});

describe("popup-z-index", () => {
  it("starts above the modal layer", () => {
    expect(nextPopupZIndex()).toBe(301);
    expect(nextPopupZIndex()).toBe(302);
    expect(peekPopupZIndex()).toBe(302);
  });
});
