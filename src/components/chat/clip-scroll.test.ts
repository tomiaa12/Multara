// @vitest-environment happy-dom
import { describe, expect, test } from "vitest";
import { forwardWheel, scrollableAncestor } from "./clip-scroll";

describe("forwardWheel", () => {
  test("scrolls the outer list instead of the nested terminal body", () => {
    const list = document.createElement("div");
    list.style.overflowY = "auto";
    Object.defineProperty(list, "clientHeight", { value: 100 });
    Object.defineProperty(list, "scrollHeight", { value: 400 });
    const terminal = document.createElement("div");
    list.appendChild(terminal);
    document.body.appendChild(list);

    expect(scrollableAncestor(terminal)).toBe(list);
    const event = new WheelEvent("wheel", { deltaY: 30, cancelable: true });
    expect(forwardWheel(event, terminal)).toBe(true);
    expect(list.scrollTop).toBe(30);
    expect(event.defaultPrevented).toBe(true);
    list.remove();
  });
});
