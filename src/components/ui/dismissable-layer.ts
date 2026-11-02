/**
 * 浮层统一关闭：点击层外，或窗口失焦 / 页面隐藏时调用 onDismiss。
 * 供 UiSelect、TagPicker、ContextMenu、建议菜单等 Teleport 浮层复用。
 */
export type DismissableLayerOptions = {
  /** 点击落在这些根节点内不算「外部」 */
  contains: (target: Node) => boolean;
  onDismiss: () => void;
  /** 默认 true：监听 pointerdown 捕获 */
  outsidePointer?: boolean;
  /** 默认 true：窗口 blur / 页面 hidden */
  loseFocus?: boolean;
};

export function bindDismissableLayer({
  contains,
  onDismiss,
  outsidePointer = true,
  loseFocus = true,
}: DismissableLayerOptions) {
  function onPointerDown(event: PointerEvent) {
    const target = event.target;
    if (!(target instanceof Node) || contains(target)) return;
    onDismiss();
  }

  function onWindowBlur() {
    onDismiss();
  }

  function onVisibilityChange() {
    if (document.visibilityState === "hidden") onDismiss();
  }

  if (outsidePointer) document.addEventListener("pointerdown", onPointerDown, true);
  if (loseFocus) {
    window.addEventListener("blur", onWindowBlur);
    document.addEventListener("visibilitychange", onVisibilityChange);
  }

  return () => {
    if (outsidePointer) document.removeEventListener("pointerdown", onPointerDown, true);
    if (loseFocus) {
      window.removeEventListener("blur", onWindowBlur);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    }
  };
}
