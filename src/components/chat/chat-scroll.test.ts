// @vitest-environment happy-dom
import { describe, expect, test } from "vitest";
import { listIsAtBottom, stickListToBottom } from "./chat-scroll";

describe("listIsAtBottom", () => {
  test("treats absolute bottom as bottom even with short last content", () => {
    const list = document.createElement("div");
    Object.defineProperties(list, {
      scrollHeight: { value: 1000 },
      scrollTop: { value: 950 },
      clientHeight: { value: 40 },
      getBoundingClientRect: { value: () => ({ bottom: 400, top: 0, left: 0, right: 0, width: 0, height: 400 }) },
    });
    const exchange = document.createElement("section");
    exchange.className = "message-exchange";
    const content = document.createElement("div");
    content.getBoundingClientRect = () => ({ bottom: 80, top: 0, left: 0, right: 0, width: 0, height: 80 }) as DOMRect;
    exchange.append(content);
    list.append(exchange);
    expect(listIsAtBottom(list)).toBe(true);
  });

  test("stickListToBottom sets scrollTop to scrollHeight", () => {
    const list = document.createElement("div");
    Object.defineProperty(list, "scrollHeight", { value: 420 });
    list.scrollTop = 0;
    stickListToBottom(list);
    expect(list.scrollTop).toBe(420);
  });
});
