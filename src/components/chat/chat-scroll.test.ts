// @vitest-environment happy-dom
import { describe, expect, test, vi } from "vitest";
import {
  afterProgrammaticStick,
  listContentHeight,
  listIsAtBottom,
  pinLastExchangeToTop,
  SMOOTH_SCROLL_FOLLOW_SUPPRESS_MS,
  stickListToBottom,
  stickyUserTurnPushOffsets,
  syncStickyExchangeMinHeight,
} from "./chat-scroll";

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

describe("syncStickyExchangeMinHeight", () => {
  test("only the last exchange gets the content-box height", () => {
    const list = document.createElement("div");
    Object.defineProperty(list, "clientHeight", { value: 480 });
    list.style.paddingTop = "16px";
    list.style.paddingBottom = "16px";
    const first = document.createElement("section");
    first.className = "message-exchange";
    first.style.minHeight = "480px";
    const last = document.createElement("section");
    last.className = "message-exchange";
    list.append(first, last);
    document.body.append(list);

    expect(listContentHeight(list)).toBe(448);
    expect(syncStickyExchangeMinHeight(list, true)).toBe(last);
    expect(first.style.minHeight).toBe("");
    expect(last.style.minHeight).toBe("448px");

    syncStickyExchangeMinHeight(list, false);
    expect(last.style.minHeight).toBe("");
    list.remove();
  });
});

describe("pinLastExchangeToTop", () => {
  test("only sticks to bottom without delta overscroll", () => {
    const list = document.createElement("div");
    Object.defineProperties(list, {
      scrollHeight: { value: 900, configurable: true },
      clientHeight: { value: 400 },
    });
    list.scrollTop = 100;
    const last = document.createElement("section");
    // 即使 last 视觉上高于视口，也不再减 scrollTop
    last.getBoundingClientRect = () => ({ top: 40, bottom: 440, left: 0, right: 0, width: 0, height: 400 }) as DOMRect;
    list.getBoundingClientRect = () => ({ top: 100, bottom: 500, left: 0, right: 0, width: 0, height: 400 }) as DOMRect;
    pinLastExchangeToTop(list, last);
    expect(list.scrollTop).toBe(900);
  });
});

describe("stickyUserTurnPushOffsets", () => {
  test("next sticky pushes previous fully above when it reaches the top", () => {
    expect(
      stickyUserTurnPushOffsets(100, [
        { height: 60, top: 100 },
        { height: 20, top: 100 },
      ]),
    ).toEqual([-60, 0]);
  });

  test("partially overlapping next sticky eases previous upward", () => {
    expect(
      stickyUserTurnPushOffsets(100, [
        { height: 60, top: 100 },
        { height: 20, top: 140 },
      ]),
    ).toEqual([-20, 0]);
  });

  test("leaves previous at top while next is still below its height", () => {
    expect(
      stickyUserTurnPushOffsets(100, [
        { height: 60, top: 100 },
        { height: 20, top: 180 },
      ]),
    ).toEqual([0, 0]);
  });
});

describe("afterProgrammaticStick", () => {
  test("auto releases after two animation frames", async () => {
    const list = document.createElement("div");
    let done = false;
    afterProgrammaticStick(list, "auto", () => {
      done = true;
    });
    expect(done).toBe(false);
    await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    expect(done).toBe(true);
  });

  test("smooth waits for scrollend instead of two frames", async () => {
    const list = document.createElement("div");
    let done = false;
    afterProgrammaticStick(list, "smooth", () => {
      done = true;
    });
    await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
    expect(done).toBe(false);
    list.dispatchEvent(new Event("scrollend"));
    expect(done).toBe(true);
  });

  test("smooth falls back after timeout when scrollend is missing", async () => {
    vi.useFakeTimers();
    const list = document.createElement("div");
    let done = false;
    afterProgrammaticStick(list, "smooth", () => {
      done = true;
    });
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(SMOOTH_SCROLL_FOLLOW_SUPPRESS_MS);
    expect(done).toBe(true);
    vi.useRealTimers();
  });
});
