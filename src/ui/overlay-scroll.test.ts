// @vitest-environment happy-dom
import { afterEach, describe, expect, test, vi } from "vitest";
import {
  BAR_CLASS,
  HOST_CLASS,
  MINIMUM_SLIDER_SIZE,
  attachOverlayScroll,
  attachOverlayScrollTree,
  canAttachOverlayScroll,
  detachOverlayScrollTree,
  overlaySliderMetrics,
  startOverlayScrollObserver,
  stopOverlayScrollObserver,
} from "./overlay-scroll";

afterEach(() => {
  stopOverlayScrollObserver();
  detachOverlayScrollTree(document.body);
  document.body.replaceChildren();
});

describe("overlaySliderMetrics", () => {
  test("hides when content fits the viewport", () => {
    expect(overlaySliderMetrics(100, 80, 0)).toEqual({
      needed: false,
      size: 0,
      position: 0,
      ratio: 0,
    });
    expect(overlaySliderMetrics(100, 100, 0).needed).toBe(false);
  });

  test("keeps the slider large enough to grab", () => {
    expect(overlaySliderMetrics(100, 10_000, 0).size).toBe(MINIMUM_SLIDER_SIZE);
  });

  test("maps scroll position to the slider travel", () => {
    const start = overlaySliderMetrics(100, 200, 0);
    const end = overlaySliderMetrics(100, 200, 100);
    expect(start.needed).toBe(true);
    expect(start.position).toBe(0);
    expect(end.position + end.size).toBe(100);
    expect(end.ratio).toBe((100 - end.size) / 100);
  });

  test("uses real client size for travel when track reserves a corner", () => {
    // 视口 100、内容 200 → 真实行程 100；轨道因竖条预留只剩 90
    const end = overlaySliderMetrics(100, 200, 100, 90);
    expect(end.needed).toBe(true);
    expect(end.position + end.size).toBe(90);
    expect(end.ratio).toBe((90 - end.size) / 100);
  });
});

describe("canAttachOverlayScroll", () => {
  test("skips editors, form fields and contenteditable hosts", () => {
    const host = document.createElement("div");
    host.className = HOST_CLASS;
    expect(canAttachOverlayScroll(host)).toBe(true);

    const field = document.createElement("div");
    field.className = HOST_CLASS;
    field.contentEditable = "true";
    expect(canAttachOverlayScroll(field)).toBe(false);

    const area = document.createElement("textarea");
    area.className = HOST_CLASS;
    expect(canAttachOverlayScroll(area)).toBe(false);

    const pre = document.createElement("pre");
    pre.className = HOST_CLASS;
    expect(canAttachOverlayScroll(pre)).toBe(false);

    const monaco = document.createElement("div");
    monaco.className = "monaco-editor";
    const inner = document.createElement("div");
    inner.className = HOST_CLASS;
    monaco.appendChild(inner);
    expect(canAttachOverlayScroll(inner)).toBe(false);
  });
});

describe("attachOverlayScroll", () => {
  test("inserts overlay tracks and removes them on dispose", () => {
    const el = document.createElement("div");
    el.className = HOST_CLASS;
    document.body.appendChild(el);
    const dispose = attachOverlayScroll(el);
    expect(el.querySelectorAll(`.${BAR_CLASS}`)).toHaveLength(2);
    expect(el.getAttribute("data-overlay-scroll")).toBe("");
    dispose();
    expect(el.querySelectorAll(`.${BAR_CLASS}`)).toHaveLength(0);
    expect(el.hasAttribute("data-overlay-scroll")).toBe(false);
  });

  test("does not attach twice", () => {
    const el = document.createElement("div");
    document.body.appendChild(el);
    attachOverlayScroll(el);
    attachOverlayScroll(el);
    expect(el.querySelectorAll(`.${BAR_CLASS}`)).toHaveLength(2);
  });

  test("leaves contenteditable hosts untouched", () => {
    const el = document.createElement("div");
    el.contentEditable = "true";
    el.className = HOST_CLASS;
    document.body.appendChild(el);
    attachOverlayScroll(el);
    expect(el.querySelector(`.${BAR_CLASS}`)).toBeNull();
  });
});

describe("startOverlayScrollObserver", () => {
  test("attaches when a vs-scroll node appears", async () => {
    startOverlayScrollObserver(document.body);
    const el = document.createElement("div");
    el.className = HOST_CLASS;
    document.body.appendChild(el);
    await vi.waitFor(() => {
      expect(el.querySelector(`.${BAR_CLASS}`)).toBeTruthy();
    });
  });

  test("attaches existing hosts in a tree", () => {
    const el = document.createElement("div");
    el.className = HOST_CLASS;
    document.body.appendChild(el);
    attachOverlayScrollTree(document.body);
    expect(el.querySelectorAll(`.${BAR_CLASS}`)).toHaveLength(2);
  });
});
