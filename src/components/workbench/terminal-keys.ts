const CTRL_A = String.fromCharCode("A".charCodeAt(0) - 64);
const CTRL_E = String.fromCharCode("E".charCodeAt(0) - 64);
const CTRL_U = "\u0015";
const CTRL_W = String.fromCharCode("W".charCodeAt(0) - 64);

export function isMacPlatform() {
  return /Mac|iPhone|iPad/.test(navigator.platform);
}

/**
 * VS Code `registerSendSequenceKeybinding` 里、终端获得焦点时发给 shell 的序列。
 * macOS 上 Option 是 Alt，Command 是 Meta。
 */
export function terminalSendSequence(event: KeyboardEvent, mac = isMacPlatform()): string | null {
  const alt = event.altKey;
  const shift = event.shiftKey;
  const cmd = mac ? event.metaKey : event.ctrlKey;
  const ctrl = event.ctrlKey;
  if (alt && shift) return null;
  if (event.key === "ArrowUp" && alt && !cmd && !shift) return "\x1b[1;5A";
  if (event.key === "ArrowDown" && alt && !cmd && !shift) return "\x1b[1;5B";
  if (event.key === "ArrowRight" && alt && !cmd && !shift) return mac ? "\x1bf" : "\x1b[1;5C";
  if (event.key === "ArrowLeft" && alt && !cmd && !shift) return mac ? "\x1bb" : "\x1b[1;5D";
  if (event.key === "Backspace" && !shift) {
    if (mac && alt && !event.metaKey && !ctrl) return CTRL_W;
    if (!mac && cmd && !alt) return CTRL_W;
    if (mac && event.metaKey && !alt && !ctrl) return CTRL_U;
  }
  if (event.key === "Delete" && !shift) {
    if (mac && alt && !event.metaKey && !ctrl) return "\u001bd";
    if (!mac && cmd && !alt) return "\u001bd";
  }
  if (mac && event.metaKey && !alt && !ctrl && !shift && event.key === "ArrowLeft") return CTRL_A;
  if (mac && event.metaKey && !alt && !ctrl && !shift && event.key === "ArrowRight") return CTRL_E;
  if (!mac && ctrl && shift && !alt && event.code === "Digit2") return "\u0000";
  if (mac && ctrl && shift && !event.metaKey && !alt && event.code === "Digit2") return "\u0000";
  if (!mac && ctrl && shift && !alt && event.code === "Digit6") return "\u001e";
  if (mac && ctrl && shift && !event.metaKey && !alt && event.code === "Digit6") return "\u001e";
  return null;
}

export type TerminalKeyCommand =
  | "copy"
  | "paste"
  | "find"
  | "clear"
  | "selectAll"
  | "previousCommand"
  | "nextCommand"
  | "scrollLineUp"
  | "scrollLineDown"
  | "scrollPageUp"
  | "scrollPageDown"
  | "scrollTop"
  | "scrollBottom"
  | "split"
  | "focusNext"
  | "focusPrevious"
  | "recentCommand"
  | "recentDirectory"
  | "suggest";

/** 终端获得焦点时由工作台处理、不发给 shell 的按键，对应 VS Code 的默认快捷键。 */
export function terminalKeyCommand(
  event: KeyboardEvent,
  options: { mac?: boolean; hasSelection?: boolean } = {},
): TerminalKeyCommand | null {
  const mac = options.mac ?? isMacPlatform();
  const { altKey: alt, shiftKey: shift, ctrlKey: ctrl, metaKey: meta, code, key } = event;
  const only = (want: { ctrl?: boolean; shift?: boolean; alt?: boolean; meta?: boolean }) =>
    ctrl === !!want.ctrl && shift === !!want.shift && alt === !!want.alt && meta === !!want.meta;

  if (mac) {
    if (only({ meta: true }) && code === "KeyC" && options.hasSelection) return "copy";
    if (only({ meta: true }) && code === "KeyV") return "paste";
    if (only({ meta: true }) && code === "KeyF") return "find";
    if (only({ meta: true }) && code === "KeyK") return "clear";
    if (only({ meta: true }) && code === "KeyA") return "selectAll";
    if (only({ meta: true }) && key === "ArrowUp") return "previousCommand";
    if (only({ meta: true }) && key === "ArrowDown") return "nextCommand";
    if (only({ meta: true, alt: true }) && key === "PageUp") return "scrollLineUp";
    if (only({ meta: true, alt: true }) && key === "PageDown") return "scrollLineDown";
    if (only({ meta: true }) && key === "Home") return "scrollTop";
    if (only({ meta: true }) && key === "End") return "scrollBottom";
    if (only({ meta: true }) && code === "Backslash") return "split";
    if (only({ meta: true, shift: true }) && code === "BracketRight") return "focusNext";
    if (only({ meta: true, shift: true }) && code === "BracketLeft") return "focusPrevious";
  } else {
    if (only({ ctrl: true }) && code === "KeyC" && options.hasSelection) return "copy";
    if (only({ ctrl: true, shift: true }) && code === "KeyC") return "copy";
    if (only({ ctrl: true }) && code === "KeyV") return "paste";
    if (only({ ctrl: true, shift: true }) && code === "KeyV") return "paste";
    if (only({ ctrl: true }) && code === "KeyF") return "find";
    if (only({ ctrl: true }) && key === "ArrowUp") return "previousCommand";
    if (only({ ctrl: true }) && key === "ArrowDown") return "nextCommand";
    if (only({ ctrl: true, shift: true }) && key === "ArrowUp") return "scrollLineUp";
    if (only({ ctrl: true, shift: true }) && key === "ArrowDown") return "scrollLineDown";
    if (only({ ctrl: true }) && key === "Home") return "scrollTop";
    if (only({ ctrl: true }) && key === "End") return "scrollBottom";
    if (only({ ctrl: true, shift: true }) && code === "Digit5") return "split";
    if (only({ ctrl: true }) && key === "PageDown") return "focusNext";
    if (only({ ctrl: true }) && key === "PageUp") return "focusPrevious";
  }
  if (only({ shift: true }) && key === "PageUp") return "scrollPageUp";
  if (only({ shift: true }) && key === "PageDown") return "scrollPageDown";
  if (only({ ctrl: true, alt: true }) && code === "KeyR") return "recentCommand";
  if (only({ ctrl: true }) && code === "KeyG") return "recentDirectory";
  if (only({ ctrl: true }) && code === "Space") return "suggest";
  return null;
}
