import type { Terminal } from "@xterm/xterm";

/** xterm 私有渲染尺寸；WebGL 下没有 `.xterm-rows`，对齐 VS Code SuggestAddon。 */
type XtermWithCore = Terminal & {
  _core?: {
    _renderService?: {
      dimensions?: {
        css?: {
          cell?: { width: number; height: number };
        };
      };
    };
  };
};

export function terminalScreenElement(element: HTMLElement): HTMLElement | null {
  const screen = element.querySelector(".xterm-screen");
  // 不用 instanceof HTMLElement：node 单测环境没有 DOM 全局
  if (!screen || typeof (screen as HTMLElement).getBoundingClientRect !== "function") return null;
  return screen as HTMLElement;
}

/**
 * 单个 cell 的 CSS 像素宽高。
 * 优先用 renderService（WebGL / canvas 都有）；再退回按 `.xterm-screen` 均分。
 */
export function terminalCellSize(
  term: Terminal,
  element: HTMLElement,
): { width: number; height: number } | null {
  const cell = (term as XtermWithCore)._core?._renderService?.dimensions?.css?.cell;
  if (cell && cell.width > 0 && cell.height > 0) {
    return { width: cell.width, height: cell.height };
  }
  if (!term.rows || !term.cols) return null;
  const screen = element.querySelector(".xterm-screen") as HTMLElement | null;
  if (!screen || !(screen.clientWidth > 0) || !(screen.clientHeight > 0)) return null;
  const width = screen.clientWidth / term.cols;
  const height = screen.clientHeight / term.rows;
  if (!(width > 0 && height > 0)) return null;
  return { width, height };
}
