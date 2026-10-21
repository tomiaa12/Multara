/** Invisible pad so the caret sits in a text node instead of on a `<br>`. */
export const LINE_BREAK_PAD = "\u200b";

export function stripLineBreakPad(value: string) {
  return value.replaceAll(LINE_BREAK_PAD, "");
}

function insideField(root: HTMLElement, node: Node) {
  return node === root || root.contains(node);
}

/** How far the scrollport must move so the caret line sits fully inside it. */
export function caretScrollDelta(viewTop: number, viewBottom: number, caretTop: number, caretBottom: number) {
  if (caretBottom > viewBottom) return caretBottom - viewBottom;
  if (caretTop < viewTop) return caretTop - viewTop;
  return 0;
}

function lineBoxHeight(root: HTMLElement) {
  const style = getComputedStyle(root);
  const lineHeight = Number.parseFloat(style.lineHeight);
  if (Number.isFinite(lineHeight) && lineHeight > 0) return lineHeight;
  const fontSize = Number.parseFloat(style.fontSize);
  if (Number.isFinite(fontSize) && fontSize > 0) return fontSize;
  return 0;
}

function placeCaret(selection: Selection, pad: Text) {
  const caret = document.createRange();
  caret.setStart(pad, LINE_BREAK_PAD.length);
  caret.collapse(true);
  selection.removeAllRanges();
  selection.addRange(caret);
}

function revealCaret(root: HTMLElement, pad: Text) {
  const selection = window.getSelection();
  if (!selection) return;
  const probe = document.createElement("span");
  const height = lineBoxHeight(root);
  probe.style.display = "inline-block";
  probe.style.width = "0";
  probe.style.verticalAlign = "top";
  if (height > 0) probe.style.height = `${height}px`;
  pad.after(probe);
  const caret = probe.getBoundingClientRect();
  const view = root.getBoundingClientRect();
  const delta = caretScrollDelta(view.top, view.bottom, caret.top, caret.bottom);
  probe.remove();
  if (delta) root.scrollTop += delta;
  placeCaret(selection, pad);
}

/**
 * Shift+Enter line break.
 * WKWebView paints a second caret on the previous line when the selection
 * sits on a `<br>`. A zero-width pad after the break keeps a single caret.
 */
export function insertComposerLineBreak(root: HTMLElement) {
  const selection = window.getSelection();
  if (!selection?.rangeCount) return;
  const range = selection.getRangeAt(0);
  if (!insideField(root, range.commonAncestorContainer)) return;
  if (!range.collapsed) range.deleteContents();

  const br = document.createElement("br");
  const pad = document.createTextNode(LINE_BREAK_PAD);
  const fragment = document.createDocumentFragment();
  fragment.append(br, pad);
  range.insertNode(fragment);
  const trailing = pad.nextSibling;
  if (trailing?.nodeType === Node.TEXT_NODE && !(trailing.textContent ?? "").length) trailing.remove();

  revealCaret(root, pad);
}
