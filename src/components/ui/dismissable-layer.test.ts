// @vitest-environment happy-dom
import { describe, expect, it, vi } from "vitest";
import { bindDismissableLayer } from "./dismissable-layer";

describe("bindDismissableLayer", () => {
  it("点击外部与窗口失焦时关闭，层内点击不关闭", () => {
    const root = document.createElement("div");
    document.body.appendChild(root);
    const onDismiss = vi.fn();
    const unbind = bindDismissableLayer({
      contains: (target) => root.contains(target),
      onDismiss,
    });

    root.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(onDismiss).not.toHaveBeenCalled();

    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(onDismiss).toHaveBeenCalledTimes(1);

    window.dispatchEvent(new Event("blur"));
    expect(onDismiss).toHaveBeenCalledTimes(2);

    unbind();
    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(onDismiss).toHaveBeenCalledTimes(2);
    root.remove();
  });

  it("可关闭 outsidePointer / loseFocus", () => {
    const onDismiss = vi.fn();
    const unbind = bindDismissableLayer({
      contains: () => false,
      onDismiss,
      outsidePointer: false,
      loseFocus: false,
    });

    document.body.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    window.dispatchEvent(new Event("blur"));
    expect(onDismiss).not.toHaveBeenCalled();
    unbind();
  });
});
