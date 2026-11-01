import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
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
