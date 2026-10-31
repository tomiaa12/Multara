/** DOM LiveTranscriptInserter for ChatComposerField (VS Code Monaco counterpart). */

const DICTATION_ATTR = "data-dictation";
const INTERIM_CLASS = "dictation-interim";
const MAX_SESSION_MS = 20 * 60 * 1000;

/** 把听写 span 展开成普通文本（含灰色的未定稿子节点），文字保留。 */
function unwrapMarker(marker: HTMLElement) {
  const parent = marker.parentNode;
  if (!parent) return;
  const text = marker.textContent ?? "";
  marker.replaceWith(document.createTextNode(text));
  parent.normalize();
}

const CJK_LANG_RE = /^(zh|ja|ko)([-_]|$)/i;

export type ComposerInserterHost = {
  getEditor: () => HTMLElement | null;
  notifyChange: () => void;
  isComposing: () => boolean;
};

export type ComposerInserter = {
  update: (text: string, finalizedText: string) => void;
  flushPending: () => void;
  finish: () => void;
  dispose: () => void;
  readonly userModified: boolean;
  readonly startedAt: number;
};

function needsLeadingSpace(before: string, language: string): boolean {
  if (!before) return false;
  if (CJK_LANG_RE.test(language)) return false;
  const ch = before.at(-1);
  if (!ch) return false;
  return !/\s/.test(ch);
}

function charBefore(root: HTMLElement, marker: HTMLElement): string {
  const range = document.createRange();
  range.setStart(root, 0);
  range.setEndBefore(marker);
  const text = range.toString().replace(/\u00a0/g, " ");
  return text.at(-1) ?? "";
}

function placeCaretEnd(node: Node) {
  const selection = window.getSelection();
  if (!selection) return;
  const range = document.createRange();
  range.selectNodeContents(node);
  range.collapse(false);
  selection.removeAllRanges();
  selection.addRange(range);
}

function ensureAnchor(root: HTMLElement): HTMLElement {
  const existing = root.querySelector<HTMLElement>(`span[${DICTATION_ATTR}]`);
  if (existing) return existing;

  const selection = window.getSelection();
  const marker = document.createElement("span");
  marker.setAttribute(DICTATION_ATTR, "1");

  if (selection?.rangeCount && root.contains(selection.anchorNode)) {
    const range = selection.getRangeAt(0);
    const chip = (range.startContainer instanceof Element
      ? range.startContainer
      : range.startContainer.parentElement)?.closest(".composer-chip");
    if (chip && root.contains(chip)) {
      const after = document.createTextNode("\u00a0");
      chip.after(after);
      range.setStartAfter(after);
      range.collapse(true);
    }
    range.insertNode(marker);
  } else {
    root.append(marker);
  }
  return marker;
}

export function createComposerInserter(
  host: ComposerInserterHost,
  options: { language: string },
): ComposerInserter {
  let committedText = "";
  let userModified = false;
  let pending: { text: string; finalizedText: string } | null = null;
  let listening = true;
  let applying = false;
  const startedAt = Date.now();

  const onInput = () => {
    if (applying || !listening || userModified) return;
    const root = host.getEditor();
    const marker = root?.querySelector<HTMLElement>(`span[${DICTATION_ATTR}]`);
    if (!marker || !root?.contains(marker)) return;
    committedText = marker.textContent ?? "";
    userModified = true;
    unwrapMarker(marker);
    host.notifyChange();
  };

  function render(text: string, finalizedText: string) {
    const root = host.getEditor();
    if (!root) return;
    applying = true;
    try {
      if (userModified) {
        if (!text.startsWith(committedText)) return;
        const delta = text.slice(committedText.length);
        if (!delta) return;
        const selection = window.getSelection();
        if (selection?.rangeCount && root.contains(selection.anchorNode)) {
          const range = selection.getRangeAt(0);
          range.deleteContents();
          const node = document.createTextNode(delta);
          range.insertNode(node);
          placeCaretEnd(node);
        } else {
          root.append(document.createTextNode(delta));
        }
        committedText = text;
        host.notifyChange();
        return;
      }

      const marker = ensureAnchor(root);
      const lead = needsLeadingSpace(charBefore(root, marker), options.language) ? " " : "";
      const finalPart = finalizedText.slice(0, text.length);
      const interimPart = text.slice(finalPart.length);

      marker.replaceChildren();
      if (lead) marker.append(document.createTextNode(lead));
      if (finalPart) marker.append(document.createTextNode(finalPart));
      if (interimPart) {
        const interim = document.createElement("span");
        interim.className = INTERIM_CLASS;
        interim.textContent = interimPart;
        marker.append(interim);
      }
      placeCaretEnd(marker);
      host.notifyChange();
    } finally {
      applying = false;
    }
  }

  function update(text: string, finalizedText: string) {
    if (!listening) return;
    if (host.isComposing()) {
      pending = { text, finalizedText };
      return;
    }
    pending = null;
    render(text, finalizedText);
  }

  function flushPending() {
    if (!pending) return;
    const next = pending;
    pending = null;
    render(next.text, next.finalizedText);
  }

  function finish() {
    if (!listening) return;
    listening = false;
    flushPending();
    const root = host.getEditor();
    const marker = root?.querySelector<HTMLElement>(`span[${DICTATION_ATTR}]`);
    if (marker) {
      applying = true;
      try {
        const plain = document.createTextNode(marker.textContent ?? "");
        marker.replaceWith(plain);
        placeCaretEnd(plain);
        host.notifyChange();
      } finally {
        applying = false;
      }
    }
    root?.removeEventListener("input", onInput);
  }

  /** 出错 / 取消：不再更新，但已写入的文字保留，并去掉听写标记（否则下一次会复用并覆盖它）。 */
  function dispose() {
    listening = false;
    pending = null;
    const root = host.getEditor();
    root?.removeEventListener("input", onInput);
    const marker = root?.querySelector<HTMLElement>(`span[${DICTATION_ATTR}]`);
    if (marker) {
      applying = true;
      try {
        unwrapMarker(marker);
        host.notifyChange();
      } finally {
        applying = false;
      }
    }
  }

  host.getEditor()?.addEventListener("input", onInput);

  return {
    update,
    flushPending,
    finish,
    dispose,
    get userModified() {
      return userModified;
    },
    startedAt,
  };
}

export const DICTATION_MAX_SESSION_MS = MAX_SESSION_MS;
