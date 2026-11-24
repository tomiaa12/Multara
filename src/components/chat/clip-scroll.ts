/** 嵌套预览（终端输出）把滚轮交给外层可滚动容器，避免挡住回答列表。 */
export function scrollableAncestor(start: HTMLElement | null): HTMLElement | null {
  let node = start?.parentElement ?? null;
  while (node) {
    const overflow = getComputedStyle(node).overflowY;
    if (
      (overflow === "auto" || overflow === "scroll" || overflow === "overlay")
      && node.scrollHeight > node.clientHeight + 1
    ) {
      return node;
    }
    node = node.parentElement;
  }
  return null;
}

export function forwardWheel(event: WheelEvent, start: HTMLElement | null): boolean {
  if (event.ctrlKey || event.metaKey) return false;
  const parent = scrollableAncestor(start);
  if (!parent) return false;
  parent.scrollTop += event.deltaY;
  parent.scrollLeft += event.deltaX;
  event.preventDefault();
  return true;
}
