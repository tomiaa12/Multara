export type ExplorerKeyAction =
  | "rename"
  | "delete"
  | "delete-permanent"
  | "open"
  | "open-preserve-focus"
  | "open-side"
  | "copy-path"
  | "copy-relative"
  | "reveal-os"
  | "copy"
  | "cut"
  | "paste"
  | "undo"
  | "redo"
  | "cancel-cut"
  | "select-all"
  | "find"
  | "collapse-all"
  | "left"
  | "right"
  | "next"
  | "previous"
  | "first"
  | "last"
  | "page-down"
  | "page-up"
  | "chord-k";

type KeyState = {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
};

export interface ExplorerKeyResult {
  action: ExplorerKeyAction;
  /** Shift+方向键：扩展选择。 */
  extend?: boolean;
}

/**
 * VS Code 资源管理器（files.contribution / listCommands）在各平台的默认键位。
 * `chordK` 表示上一键是 Ctrl/Cmd+K（用于 Ctrl+K Ctrl+Shift+C 复制相对路径）。
 */
export function explorerKey(
  event: KeyState,
  options: { isMac: boolean; chordK?: boolean },
): ExplorerKeyResult | null {
  const { isMac } = options;
  const key = event.key;
  const lower = key.length === 1 ? key.toLowerCase() : key;
  const primary = isMac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  const noMods = !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey;
  const only = (mods: { primary?: boolean; alt?: boolean; shift?: boolean }) =>
    Boolean(mods.primary) === primary &&
    Boolean(mods.alt) === event.altKey &&
    Boolean(mods.shift) === event.shiftKey &&
    (mods.primary ? true : !event.metaKey && !event.ctrlKey);

  if (options.chordK && only({ primary: true, shift: true }) && lower === "c") return { action: "copy-relative" };

  if (noMods && key === "F2") return { action: "rename" };
  if (isMac && noMods && key === "Enter") return { action: "rename" };

  if (!isMac && noMods && key === "Delete") return { action: "delete" };
  if (!isMac && only({ shift: true }) && key === "Delete") return { action: "delete-permanent" };
  if (isMac && only({ primary: true }) && key === "Backspace") return { action: "delete" };
  if (isMac && only({ primary: true, alt: true }) && key === "Backspace") return { action: "delete-permanent" };
  if (isMac && noMods && key === "Delete") return { action: "delete" };

  if (isMac ? only({ primary: true, alt: true }) && lower === "c" : only({ shift: true, alt: true }) && lower === "c") {
    return { action: "copy-path" };
  }
  if (only({ primary: true, alt: true, shift: true }) && lower === "c") return { action: "copy-relative" };
  if (isMac ? only({ primary: true, alt: true }) && lower === "r" : only({ shift: true, alt: true }) && lower === "r") {
    return { action: "reveal-os" };
  }
  if (only({ primary: true }) && lower === "k") return { action: "chord-k" };

  if (only({ primary: true }) && lower === "c") return { action: "copy" };
  if (only({ primary: true }) && lower === "x") return { action: "cut" };
  if (only({ primary: true }) && lower === "v") return { action: "paste" };
  if (only({ primary: true }) && lower === "z") return { action: "undo" };
  if (only({ primary: true, shift: true }) && lower === "z") return { action: "redo" };
  if (!isMac && only({ primary: true }) && lower === "y") return { action: "redo" };
  if (only({ primary: true }) && lower === "a") return { action: "select-all" };
  if (only({ primary: true, alt: true }) && lower === "f") return { action: "find" };
  if (noMods && key === "Escape") return { action: "cancel-cut" };

  if (isMac && only({ primary: true }) && key === "ArrowDown") return { action: "open" };
  if (!isMac && noMods && key === "Enter") return { action: "open" };
  if (only({ primary: true }) && key === "Enter") return { action: "open-side" };
  if (noMods && key === " ") return { action: "open-preserve-focus" };

  if (only({ primary: true }) && key === "ArrowLeft") return { action: "collapse-all" };
  if (isMac && only({ primary: true }) && key === "ArrowUp") return { action: "left" };
  if (noMods && key === "ArrowLeft") return { action: "left" };
  if (noMods && key === "ArrowRight") return { action: "right" };

  const extend = event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey;
  if (noMods || extend) {
    if (key === "ArrowDown") return { action: "next", extend };
    if (key === "ArrowUp") return { action: "previous", extend };
    if (key === "Home") return { action: "first", extend };
    if (key === "End") return { action: "last", extend };
    if (key === "PageDown") return { action: "page-down", extend };
    if (key === "PageUp") return { action: "page-up", extend };
  }
  return null;
}

export function explorerShortcutLabels(isMac: boolean) {
  return {
    rename: isMac ? "Enter" : "F2",
    delete: isMac ? "⌘⌫" : "Del",
    deletePermanent: isMac ? "⌥⌘⌫" : "Shift+Del",
    copyPath: isMac ? "⌥⌘C" : "Shift+Alt+C",
    copyRelative: isMac ? "⌥⇧⌘C" : "Ctrl+K Ctrl+Shift+C",
    reveal: isMac ? "⌥⌘R" : "Shift+Alt+R",
    cut: isMac ? "⌘X" : "Ctrl+X",
    copy: isMac ? "⌘C" : "Ctrl+C",
    paste: isMac ? "⌘V" : "Ctrl+V",
    undo: isMac ? "⌘Z" : "Ctrl+Z",
    redo: isMac ? "⇧⌘Z" : "Ctrl+Y",
    openSide: isMac ? "⌘Enter" : "Ctrl+Enter",
    find: isMac ? "⌥⌘F" : "Ctrl+Alt+F",
    findInFolder: isMac ? "⌥⇧F" : "Shift+Alt+F",
  };
}
