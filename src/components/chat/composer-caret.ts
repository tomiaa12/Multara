/** 当前选区折叠后的光标视口矩形；空行时退回所在节点。 */
export function getCaretClientRect(): DOMRect | null {
  const selection = window.getSelection();
  if (!selection?.rangeCount) return null;
  const range = selection.getRangeAt(0).cloneRange();
  range.collapse(true);
  const rects = range.getClientRects();
  if (rects.length) return rects[0] ?? null;
  const rect = range.getBoundingClientRect();
  if (rect.width || rect.height) return rect;
  const node = range.startContainer;
  const element = node.nodeType === Node.ELEMENT_NODE
    ? (node as Element)
    : node.parentElement;
  return element?.getBoundingClientRect() ?? null;
}
