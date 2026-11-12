import { monaco } from "../monaco";
import { refreshMonacoTheme } from "./monaco-workspace";

const MAX_ATTEMPTS = 30;

function nextFrame() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => resolve());
  });
}

function fontTrusted(editor: monaco.editor.IStandaloneCodeEditor) {
  const info = editor.getOption(monaco.editor.EditorOption.fontInfo);
  return Boolean(info?.isTrusted) && Number(info?.typicalHalfwidthCharacterWidth) > 2;
}

/**
 * Monaco 第一次量字体时，WebView 常给出接近 0 的字宽并缓存为不可信。
 * 不可信的结果要等 5 秒才丢弃，这期间字形会叠在同一行上。
 * 容器有真实尺寸后再量，量不到就清掉缓存立刻重试，量准之前不要把编辑器露出来。
 */
export async function settleMonacoEditor(
  editor: monaco.editor.IStandaloneCodeEditor,
  host: HTMLElement,
  isCurrent: () => boolean = () => true,
): Promise<void> {
  const fontsReady = document.fonts?.ready ?? Promise.resolve();
  await fontsReady.catch(() => undefined);
  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt += 1) {
    if (!isCurrent()) return;
    if (host.clientWidth > 0 && host.clientHeight > 0) {
      if (!fontTrusted(editor)) monaco.editor.remeasureFonts();
      editor.layout();
      if (fontTrusted(editor)) break;
    }
    await nextFrame();
  }
  if (!isCurrent()) return;
  if (!fontTrusted(editor)) {
    await new Promise<void>((resolve) => {
      let settled = false;
      const timer = window.setTimeout(finish, 5000);
      const sub = editor.onDidChangeConfiguration((event) => {
        if (event.hasChanged(monaco.editor.EditorOption.fontInfo)) finish();
      });
      function finish() {
        if (settled) return;
        settled = true;
        window.clearTimeout(timer);
        sub.dispose();
        resolve();
      }
    });
    if (!isCurrent()) return;
    if (!fontTrusted(editor)) monaco.editor.remeasureFonts();
    editor.layout();
  }
  if (!isCurrent()) return;
  refreshMonacoTheme();
  editor.layout();
}

export async function settleDiffEditor(
  editor: monaco.editor.IStandaloneDiffEditor,
  host: HTMLElement,
  isCurrent: () => boolean = () => true,
): Promise<void> {
  await settleMonacoEditor(editor.getModifiedEditor(), host, isCurrent);
  if (!isCurrent()) return;
  editor.layout();
}
