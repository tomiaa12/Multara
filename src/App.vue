<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch, type Component } from "vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { lastProjectsPath, rememberProjectsPath } from "./views/dashboard/last-projects-path";
import {
  BarChart3,
  Bot,
  Boxes,
  ChevronLeft,
  ChevronRight,
  Copy,
  FolderKanban,
  Inbox,
  ListTodo,
  MessagesSquare,
  Minus,
  Moon,
  Network,
  Palette,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  Sparkles,
  Square,
  Sun,
  Users,
  Workflow,
  X,
} from "@lucide/vue";
import { titlebarDetail } from "./stores/titlebar";
import { backTarget, crumbs, markMenuNavigation } from "./stores/breadcrumbs";
import FloatingChat from "./components/FloatingChat.vue";
import UpdateNotification from "./components/UpdateNotification.vue";
import { RunLogHost } from "./components/run-log";
import UnsavedChangesDialog from "./components/workbench/UnsavedChangesDialog.vue";
import {
  confirmLeaveUnsavedEditors,
  finishUnsavedPrompt,
  getUnsavedPrompt,
  hasUnsavedEditors,
  onUnsavedPromptChange,
} from "./components/workbench/unsaved-editor";
import { useAppStore } from "./stores/app";
import { useEditorStatusStore } from "./stores/editor-status";
import EditorLanguagePicker from "./components/workbench/EditorLanguagePicker.vue";
import ContextMenu, { type ContextMenuItem } from "./components/ui/ContextMenu.vue";
import { editorZoomLabel } from "./components/workbench/monaco-zoom";
import { isWorkbenchPath } from "./router";
import { t } from "./i18n";
import { unreadBadgeLabel, unreadTotal } from "./domain/inbox";
import SideNav, { type SideNavItem } from "./components/SideNav.vue";
import TerminalActivity from "./components/workbench/TerminalActivity.vue";
import { startRunningTerminalWatch } from "./stores/running-terminals";

const store = useAppStore();
const editorStatus = useEditorStatusStore();
const route = useRoute();
const router = useRouter();
const languageButton = ref<HTMLButtonElement | null>(null);
const languagePickerOpen = ref(false);
const languageAnchor = ref<DOMRect | null>(null);
const showEditorLanguage = computed(
  () => isWorkbenchPath(route.path) && Boolean(editorStatus.languageLabel),
);
const showEditorStatus = computed(
  () => isWorkbenchPath(route.path) && Boolean(editorStatus.fileKey),
);
const statusMenu = ref<{ x: number; y: number; kind: "indent" | "eol" } | null>(null);
const indentMenuItems: ContextMenuItem[] = [
  { id: "spaces-2", label: t("indent.spaces", { n: 2 }) },
  { id: "spaces-4", label: t("indent.spaces", { n: 4 }) },
  { id: "spaces-8", label: t("indent.spaces", { n: 8 }) },
  { type: "sep" },
  { id: "tabs-4", label: t("indent.tabs", { n: 4 }) },
];
const eolMenuItems: ContextMenuItem[] = [
  { id: "LF", label: "LF" },
  { id: "CRLF", label: "CRLF" },
];
const backCrumb = computed(() => backTarget());

function goBack() {
  if (backCrumb.value) void router.push(backCrumb.value.fullPath);
}
const SIDEBAR_COLLAPSED_KEY = "multara.sidebar-collapsed";
const SIDEBAR_WIDTH_KEY = "multara.sidebar-width";
/** 收起态固定宽（与 layout.css 一致） */
const SIDEBAR_COLLAPSED_WIDTH = 56;
const SIDEBAR_DEFAULT_WIDTH = 208;
const SIDEBAR_MIN_WIDTH = 160;
const SIDEBAR_MAX_WIDTH = 400;
/** 拖到此宽度以下视为收起 */
const SIDEBAR_COLLAPSE_THRESHOLD = 120;
/** 收起态向右拖过此宽度则展开 */
const SIDEBAR_EXPAND_THRESHOLD = 88;

function clampSidebarWidth(width: number) {
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, Math.round(width)));
}

function readStoredSidebarWidth() {
  const raw = Number(localStorage.getItem(SIDEBAR_WIDTH_KEY));
  if (!Number.isFinite(raw)) return SIDEBAR_DEFAULT_WIDTH;
  return clampSidebarWidth(raw);
}

const sidebarCollapsed = ref(localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "true");
const sidebarWidth = ref(readStoredSidebarWidth());
const sidebarResizing = ref(false);
const shellSidebarPx = computed(() =>
  sidebarCollapsed.value ? SIDEBAR_COLLAPSED_WIDTH : sidebarWidth.value,
);
const documentTheme = ref(document.documentElement.dataset.theme === "light" ? "light" : "dark");
let themeObserver: MutationObserver | undefined;

function persistSidebar() {
  localStorage.setItem(SIDEBAR_COLLAPSED_KEY, String(sidebarCollapsed.value));
  localStorage.setItem(SIDEBAR_WIDTH_KEY, String(sidebarWidth.value));
}

function toggleSidebar() {
  sidebarCollapsed.value = !sidebarCollapsed.value;
  persistSidebar();
}

function beginSidebarResize(event: PointerEvent) {
  const handle = event.currentTarget;
  if (!(handle instanceof HTMLElement) || event.button !== 0) return;
  event.preventDefault();
  handle.setPointerCapture(event.pointerId);
  sidebarResizing.value = true;
  document.body.classList.add("is-resizing-panels");
  const startX = event.clientX;
  const originCollapsed = sidebarCollapsed.value;
  const startWidth = originCollapsed ? SIDEBAR_COLLAPSED_WIDTH : sidebarWidth.value;

  const move = (moveEvent: PointerEvent) => {
    const next = startWidth + (moveEvent.clientX - startX);
    if (sidebarCollapsed.value) {
      if (next < SIDEBAR_EXPAND_THRESHOLD) return;
      sidebarCollapsed.value = false;
      sidebarWidth.value = clampSidebarWidth(Math.max(next, SIDEBAR_MIN_WIDTH));
      return;
    }
    if (next < SIDEBAR_COLLAPSE_THRESHOLD) {
      sidebarCollapsed.value = true;
      return;
    }
    sidebarWidth.value = clampSidebarWidth(next);
  };

  const finish = () => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", finish);
    handle.removeEventListener("pointercancel", finish);
    if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId);
    document.body.classList.remove("is-resizing-panels");
    sidebarResizing.value = false;
    persistSidebar();
  };

  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", finish);
  handle.addEventListener("pointercancel", finish);
}

async function toggleTheme() {
  const next = documentTheme.value === "light" ? "dark" : "light";
  document.documentElement.dataset.theme = next;
  try {
    await store.saveSettings({ theme: next });
  } catch {
    // 主题已在界面上生效；持久化失败时下次启动沿用旧值。
  }
}
const isMacOS = /Mac|iPhone|iPad/.test(navigator.platform);
const isWindows = /Win/i.test(navigator.platform);
const maximized = ref(false);
const unsavedPrompt = ref(getUnsavedPrompt());
let closing = false;
let forceQuit = false;
let stopWindowResize: (() => void) | undefined;
let stopWindowClose: (() => void) | undefined;
let stopUnsavedPrompt: (() => void) | undefined;

function onPageUnload(event: BeforeUnloadEvent) {
  if (!hasUnsavedEditors()) return;
  event.preventDefault();
  event.returnValue = "";
}

async function currentWindow() {
  return getCurrentWindow();
}

async function minimizeWindow() {
  try {
    await (await currentWindow()).minimize();
  } catch {
    // Browser preview has no native window chrome.
  }
}

async function toggleMaximizeWindow() {
  if (!isWindows) return;
  try {
    const window = await currentWindow();
    await window.toggleMaximize();
    maximized.value = await window.isMaximized();
  } catch {
    // Browser preview has no native window chrome.
  }
}

function toggleLanguagePicker() {
  if (languagePickerOpen.value) {
    languagePickerOpen.value = false;
    languageAnchor.value = null;
    return;
  }
  languageAnchor.value = languageButton.value?.getBoundingClientRect() ?? null;
  languagePickerOpen.value = Boolean(languageAnchor.value);
}

function onSelectLanguage(id: string) {
  editorStatus.choose(id);
  languagePickerOpen.value = false;
  languageAnchor.value = null;
}

function openStatusMenu(kind: "indent" | "eol", event: MouseEvent) {
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  statusMenu.value = { x: rect.left, y: rect.top, kind };
}

function onStatusMenuSelect(id: string) {
  const kind = statusMenu.value?.kind;
  statusMenu.value = null;
  if (kind === "eol" && (id === "LF" || id === "CRLF")) {
    editorStatus.requestDocument({ type: "eol", eol: id });
  }
  if (kind === "indent") {
    if (id === "tabs-4") editorStatus.requestDocument({ type: "indent", insertSpaces: false, tabSize: 4 });
    if (id === "spaces-2") editorStatus.requestDocument({ type: "indent", insertSpaces: true, tabSize: 2 });
    if (id === "spaces-4") editorStatus.requestDocument({ type: "indent", insertSpaces: true, tabSize: 4 });
    if (id === "spaces-8") editorStatus.requestDocument({ type: "indent", insertSpaces: true, tabSize: 8 });
  }
}

watch(
  () => route.path,
  () => {
    languagePickerOpen.value = false;
    languageAnchor.value = null;
    statusMenu.value = null;
  },
);

async function quitWindow() {
  forceQuit = true;
  const windowRef = await currentWindow();
  try {
    await windowRef.destroy();
  } catch {
    try {
      await windowRef.close();
    } catch {
      forceQuit = false;
    }
  }
}

async function closeWindow() {
  if (closing) return;
  closing = true;
  try {
    if (!(await confirmLeaveUnsavedEditors())) return;
    await quitWindow();
  } catch {
    // Browser preview has no native window chrome.
  } finally {
    closing = false;
  }
}

const projectsNavTo = ref(lastProjectsPath());

watch(
  () => route.path,
  (path) => {
    rememberProjectsPath(path);
    if (path === "/projects" || path.startsWith("/projects/")) projectsNavTo.value = path;
  },
  { immediate: true },
);

const primary = computed(() => [
  { to: projectsNavTo.value, match: "/projects", label: t("nav.projects"), icon: FolderKanban },
  { to: "/inbox", label: t("nav.inbox"), icon: Inbox },
  { to: "/tasks", label: t("nav.tasks"), icon: ListTodo },
  { to: "/automation", label: t("nav.automation"), icon: Workflow },
  { to: "/channels", label: t("nav.channels"), icon: MessagesSquare },
  { to: "/agents", label: t("nav.agents"), icon: Bot },
  { to: "/squads", label: t("nav.squads"), icon: Users },
  { to: "/stats", label: t("nav.stats"), icon: BarChart3 },
]);

const inboxUnread = computed(() => unreadTotal(store.data?.inbox ?? []));

function isNavActive(item: { to: string; match?: string }) {
  const base = item.match ?? item.to;
  return route.path === base || route.path.startsWith(`${base}/`);
}

const navGroups = computed(() => [
  {
    label: t("app.navGroup.workspace"),
    items: primary.value.map((item) => ({
      id: item.match ?? item.to,
      label: item.label,
      icon: item.icon,
      to: item.to,
      active: isNavActive(item),
      badge: item.to === "/inbox" && inboxUnread.value ? unreadBadgeLabel(inboxUnread.value) : undefined,
      title: sidebarCollapsed.value ? item.label : undefined,
    })),
  },
  {
    label: t("app.navGroup.config"),
    items: config.value.map((item) => ({
      id: item.to,
      label: item.label,
      icon: item.icon,
      to: item.to,
      active: isNavActive(item),
      title: sidebarCollapsed.value ? item.label : undefined,
    })),
  },
]);

function onNavSelect(item: SideNavItem) {
  if (item.to) markMenuNavigation(router, item.to);
}

const config = computed(() => [
  { to: "/runtimes", label: t("nav.runtimes"), icon: Boxes },
  { to: "/skills", label: t("nav.skills"), icon: Sparkles },
  { to: "/mcp", label: t("nav.mcp"), icon: Network },
  ...(import.meta.env.DEV
    ? [{ to: "/preview", label: t("nav.preview"), icon: Palette }]
    : []),
  { to: "/settings", label: t("nav.settings"), icon: Settings },
]);

async function restoreImmersiveTitlebar() {
  if (!isWindows) return;
  try {
    await (await currentWindow()).setDecorations(false);
  } catch {
    // Browser preview has no native window chrome.
  }
}

onMounted(async () => {
  startRunningTerminalWatch();
  store.initialize();
  // 设置页、store 初始化都会直接改 data-theme，这里统一监听以同步标题栏图标。
  themeObserver = new MutationObserver(() => {
    documentTheme.value = document.documentElement.dataset.theme === "light" ? "light" : "dark";
  });
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  window.addEventListener("beforeunload", onPageUnload);
  stopUnsavedPrompt = onUnsavedPromptChange(() => {
    unsavedPrompt.value = getUnsavedPrompt();
  });
  try {
    const windowRef = await currentWindow();
    stopWindowClose = await windowRef.onCloseRequested(async (event) => {
      if (forceQuit) return;
      event.preventDefault();
      await closeWindow();
    });
  } catch {
    // Browser preview has no native window chrome.
  }
  if (!isWindows) return;
  try {
    const window = await currentWindow();
    await restoreImmersiveTitlebar();
    maximized.value = await window.isMaximized();
    stopWindowResize = await window.onResized(async () => {
      maximized.value = await window.isMaximized();
    });
    const unlistenFocus = await window.onFocusChanged((focused) => {
      if (focused) void restoreImmersiveTitlebar();
    });
    const previous = stopWindowResize;
    stopWindowResize = () => {
      previous?.();
      unlistenFocus();
    };
  } catch {
    // Browser preview has no native window chrome.
  }
});
onBeforeUnmount(() => {
  themeObserver?.disconnect();
  window.removeEventListener("beforeunload", onPageUnload);
  stopUnsavedPrompt?.();
  stopWindowClose?.();
  stopWindowResize?.();
});
</script>

<template>
  <div
    class="app-shell"
    :class="{
      'is-macos': isMacOS,
      'is-windows': isWindows,
      'is-sidebar-collapsed': sidebarCollapsed,
      'is-sidebar-resizing': sidebarResizing,
    }"
    :style="{ '--shell-sidebar': `${shellSidebarPx}px` }"
  >
    <header class="titlebar" data-tauri-drag-region>
      <div class="titlebar-drag" data-tauri-drag-region @dblclick="toggleMaximizeWindow" />
      <nav class="titlebar-crumbs" data-tauri-drag-region :aria-label="t('nav.breadcrumb')">
        <button
          type="button"
          class="titlebar-action titlebar-back"
          :disabled="!backCrumb"
          :title="backCrumb ? t('nav.backTo', { label: backCrumb.label }) : undefined"
          :aria-label="t('nav.back')"
          @click="goBack"
        >
          <ChevronLeft :size="16" />
        </button>
        <template v-for="(crumb, index) in crumbs" :key="crumb.path">
          <ChevronRight v-if="index > 0" class="titlebar-crumb-sep" :size="13" aria-hidden="true" />
          <span v-if="index === crumbs.length - 1" class="titlebar-crumb is-current" aria-current="page">{{ crumb.label }}</span>
          <RouterLink v-else :to="crumb.fullPath" class="titlebar-crumb">{{ crumb.label }}</RouterLink>
        </template>
      </nav>
      <!-- 面包屑已表明当前页面；中间只在需求工作台展示写入的详情（需求名 + 项目数） -->
      <span class="titlebar-title" data-tauri-drag-region>
        <template v-if="titlebarDetail && route.path.startsWith('/projects/')">
          <strong :title="titlebarDetail.title">{{ titlebarDetail.title }}</strong>
          <small v-if="titlebarDetail.summary" :title="titlebarDetail.summary">{{ titlebarDetail.summary }}</small>
        </template>
      </span>
      <div class="titlebar-right">
        <button
          type="button"
          class="titlebar-action"
          :title="documentTheme === 'light' ? t('nav.themeDark') : t('nav.themeLight')"
          @click="toggleTheme"
        >
          <Moon v-if="documentTheme === 'light'" :size="15" />
          <Sun v-else :size="15" />
        </button>
        <div v-if="isWindows" class="window-controls">
          <button type="button" class="window-control" :title="t('app.minimize')" @pointerdown.stop.prevent="minimizeWindow">
            <Minus :size="12" />
          </button>
          <button type="button" class="window-control" :title="maximized ? t('app.restore') : t('app.maximize')" @pointerdown.stop.prevent="toggleMaximizeWindow">
            <Copy v-if="maximized" :size="11" />
            <Square v-else :size="11" />
          </button>
          <button type="button" class="window-control is-close" :title="t('app.close')" @pointerdown.stop.prevent="closeWindow">
            <X :size="13" />
          </button>
        </div>
      </div>
    </header>

    <SideNav
      :groups="navGroups"
      :collapsed="sidebarCollapsed"
      :aria-label="t('app.mainNav')"
      @select="onNavSelect"
      @resize-start="beginSidebarResize"
    >
      <template #footer>
        <button
          type="button"
          class="nav-item sidebar-collapse"
          :title="sidebarCollapsed ? t('app.expandSidebar') : t('app.collapseSidebar')"
          @click="toggleSidebar"
        >
          <PanelLeftOpen v-if="sidebarCollapsed" :size="16" />
          <PanelLeftClose v-else :size="16" />
          <span class="nav-item-label">{{ t("app.collapseSidebar") }}</span>
        </button>
      </template>
    </SideNav>

    <main class="main-content vs-scroll">
      <div v-if="store.loading" class="empty">{{ t("app.loading") }}</div>
      <div v-else-if="store.error" class="error-card">{{ store.error }}</div>
      <RouterView v-else v-slot="{ Component }">
        <!-- 按 path 缓存页面，切侧栏菜单不丢滚动/表单；上限防止无限涨内存 -->
        <keep-alive :max="24">
          <component :is="Component" :key="route.path" />
        </keep-alive>
      </RouterView>
    </main>

    <!-- 需求工作台/预览自带聊天：只隐藏不销毁，避免切菜单后再开气泡回到最早提问 -->
    <FloatingChat :hidden="route.path.startsWith('/projects') || route.path === '/preview'" />
    <UpdateNotification />
    <UnsavedChangesDialog
      v-if="unsavedPrompt"
      :title="unsavedPrompt.title"
      :message="unsavedPrompt.message"
      @save="finishUnsavedPrompt('save')"
      @discard="finishUnsavedPrompt('discard')"
      @cancel="finishUnsavedPrompt('cancel')"
    />
    <footer class="statusbar">
      <div class="statusbar-end">
        <span
          v-if="showEditorStatus && editorStatus.blameLabel"
          class="statusbar-item"
          :title="editorStatus.blameTitle"
        >{{ editorStatus.blameLabel }}</span>
        <button
          v-if="showEditorStatus"
          type="button"
          class="statusbar-item"
          :title="t('app.gotoLine')"
          @click="editorStatus.requestDocument({ type: 'goto' })"
        >
          {{ editorStatus.positionLabel }}
        </button>
        <span v-if="showEditorStatus" class="statusbar-item" :title="t('app.encoding')">{{ editorStatus.encoding }}</span>
        <button
          v-if="showEditorStatus"
          type="button"
          class="statusbar-item"
          :title="t('app.eol')"
          @click="openStatusMenu('eol', $event)"
        >
          {{ editorStatus.eol }}
        </button>
        <button
          v-if="showEditorStatus"
          type="button"
          class="statusbar-item"
          :title="t('app.indent')"
          @click="openStatusMenu('indent', $event)"
        >
          {{ editorStatus.indentText }}
        </button>
        <button
          v-if="showEditorLanguage"
          ref="languageButton"
          type="button"
          class="statusbar-item"
          :title="t('app.languageMode')"
          @click="toggleLanguagePicker"
        >
          {{ editorStatus.languageLabel }}
        </button>
        <span v-if="showEditorStatus" class="statusbar-item" :title="editorZoomLabel">{{ editorZoomLabel }}</span>
        <TerminalActivity />
        <span class="statusbar-running">
          <i class="ds-dot" :data-tone="(store.data?.stats.running ?? 0) > 0 ? 'success' : undefined" />
          {{ t("app.runningCount", { n: store.data?.stats.running ?? 0 }) }}
        </span>
      </div>
    </footer>
    <EditorLanguagePicker
      v-if="languagePickerOpen && languageAnchor"
      :languages="editorStatus.languages"
      :current-id="editorStatus.languageId"
      :has-override="editorStatus.hasOverride"
      :anchor="languageAnchor"
      @select="onSelectLanguage"
      @close="languagePickerOpen = false"
    />
    <ContextMenu
      v-if="statusMenu"
      :x="statusMenu.x"
      :y="statusMenu.y - 8"
      :items="statusMenu.kind === 'indent' ? indentMenuItems : eolMenuItems"
      @select="onStatusMenuSelect"
      @close="statusMenu = null"
    />
    <RunLogHost />
  </div>
</template>
