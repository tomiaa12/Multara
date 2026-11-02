/**
 * DiffEditor 强制 fixedOverflowWidgets；若挂在 .vs-scroll（transform: translateZ(0)）内，
 * position:fixed 会相对该容器定位，只读 tip / hover 会相对光标偏移。
 * 与 UiModal 一样挂到 body，避开 transform 包含块。
 */
let root: HTMLElement | null = null;

export function monacoOverflowWidgetsRoot(): HTMLElement {
  if (root?.isConnected) return root;
  root = document.createElement("div");
  // monaco-editor：吃到主题 CSS 变量与 .monaco-editor .monaco-editor-overlaymessage 样式
  root.className = "monaco-overflow-widgets-root monaco-editor";
  root.setAttribute("aria-hidden", "true");
  document.body.appendChild(root);
  return root;
}
