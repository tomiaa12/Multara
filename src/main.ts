import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";

function isEditableTarget(target: EventTarget | null) {
  return (
    target instanceof Element
    && Boolean(target.closest("input, textarea, select, [contenteditable='true'], [contenteditable='']"))
  );
}

function clientPointInRange(range: Range, x: number, y: number) {
  for (const rect of range.getClientRects()) {
    if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) return true;
  }
  return false;
}

/**
 * macOS / WKWebView：右键会先划选光标下的词（供「查询 / 翻译」）。
 * preventDefault 拦不住划词，需再清掉；若用户已拖选且点在选区内则保留。
 */
document.addEventListener(
  "mousedown",
  (event) => {
    if (event.button !== 2 || isEditableTarget(event.target)) return;
    event.preventDefault();

    const selection = window.getSelection();
    const preserved =
      selection
      && !selection.isCollapsed
      && selection.rangeCount > 0
      && clientPointInRange(selection.getRangeAt(0), event.clientX, event.clientY)
        ? selection.getRangeAt(0).cloneRange()
        : null;

    const restore = () => {
      const next = window.getSelection();
      if (!next) return;
      next.removeAllRanges();
      if (preserved) next.addRange(preserved);
    };
    restore();
    queueMicrotask(restore);
    requestAnimationFrame(restore);
  },
  true,
);

/**
 * 禁用系统默认右键菜单（含 macOS「查询 / 翻译」）。
 * 自定义 ContextMenu 仍走各处的 @contextmenu；输入框保留系统菜单以便复制粘贴。
 */
document.addEventListener(
  "contextmenu",
  (event) => {
    if (isEditableTarget(event.target)) return;
    event.preventDefault();
  },
  true,
);
import Codicon from "./components/ui/Codicon.vue";
import UiButton from "./components/ui/UiButton.vue";
import UiInput from "./components/ui/UiInput.vue";
import UiPane from "./components/ui/UiPane.vue";
import UiPaneView from "./components/ui/UiPaneView.vue";
import UiSelect from "./components/ui/UiSelect.vue";
import UiShimmerText from "./components/ui/UiShimmerText.vue";
import UiPathText from "./components/ui/UiPathText.vue";
import UiRelativeTime from "./components/ui/UiRelativeTime.vue";
import UiCountBadge from "./components/ui/UiCountBadge.vue";
import UiDirectoryTable from "./components/ui/UiDirectoryTable.vue";
import UiTooltip from "./components/ui/UiTooltip.vue";
import UiNotice from "./components/ui/UiNotice.vue";
import UiModal from "./components/ui/UiModal.vue";
import UiTextarea from "./components/ui/UiTextarea.vue";
import { router } from "./router";
import { overlayScrollPlugin } from "./ui/overlay-scroll";
import { applyThemeSetting, THEME_STORAGE_KEY } from "./theme";
import "./design/tokens.css";
import "./design/base.css";
import "./design/components.css";
import "./design/layout.css";
import "./styles.css";
import "./assets/diff-overlay.css";

// CSS 加载后再次对齐主题，避免只有 :root 深色规则生效。
try {
  applyThemeSetting(localStorage.getItem(THEME_STORAGE_KEY));
} catch {
  applyThemeSetting("system");
}

if (import.meta.env.DEV) {
  void import("./e2e/devBridge");
}

createApp(App)
  .use(overlayScrollPlugin)
  .component("Codicon", Codicon)
  .component("UiButton", UiButton)
  .component("UiInput", UiInput)
  .component("UiPane", UiPane)
  .component("UiPaneView", UiPaneView)
  .component("UiSelect", UiSelect)
  .component("UiShimmerText", UiShimmerText)
  .component("UiPathText", UiPathText)
  .component("UiRelativeTime", UiRelativeTime)
  .component("UiCountBadge", UiCountBadge)
  .component("UiDirectoryTable", UiDirectoryTable)
  .component("UiTooltip", UiTooltip)
  .component("UiTextarea", UiTextarea)
  .component("UiNotice", UiNotice)
  .component("UiModal", UiModal)
  .use(createPinia())
  .use(router)
  .mount("#app");
