// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createApp, defineComponent, h, nextTick, ref, withDirectives } from "vue";
import {
  collapseTransition,
  motionEnabled,
  playEnter,
  playLeaveGhost,
  setMotionEnabled,
  vDetailsMotion,
  vMotion,
  type MotionValue,
} from "./motion";

function sized(el: HTMLElement, width = 120, height = 40) {
  el.getBoundingClientRect = () => ({ width, height, top: 0, left: 0, right: width, bottom: height, x: 0, y: 0, toJSON() {} }) as DOMRect;
  return el;
}

function mountToggle(value: MotionValue, style = "position: fixed") {
  const show = ref(true);
  const host = document.createElement("div");
  document.body.appendChild(host);
  const app = createApp(
    defineComponent({
      setup() {
        return () =>
          show.value
            ? withDirectives(
                h("div", {
                  class: "menu",
                  id: "menu",
                  style,
                  onVnodeMounted: (vnode) => sized(vnode.el as HTMLElement),
                }, "hi"),
                [[vMotion, value]],
              )
            : null;
      },
    }),
  );
  app.mount(host);
  return { show, host, app };
}

describe("motion", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
  });
  afterEach(() => {
    setMotionEnabled(null);
    vi.useRealTimers();
  });

  it("is off in tests unless forced on", () => {
    expect(motionEnabled()).toBe(false);
    setMotionEnabled(true);
    expect(motionEnabled()).toBe(true);
  });

  it("leaves an inert ghost in place of an unmounted fixed overlay, then removes it", async () => {
    setMotionEnabled(true);
    const { show, host } = mountToggle("dropdown");
    expect(host.querySelector(".menu")?.classList.contains("ds-motion-in-dropdown")).toBe(true);

    show.value = false;
    await nextTick();
    const ghost = host.querySelector<HTMLElement>(".ds-motion-ghost");
    expect(ghost).not.toBeNull();
    expect(ghost!.classList.contains("ds-motion-out-dropdown")).toBe(true);
    expect(ghost!.classList.contains("ds-motion-in-dropdown")).toBe(false);
    expect(ghost!.getAttribute("aria-hidden")).toBe("true");
    expect(ghost!.hasAttribute("inert")).toBe(true);
    expect(ghost!.id).toBe("");
    expect(host.querySelectorAll(".menu").length).toBe(1);

    ghost!.dispatchEvent(new Event("animationend"));
    expect(host.querySelector(".ds-motion-ghost")).toBeNull();
  });

  it("removes the ghost on a timer when animationend never fires", async () => {
    setMotionEnabled(true);
    const { show, host } = mountToggle({ leave: "fade" });
    show.value = false;
    await nextTick();
    expect(host.querySelector(".ds-motion-ghost")).not.toBeNull();
    vi.advanceTimersByTime(500);
    expect(host.querySelector(".ds-motion-ghost")).toBeNull();
  });

  it("skips the ghost for in-flow elements and when motion is off", async () => {
    setMotionEnabled(true);
    const inFlow = mountToggle("fade", "position: static");
    inFlow.show.value = false;
    await nextTick();
    expect(inFlow.host.querySelector(".ds-motion-ghost")).toBeNull();

    setMotionEnabled(false);
    const off = mountToggle("fade");
    expect(off.host.querySelector(".menu")?.className).toBe("menu");
    off.show.value = false;
    await nextTick();
    expect(off.host.querySelector(".ds-motion-ghost")).toBeNull();
  });

  it("enter-only values never leave a ghost", async () => {
    setMotionEnabled(true);
    const { show, host } = mountToggle({ enter: "page" });
    expect(host.querySelector(".menu")?.classList.contains("ds-motion-in-page")).toBe(true);
    show.value = false;
    await nextTick();
    expect(host.querySelector(".ds-motion-ghost")).toBeNull();
  });

  it("playEnter removes its class when the animation ends", () => {
    setMotionEnabled(true);
    const el = document.createElement("div");
    document.body.appendChild(el);
    playEnter(el, "pop");
    expect(el.classList.contains("ds-motion-in-pop")).toBe(true);
    el.dispatchEvent(new Event("animationend"));
    expect(el.classList.contains("ds-motion-in-pop")).toBe(false);
  });

  it("ghost keeps scroll position and typed input values", () => {
    setMotionEnabled(true);
    const el = sized(document.createElement("div"));
    el.style.position = "absolute";
    el.innerHTML = `<div class="list" style="overflow:auto"></div><input value="" />`;
    document.body.appendChild(el);
    (el.querySelector(".list") as HTMLElement).scrollTop = 30;
    (el.querySelector("input") as HTMLInputElement).value = "typed";
    playLeaveGhost(el, "fade");
    const ghost = document.querySelector<HTMLElement>(".ds-motion-ghost")!;
    expect((ghost.querySelector("input") as HTMLInputElement).value).toBe("typed");
    expect((ghost.querySelector(".list") as HTMLElement).scrollTop).toBe(30);
  });

  it("collapse hooks finish immediately when motion is off", () => {
    const hooks = collapseTransition();
    const done = vi.fn();
    hooks.onEnter(document.createElement("div"), done);
    hooks.onLeave(document.createElement("div"), done);
    expect(done).toHaveBeenCalledTimes(2);
    expect(hooks.css).toBe(false);
  });

  it("details keep native toggling when motion is off", () => {
    const details = document.createElement("details");
    details.innerHTML = "<summary>s</summary><p>body</p>";
    document.body.appendChild(details);
    vDetailsMotion.mounted!(details, {} as never, {} as never, null);
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    details.querySelector("summary")!.dispatchEvent(click);
    expect(click.defaultPrevented).toBe(false);
  });

  it("details animate instead of toggling natively when motion is on", () => {
    setMotionEnabled(true);
    const details = document.createElement("details");
    details.innerHTML = "<summary>s</summary><p>body</p>";
    const animate = vi.fn(() => ({ cancel() {}, onfinish: null, oncancel: null }) as unknown as Animation);
    details.animate = animate;
    document.body.appendChild(details);
    vDetailsMotion.mounted!(details, {} as never, {} as never, null);

    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    details.querySelector("summary")!.dispatchEvent(click);
    expect(click.defaultPrevented).toBe(true);
    expect(details.open).toBe(true);

    // summary 里的按钮不触发展开收起
    details.querySelector("summary")!.innerHTML = "<button>x</button>";
    const buttonClick = new MouseEvent("click", { bubbles: true, cancelable: true });
    details.querySelector("button")!.dispatchEvent(buttonClick);
    expect(buttonClick.defaultPrevented).toBe(false);
  });
});
