<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type Component } from "vue";
import { playEnter } from "./ui/motion";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { rememberAppRoute } from "./views/dashboard/last-app-route";
import { lastProjectsPath, rememberProjectsPath } from "./views/dashboard/last-projects-path";
import { readStorage, writeStorage } from "./utils/safe-storage";
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
  GitBranch,
  Workflow,
  X,
} from "@lucide/vue";
import { startOsFileDrop } from "./components/chat/os-file-drop";
import { backTarget, crumbs, markMenuNavigation } from "./stores/breadcrumbs";
import FloatingChat from "./components/FloatingChat.vue";
import CanvasFrameHost from "./components/CanvasFrameHost.vue";
import SessionRawDataHost from "./components/session-raw/SessionRawDataHost.vue";
import UpdateNotification from "./components/UpdateNotification.vue";
import { RunLogHost } from "./components/run-log";
import UnsavedChangesDialog from "./components/workbench/UnsavedChangesDialog.vue";
import UiConfirmDialog from "./components/ui/UiConfirmDialog.vue";
import UiToastHost from "./components/ui/UiToastHost.vue";
import {
  confirmDialog,
  finishConfirmDialog,
  getConfirmDialogRequest,
  onConfirmDialogChange,
} from "./components/ui/confirm-dialog";
import { closeFocusedSurface } from "./components/close-surface";
import {
  confirmLeaveUnsavedEditors,
  finishUnsavedPrompt,
  getUnsavedPrompt,
  hasUnsavedEditors,
  onUnsavedPromptChange,
} from "./components/workbench/unsaved-editor";
import { useAppStore } from "./stores/app";
import { useEditorStatusStore } from "./stores/editor-status";
import { applyThemeSetting } from "./theme";
import EditorLanguagePicker from "./components/workbench/EditorLanguagePicker.vue";
import ContextMenu, { type ContextMenuItem } from "./components/ui/ContextMenu.vue";
import { editorZoomLabel } from "./components/workbench/monaco-zoom";
import { isWorkbenchPath } from "./router";
import { requirementHomeMode } from "./views/dashboard/requirement-home-mode";
import {
  buildHomeIndex,
  buildHomeProject,
  homeWorkerItems,
  type HomeProject,
} from "./views/dashboard/project-home";
import RequirementProjectRow from "./components/dashboard/RequirementProjectRow.vue";
import { t } from "./i18n";
import { unreadBadgeLabel, unreadTotal } from "./domain/inbox";
import SideNav, { type SideNavItem } from "./components/SideNav.vue";
import TerminalActivity from "./components/workbench/TerminalActivity.vue";
import RunningStatus from "./components/RunningStatus.vue";
import { startRunningTerminalWatch } from "./stores/running-terminals";
import { startSurfaceCtlWatch } from "./stores/surface-ctl";
import { startTerminalCtlWatch } from "./stores/terminal-ctl";
import TaskCreateDialog from "./views/TaskCreateDialog.vue";

const store = useAppStore();
const editorStatus = useEditorStatusStore();
const route = useRoute();

/* 切侧栏菜单：新页面淡入。不用 <Transition> 包 keep-alive —— 多根节点的页面在 out-in 下会白屏 */
const mainEl = ref<HTMLElement | null>(null);
watch(
  () => route.path,
  () =>
    void nextTick(() => {
      for (const child of mainEl.value?.children ?? []) {
        if (child instanceof HTMLElement && !child.className.startsWith("vs-overlay")) playEnter(child, "page");
      }
    }),
);
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
const hideFloatingChat = computed(
  () => isWorkbenchPath(route.path) || route.path === "/inbox" || (route.path === "/projects" && requirementHomeMode.value === "run"),
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
const workbenchProjectId = computed(() => {
  const match = route.path.match(/^\/projects\/([^/]+)/);
  return match?.[1] ?? null;
});
/** 所有需求工作台共用一个 keep-alive 槽。按 path 分槽会在下拉直切时同时挂两个完整工作台（Monaco + git + 聊天），直接卡死。 */
const viewCacheKey = computed(() =>
  workbenchProjectId.value ? "project-workspace" : route.path,
);
/** 与需求首页同一套「谁在工作」数据，供面包屑切换下拉复用。 */
const projectSwitchRows = computed(() => {
  const data = store.data;
  if (!data) return [] as HomeProject[];
  const currentId = workbenchProjectId.value;
  const active = data.projects.filter((item) => !item.archivedAt);
  const current = currentId ? data.projects.find((item) => item.id === currentId) : undefined;
  const list = current && current.archivedAt && !active.some((item) => item.id === current.id)
    ? [...active, current]
    : active;
  const index = buildHomeIndex(data, store.runStates);
  const now = Date.now();
  return [...list]
    .sort((a, b) => (b.openedAt || b.createdAt).localeCompare(a.openedAt || a.createdAt))
    .map((item) => buildHomeProject(item, data, store.runStates, now, index));
});

const projectSwitchById = computed(() => {
  const map = new Map<string, HomeProject>();
  for (const row of projectSwitchRows.value) map.set(row.id, row);
  return map;
});

const projectSwitchOptions = computed(() =>
  projectSwitchRows.value.map((row) => ({
    value: row.id,
    label: row.name || t("workbench.projectFallback"),
    // 占位以启用双行样式；实际内容由 option 插槽里的 RequirementProjectRow 渲染
    description: row.activityLabel || row.workingMeta || row.workerSentence || "\u00a0",
    keywords: row.workers.map((worker) => worker.name),
  })),
);

function projectSwitchOptionRows(projectId: string | number | boolean | null | undefined) {
  if (projectId == null || projectId === false) return [] as HomeProject[];
  const row = projectSwitchById.value.get(String(projectId));
  return row ? [row] : [];
}

function switchWorkbenchProject(projectId: string) {
  if (!projectId || projectId === workbenchProjectId.value) return;
  const target = `/projects/${projectId}`;
  markMenuNavigation(router, target);
  void router.push(target);
}

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
  const raw = Number(readStorage(SIDEBAR_WIDTH_KEY));
  if (!Number.isFinite(raw)) return SIDEBAR_DEFAULT_WIDTH;
  return clampSidebarWidth(raw);
}

const sidebarCollapsed = ref(readStorage(SIDEBAR_COLLAPSED_KEY) === "true");
const sidebarWidth = ref(readStoredSidebarWidth());
const sidebarResizing = ref(false);
const shellSidebarPx = computed(() =>
  sidebarCollapsed.value ? SIDEBAR_COLLAPSED_WIDTH : sidebarWidth.value,
);
const documentTheme = ref(document.documentElement.dataset.theme === "light" ? "light" : "dark");
let themeObserver: MutationObserver | undefined;

function persistSidebar() {
  writeStorage(SIDEBAR_COLLAPSED_KEY, String(sidebarCollapsed.value));
  writeStorage(SIDEBAR_WIDTH_KEY, String(sidebarWidth.value));
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
  applyThemeSetting(next);
  documentTheme.value = next;
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
const confirmRequest = ref(getConfirmDialogRequest());
let closing = false;
let forceQuit = false;
let stopWindowResize: (() => void) | undefined;
let stopWindowClose: (() => void) | undefined;
let stopUnsavedPrompt: (() => void) | undefined;
let stopConfirmDialog: (() => void) | undefined;
let stopMenuWordWrap: UnlistenFn | undefined;
let stopMenuAutoSaveOnFocusLoss: UnlistenFn | undefined;
let stopMenuClose: UnlistenFn | undefined;

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

let closeShortcutBusy = false;

async function handleCloseShortcut() {
  if (closeShortcutBusy || closing) return;
  closeShortcutBusy = true;
  try {
    const handled = await closeFocusedSurface();
    if (!handled) await closeWindow();
  } finally {
    closeShortcutBusy = false;
  }
}

function onCloseShortcut(event: KeyboardEvent) {
  if (event.isComposing || event.altKey || event.shiftKey) return;
  if (!(event.metaKey || event.ctrlKey) || event.code !== "KeyW") return;
  event.preventDefault();
  event.stopPropagation();
  void handleCloseShortcut();
}

function onTaskCreateShortcut(event: KeyboardEvent) {
  if (event.isComposing || !event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return;
  if (event.code !== "KeyC") return;
  const target = event.target;
  if (target instanceof HTMLElement && target.closest("input, textarea, select, [contenteditable='true']")) return;
  event.preventDefault();
  store.openTaskCreate();
}

async function closeWindow() {
  if (closing) return;
  closing = true;
  try {
    if (hasUnsavedEditors()) {
      if (!(await confirmLeaveUnsavedEditors())) return;
    } else if (!(await confirmDialog(t("app.closeConfirm"), { type: "warning", primary: t("app.close") }))) {
      return;
    }
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
    rememberAppRoute(path);
    rememberProjectsPath(path);
    if (path === "/projects" || path.startsWith("/projects/")) projectsNavTo.value = path;
  },
  { immediate: true },
);

const primary = computed(() => [
  { to: projectsNavTo.value, match: "/projects", label: t("nav.projects"), icon: FolderKanban },
  { to: "/inbox", label: t("nav.inbox"), icon: Inbox },
  {
    to: "/tasks",
    label: t("nav.tasks"),
    icon: ListTodo,
    action: { tip: t("tasks.createShortcut"), run: () => store.openTaskCreate() },
  },
  { to: "/workflows", label: t("nav.workflows"), icon: GitBranch },
  { to: "/automation", label: t("nav.automation"), icon: Workflow },
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
      action: "action" in item ? item.action : undefined,
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
  { to: "/channels", label: t("nav.channels"), icon: MessagesSquare },
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
  startOsFileDrop();
  startRunningTerminalWatch();
  startTerminalCtlWatch();
  startSurfaceCtlWatch();
  store.initialize();
  // 设置页、store 初始化都会直接改 data-theme，这里统一监听以同步标题栏图标。
  themeObserver = new MutationObserver(() => {
    documentTheme.value = document.documentElement.dataset.theme === "light" ? "light" : "dark";
  });
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  window.addEventListener("beforeunload", onPageUnload);
  window.addEventListener("keydown", onCloseShortcut, true);
  window.addEventListener("keydown", onTaskCreateShortcut);
  stopUnsavedPrompt = onUnsavedPromptChange(() => {
    unsavedPrompt.value = getUnsavedPrompt();
  });
  stopConfirmDialog = onConfirmDialogChange(() => {
    confirmRequest.value = getConfirmDialogRequest();
  });
  try {
    stopMenuClose = await listen("menu-close", () => {
      void handleCloseShortcut();
    });
    stopMenuWordWrap = await listen<boolean>("menu-word-wrap", async ({ payload }) => {
      try {
        await store.saveSettings({ wordWrap: String(payload) });
      } catch {
        // 菜单勾选已变；持久化失败时下次启动沿用旧值。
      }
    });
    stopMenuAutoSaveOnFocusLoss = await listen<boolean>("menu-auto-save-on-focus-loss", async ({ payload }) => {
      try {
        await store.saveSettings({ autoSaveOnFocusLoss: String(payload) });
      } catch {
        // 菜单勾选已变；持久化失败时下次启动沿用旧值。
      }
    });
  } catch {
    // Browser preview has no native menu.
  }
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
  window.removeEventListener("keydown", onCloseShortcut, true);
  window.removeEventListener("keydown", onTaskCreateShortcut);
  stopUnsavedPrompt?.();
  stopConfirmDialog?.();
  stopMenuClose?.();
  stopMenuWordWrap?.();
  stopMenuAutoSaveOnFocusLoss?.();
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
      <div class="titlebar-left" data-tauri-drag-region>
        <nav class="titlebar-crumbs" data-tauri-drag-region :aria-label="t('nav.breadcrumb')">
          <UiTooltip :content="backCrumb ? t('nav.backTo', { label: backCrumb.label }) : undefined"><button
            type="button"
            class="titlebar-action titlebar-back"
            :disabled="!backCrumb"
            :aria-label="t('nav.back')"
            @click="goBack"
          >
            <ChevronLeft :size="16" />
          </button></UiTooltip>
          <template v-for="(crumb, index) in crumbs" :key="crumb.path">
            <ChevronRight v-if="index > 0" class="titlebar-crumb-sep" :size="13" aria-hidden="true" />
            <span v-if="index === crumbs.length - 1" class="titlebar-crumb is-current" aria-current="page">{{ crumb.label }}</span>
            <RouterLink v-else :to="crumb.fullPath" class="titlebar-crumb">{{ crumb.label }}</RouterLink>
          </template>
        </nav>
        <template v-if="workbenchProjectId">
          <ChevronRight class="titlebar-crumb-sep" :size="13" aria-hidden="true" />
          <UiSelect
            class="titlebar-project-switch"
            variant="ghost"
            size="sm"
            :model-value="workbenchProjectId"
            :options="projectSwitchOptions"
            :menu-min-width="280"
            :aria-label="t('workbench.switchProject')"
            searchable
            @update:model-value="switchWorkbenchProject"
          >
            <template #option="{ option }">
              <RequirementProjectRow
                v-for="row in projectSwitchOptionRows(option.value)"
                :key="row.id"
                :name="row.name"
                :session-ids="row.sessionIds"
                :workers="homeWorkerItems(row)"
                :idle-label="row.activityLabel"
                :badge="row.archived ? t('dashboard.archivedBadge') : ''"
              />
            </template>
          </UiSelect>
        </template>
      </div>
      <!-- 三列网格占位，保持右侧按钮居右 -->
      <span class="titlebar-title" data-tauri-drag-region />
      <div class="titlebar-right">
        <UiTooltip :content="documentTheme === 'light' ? t('nav.themeDark') : t('nav.themeLight')"><button
          type="button"
          class="titlebar-action"
         
          @click="toggleTheme"
        >
          <Moon v-if="documentTheme === 'light'" :size="15" />
          <Sun v-else :size="15" />
        </button></UiTooltip>
        <div v-if="isWindows" class="window-controls">
          <UiTooltip :content="t('app.minimize')"><button type="button" class="window-control" @pointerdown.stop.prevent="minimizeWindow">
            <Minus :size="12" />
          </button></UiTooltip>
          <UiTooltip :content="maximized ? t('app.restore') : t('app.maximize')"><button type="button" class="window-control" @pointerdown.stop.prevent="toggleMaximizeWindow">
            <Copy v-if="maximized" :size="11" />
            <Square v-else :size="11" />
          </button></UiTooltip>
          <UiTooltip :content="t('app.close')"><button type="button" class="window-control is-close" @pointerdown.stop.prevent="closeWindow">
            <X :size="13" />
          </button></UiTooltip>
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
        <UiTooltip
          placement="right"
          :content="sidebarCollapsed ? t('app.expandSidebar') : t('app.collapseSidebar')"
        ><button
          type="button"
          class="nav-item sidebar-collapse"
          @click="toggleSidebar"
        >
          <PanelLeftOpen v-if="sidebarCollapsed" :size="16" />
          <PanelLeftClose v-else :size="16" />
          <span class="nav-item-label">{{ t("app.collapseSidebar") }}</span>
        </button></UiTooltip>
      </template>
    </SideNav>

    <main ref="mainEl" class="main-content vs-scroll">
      <div v-if="store.loading" class="empty">{{ t("app.loading") }}</div>
      <div v-else-if="store.error" class="error-card">{{ store.error }}</div>
      <RouterView v-else v-slot="{ Component }">
        <!-- 按 path 缓存页面，切侧栏菜单不丢滚动/表单；上限防止无限涨内存 -->
        <keep-alive :max="24">
          <component :is="Component" :key="viewCacheKey" />
        </keep-alive>
      </RouterView>
    </main>

    <!-- 需求工作台/预览自带聊天：只隐藏不销毁，避免切菜单后再开气泡回到最早提问 -->
    <FloatingChat :hidden="hideFloatingChat" />
    <!-- Canvas 页面的 iframe 都在这一层（keep-alive 外面），切走页面也保持连接 -->
    <CanvasFrameHost />
    <SessionRawDataHost />
    <UpdateNotification />
    <UiToastHost />
    <UnsavedChangesDialog
      v-if="unsavedPrompt"
      :title="unsavedPrompt.title"
      :message="unsavedPrompt.message"
      @save="finishUnsavedPrompt('save')"
      @discard="finishUnsavedPrompt('discard')"
      @cancel="finishUnsavedPrompt('cancel')"
    />
    <UiConfirmDialog
      v-if="confirmRequest"
      :request="confirmRequest"
      @resolve="finishConfirmDialog"
    />
    <TaskCreateDialog
      v-if="store.taskCreateOpen"
      :initial-status="store.taskCreateStatus"
      :lock-status="store.taskCreateLockStatus"
      @close="store.closeTaskCreate()"
    />
    <footer class="statusbar">
      <div class="statusbar-end">
        <UiTooltip
          v-if="showEditorStatus && editorStatus.blameLabel"
          :content="editorStatus.blameTitle"
        >
          <span class="statusbar-item">{{ editorStatus.blameLabel }}</span>
        </UiTooltip>
        <UiTooltip :content="t('app.gotoLine')"><button
          v-if="showEditorStatus"
          type="button"
          class="statusbar-item"
         
          @click="editorStatus.requestDocument({ type: 'goto' })"
        >
          {{ editorStatus.positionLabel }}
        </button></UiTooltip>
        <UiTooltip v-if="showEditorStatus" :content="t('app.encoding')">
          <span class="statusbar-item">{{ editorStatus.encoding }}</span>
        </UiTooltip>
        <UiTooltip :content="t('app.eol')"><button
          v-if="showEditorStatus"
          type="button"
          class="statusbar-item"
         
          @click="openStatusMenu('eol', $event)"
        >
          {{ editorStatus.eol }}
        </button></UiTooltip>
        <UiTooltip :content="t('app.indent')"><button
          v-if="showEditorStatus"
          type="button"
          class="statusbar-item"
         
          @click="openStatusMenu('indent', $event)"
        >
          {{ editorStatus.indentText }}
        </button></UiTooltip>
        <UiTooltip :content="t('app.languageMode')"><button
          v-if="showEditorLanguage"
          ref="languageButton"
          type="button"
          class="statusbar-item"
         
          @click="toggleLanguagePicker"
        >
          {{ editorStatus.languageLabel }}
        </button></UiTooltip>
        <UiTooltip v-if="showEditorStatus" :content="editorZoomLabel">
          <span class="statusbar-item">{{ editorZoomLabel }}</span>
        </UiTooltip>
        <TerminalActivity />
        <RunningStatus />
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
