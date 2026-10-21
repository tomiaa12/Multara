import type { ITerminalOptions, ITheme } from "@xterm/xterm";

/**
 * xterm 只接受具体颜色值，不能直接用 CSS 变量；
 * 借助临时元素把设计令牌（可能是 color-mix 等表达式）解析成 rgb() 字符串。
 */
function resolveToken(name: string, fallback: string) {
  const probe = document.createElement("span");
  probe.style.color = `var(${name}, ${fallback})`;
  probe.style.display = "none";
  document.body.append(probe);
  const value = getComputedStyle(probe).color || fallback;
  probe.remove();
  return value;
}

/** 与 VS Code Dark+/Light+ 的终端 ANSI 调色板一致。 */
const DARK_ANSI = {
  black: "#000000",
  red: "#cd3131",
  green: "#0dbc79",
  yellow: "#e5e510",
  blue: "#2472c8",
  magenta: "#bc3fbc",
  cyan: "#11a8cd",
  white: "#e5e5e5",
  brightBlack: "#666666",
  brightRed: "#f14c4c",
  brightGreen: "#23d18b",
  brightYellow: "#f5f543",
  brightBlue: "#3b8eea",
  brightMagenta: "#d670d6",
  brightCyan: "#29b8db",
  brightWhite: "#e5e5e5",
};

const LIGHT_ANSI = {
  black: "#000000",
  red: "#cd3131",
  green: "#00bc00",
  yellow: "#949800",
  blue: "#0451a5",
  magenta: "#bc05bc",
  cyan: "#0598bc",
  white: "#555555",
  brightBlack: "#666666",
  brightRed: "#cd3131",
  brightGreen: "#14ce14",
  brightYellow: "#b5ba00",
  brightBlue: "#0451a5",
  brightMagenta: "#bc05bc",
  brightCyan: "#0598bc",
  brightWhite: "#a5a5a5",
};

export function isLightTheme() {
  return document.documentElement.dataset.theme === "light";
}

export function terminalTheme(): ITheme {
  return {
    ...(isLightTheme() ? LIGHT_ANSI : DARK_ANSI),
    // 与底部面板（.terminal-body）同色，避免终端区与面板之间出现色块
    background: resolveToken("--color-surface-raised", "#1e1e1e"),
    foreground: resolveToken("--color-text", "#cccccc"),
    cursor: resolveToken("--color-text-secondary", "#aeafad"),
    selectionBackground: resolveToken("--color-primary-subtle", "#264f78"),
    selectionInactiveBackground: resolveToken("--color-surface-hover", "#3a3d41"),
    overviewRulerBorder: "transparent",
  };
}

export const TERMINAL_FONT_SIZE_KEY = "multara.terminal.fontSize";
export const TERMINAL_DEFAULT_FONT_SIZE = 12;
export const TERMINAL_MIN_FONT_SIZE = 6;
export const TERMINAL_MAX_FONT_SIZE = 40;

export function terminalFontSize() {
  const saved = Number(localStorage.getItem(TERMINAL_FONT_SIZE_KEY));
  return saved >= TERMINAL_MIN_FONT_SIZE && saved <= TERMINAL_MAX_FONT_SIZE ? saved : TERMINAL_DEFAULT_FONT_SIZE;
}

export function terminalOptions(): ITerminalOptions {
  return {
    allowProposedApi: true,
    cursorBlink: true,
    fontFamily: "'Cascadia Mono', Menlo, Monaco, Consolas, 'Courier New', monospace",
    fontSize: terminalFontSize(),
    scrollback: 5000,
    minimumContrastRatio: 4.5,
    macOptionClickForcesSelection: true,
    altClickMovesCursor: true,
    overviewRuler: { width: 10 },
    theme: terminalTheme(),
  };
}
