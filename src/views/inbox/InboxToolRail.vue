<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { FileContent, Workspace } from "../../domain/types";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import type { TerminalProfile } from "../../services/backend";
import { detectIndent } from "../../editor/eol";
import { currentDocumentTheme, monacoThemeName } from "../../editor/monaco-workspace";
import SourceControlPanel from "../../components/SourceControlPanel.vue";
import CommitMultiDiffEditor, { type CommitDiffFile } from "../../components/scm/CommitMultiDiffEditor.vue";
import GitDiffEditor from "../../components/scm/GitDiffEditor.vue";
import FileTypeIcon from "../../components/FileTypeIcon.vue";
import Codicon from "../../components/ui/Codicon.vue";
import ContextMenu, { type ContextMenuItem } from "../../components/ui/ContextMenu.vue";
import UiButton from "../../components/ui/UiButton.vue";
import UiTooltip from "../../components/ui/UiTooltip.vue";
import EditorTabButton from "../../components/workbench/EditorTabButton.vue";
import EditorWithDiff from "../../components/workbench/EditorWithDiff.vue";
import SearchView from "../../components/workbench/SearchView.vue";
import TerminalPanel from "../../components/workbench/TerminalPanel.vue";
import WorkspaceFileTree from "../../components/workbench/WorkspaceFileTree.vue";
import { fileNameOf, workspaceMode, workspaceModeHint, workspaceModeLabel, workspaceSubtitle } from "../../components/workbench/workspace-mode";
import { loadScmDiff, scmDiffId, scmOpensAsFile } from "../../components/workbench/scm-diff";
import type { ScmOpenChangesPayload, ScmOpenTarget } from "../../components/workbench/scm-groups";
import { useAppStore } from "../../stores/app";
import {
  activeTerminalSessionId,
  ensureWorkspaceTerminal,
  killTerminalSession,
  openTerminalSession,
  patchTerminalSession,
  reorderTerminalSessions,
  selectTerminalSession,
  terminalSessions,
} from "../../stores/terminal-sessions";
import { inboxBrowserLinks } from "./inbox-browser-links";
import { placeEditorTab } from "./editor-tabs";
import InboxBrowser from "./InboxBrowser.vue";
import InboxCanvas from "./InboxCanvas.vue";
import TaskIssueSide from "../TaskIssueSide.vue";
import {
  INBOX_FILES_OPEN_KEY,
  INBOX_FILES_WIDTH_KEY,
  INBOX_PANEL_WIDTH_KEY,
  INBOX_SURFACES_KEY,
  INBOX_TERMINAL_OPEN_KEY,
  INBOX_TOOL_MIN_WIDTH,
  createInboxSurfaceTab,
  readInboxFilesWidth,
  readInboxPanelWidth,
  readInboxSurfaces,
  type InboxSurfaceId,
  type InboxSurfaceTab,
} from "./inbox-tool";

type EditorTab = {
  key: string;
  workspaceId: string;
  path: string;
  name: string;
  pinned: boolean;
  content: string;
  original: string;
  language: string;
  readOnly: boolean;
  emptyFiles?: boolean;
  diff: { id: string; original: string; modified: string; binary: boolean; language: string } | null;
  commitFiles: CommitDiffFile[] | null;
  commitId: string;
};

const TERMINAL_MIN_HEIGHT = 120;
const TERMINAL_HEIGHT_KEY = "multara.panel.terminalHeight";

const props = defineProps<{
  workspaces: Workspace[];
  workspaceId: string;
  sessionId: string;
  /** 需求才有更改和文件，任务才有属性侧栏。 */
  kind: "project" | "task" | "chat";
  taskId: string | null;
}>();

const emit = defineEmits<{
  "pick-workspace": [id: string];
  layout: [payload: { content: boolean; files: boolean; width: number; terminal: boolean }];
}>();

const store = useAppStore();
const panelWidth = ref(readInboxPanelWidth());
const filesWidth = ref(readInboxFilesWidth());
const filesWanted = ref(false);
const terminalOpen = ref(false);
const surfaces = ref<InboxSurfaceTab[]>(readInboxSurfaces());
const editorTabs = ref<EditorTab[]>([]);
const activeEditorKey = ref("");
const activeSurfaceKey = ref("");
const browserRequest = ref("");
const terminalHeight = ref(readTerminalHeight());
const profiles = ref<TerminalProfile[]>([]);
const defaultProfile = ref(localStorage.getItem("multara.terminal.defaultProfile") || "");

const workspace = computed(() => props.workspaces.find((item) => item.id === props.workspaceId) ?? props.workspaces[0] ?? null);
const openWorkspaceIds = ref(new Set<string>());
const links = computed(() => inboxBrowserLinks(store.messages[props.sessionId]));
const activeEditor = computed(() => editorTabs.value.find((tab) => tab.key === activeEditorKey.value) ?? null);
const activeSurface = computed(() => surfaces.value.find((item) => item.key === activeSurfaceKey.value) ?? null);
const fileTabActive = computed(() => Boolean(activeEditor.value) && !activeSurface.value);
const filesVisible = computed(() => fileTabActive.value && filesWanted.value);
const contentVisible = computed(() => Boolean(activeEditor.value || activeSurface.value));
const stageOpen = computed(() => contentVisible.value);
const sessions = computed(() => {
  const ids = new Set(props.workspaces.map((item) => item.id));
  return terminalSessions.value.filter((item) => ids.has(item.workspaceId));
});
const activeTerminalId = computed(() => {
  if (sessions.value.some((item) => item.id === activeTerminalSessionId.value)) return activeTerminalSessionId.value;
  return sessions.value[0]?.id ?? "";
});
const theme = computed(() => monacoThemeName(currentDocumentTheme()));
const indent = computed(() => detectIndent(activeEditor.value?.content ?? ""));
const stickyScroll = computed(() => store.data?.settings.stickyScroll !== "false");
const wordWrap = computed(() => store.data?.settings.wordWrap === "true");

const FLOAT_EDGE = 0;
const FLOAT_DRAG_THRESHOLD = 4;
const FLOAT_KEY = "multara.inbox.float";
const floatTop = ref<number | null>(readFloatTop());
const floatEl = ref<HTMLElement | null>(null);
const floatDragging = ref(false);
let floatDrag: { y: number; top: number; moved: boolean } | null = null;
let floatHostObserver: ResizeObserver | null = null;

function readFloatTop() {
  try {
    const raw = JSON.parse(localStorage.getItem(FLOAT_KEY) || "null");
    if (typeof raw === "number") return raw;
    if (raw && typeof raw.y === "number") return raw.y;
  } catch {
    return null;
  }
  return null;
}

function floatHost() {
  return document.querySelector(".inbox-detail.chat-surface");
}

function headerOffset(host: HTMLElement) {
  const header = host.querySelector(":scope > .chat-surface-header");
  if (!(header instanceof HTMLElement)) return 0;
  return header.offsetHeight;
}

function clampFloatTop(top: number, host: HTMLElement, nav: HTMLElement) {
  const min = headerOffset(host);
  const max = Math.max(min, host.clientHeight - nav.offsetHeight);
  return Math.min(Math.max(min, top), max);
}

function keepFloatInside() {
  const host = floatHost();
  const nav = floatEl.value;
  if (!(host instanceof HTMLElement) || !nav) return;
  const next = clampFloatTop(floatTop.value ?? headerOffset(host), host, nav);
  if (next === floatTop.value) return;
  floatTop.value = next;
  localStorage.setItem(FLOAT_KEY, JSON.stringify(next));
}

const floatStyle = computed(() => {
  const host = floatHost();
  const min = host instanceof HTMLElement ? headerOffset(host) : 0;
  const top = floatTop.value === null ? min : Math.max(min, floatTop.value);
  return { top: `${top}px`, right: `${FLOAT_EDGE}px` };
});

function floatNav(event: PointerEvent) {
  const handle = event.currentTarget;
  if (!(handle instanceof HTMLElement)) return null;
  const nav = handle.parentElement;
  if (!(nav instanceof HTMLElement)) return null;
  return { handle, nav };
}

function onFloatDown(event: PointerEvent) {
  if (event.button !== 0) return;
  const nodes = floatNav(event);
  const host = floatHost();
  if (!nodes || !(host instanceof HTMLElement)) return;
  event.preventDefault();
  floatEl.value = nodes.nav;
  const hostRect = host.getBoundingClientRect();
  const navRect = nodes.nav.getBoundingClientRect();
  floatDrag = {
    y: event.clientY,
    top: navRect.top - hostRect.top,
    moved: false,
  };
  nodes.handle.setPointerCapture(event.pointerId);
}

function onFloatMove(event: PointerEvent) {
  const nodes = floatNav(event);
  const host = floatHost();
  if (!floatDrag || !nodes || !(host instanceof HTMLElement)) return;
  event.preventDefault();
  const dy = event.clientY - floatDrag.y;
  if (!floatDrag.moved && Math.abs(dy) < FLOAT_DRAG_THRESHOLD) return;
  floatDrag.moved = true;
  floatDragging.value = true;
  document.body.classList.add("is-dragging-inbox-float");
  window.getSelection()?.removeAllRanges();
  floatTop.value = clampFloatTop(floatDrag.top + dy, host, nodes.nav);
}

function onFloatUp() {
  if (floatDrag?.moved && floatTop.value !== null) localStorage.setItem(FLOAT_KEY, JSON.stringify(floatTop.value));
  floatDrag = null;
  floatDragging.value = false;
  document.body.classList.remove("is-dragging-inbox-float");
}

function watchFloatHost() {
  floatHostObserver?.disconnect();
  floatHostObserver = null;
  const host = floatHost();
  if (!(host instanceof HTMLElement)) return;
  floatHostObserver = new ResizeObserver(() => keepFloatInside());
  floatHostObserver.observe(host);
  const header = host.querySelector(":scope > .chat-surface-header");
  if (header instanceof HTMLElement) floatHostObserver.observe(header);
  keepFloatInside();
}

const surfaceLabel: Record<InboxSurfaceId, string> = {
  changes: "workbench.git",
  browser: "inbox.tool.browser",
  search: "workbench.search",
  details: "tasks.propertiesPanel",
  canvas: "canvas.surface",
};

function surfaceAllowed(id: InboxSurfaceId) {
  if (id === "canvas") {
    return store.data?.settings.genuiEnabled === "true" && store.data?.settings.canvasesEnabled === "true";
  }
  if (id === "details") return props.kind === "task";
  if (id === "changes") return props.kind === "project";
  return true;
}

const visibleSurfaces = computed(() => surfaces.value.filter((item) => surfaceAllowed(item.kind)));

const ADD_TABS = [
  { id: "browser", label: "inbox.tool.browser", icon: "globe" },
  { id: "search", label: "workbench.search", icon: "search" },
] as const;

function readTerminalHeight() {
  const stored = Number(localStorage.getItem(TERMINAL_HEIGHT_KEY));
  if (stored >= TERMINAL_MIN_HEIGHT) return stored;
  return 240;
}

function writeLayout() {
  localStorage.setItem(INBOX_PANEL_WIDTH_KEY, String(panelWidth.value));
  localStorage.setItem(INBOX_FILES_WIDTH_KEY, String(filesWidth.value));
  localStorage.setItem(INBOX_FILES_OPEN_KEY, filesWanted.value ? "1" : "0");
  localStorage.setItem(INBOX_TERMINAL_OPEN_KEY, terminalOpen.value ? "1" : "0");
  localStorage.setItem(INBOX_SURFACES_KEY, JSON.stringify(surfaces.value.map((item) => item.kind)));
  localStorage.setItem("multara.inbox.activeSurface", activeSurface.value?.kind ?? "");
  emit("layout", {
    content: stageOpen.value,
    files: false,
    width: panelWidth.value,
    terminal: terminalOpen.value,
  });
  void nextTick(keepFloatInside);
}

function tabDirty(tab: EditorTab) {
  return !tab.diff && !tab.commitFiles && tab.content !== tab.original;
}

function activateEditor(tab: EditorTab) {
  activeEditorKey.value = tab.key;
  activeSurfaceKey.value = "";
  writeLayout();
}

function selectSurface(key: string) {
  activeSurfaceKey.value = key;
  writeLayout();
}

/** fresh：始终新开；singleton：同 kind 只保留一个并切过去。 */
function openSurface(kind: InboxSurfaceId, mode: "fresh" | "singleton") {
  if (mode === "singleton") {
    const existing = surfaces.value.find((item) => item.kind === kind);
    if (existing) {
      activeSurfaceKey.value = existing.key;
      writeLayout();
      return;
    }
  }
  const tab = createInboxSurfaceTab(kind);
  surfaces.value = [...surfaces.value, tab];
  activeSurfaceKey.value = tab.key;
  writeLayout();
}

function showFiles() {
  filesWanted.value = true;
  const existing = editorTabs.value.find((tab) => tab.emptyFiles);
  if (existing) {
    activateEditor(existing);
    return;
  }
  const tab: EditorTab = {
    key: "files-empty",
    workspaceId: "",
    path: "",
    name: "",
    pinned: true,
    emptyFiles: true,
    content: "",
    original: "",
    language: "",
    readOnly: true,
    diff: null,
    commitFiles: null,
    commitId: "",
  };
  editorTabs.value = [...editorTabs.value, tab];
  activateEditor(tab);
}

function tabTip(tab: EditorTab) {
  if (tab.emptyFiles) return t("workbench.files");
  if (tab.pinned) return tab.path;
  return `${tab.path} · ${t("workbench.tabPreview")}`;
}

function toggleFiles() {
  filesWanted.value = !filesVisible.value;
  writeLayout();
}

async function toggleTerminal() {
  terminalOpen.value = !terminalOpen.value;
  writeLayout();
  const current = workspace.value;
  if (!terminalOpen.value || !current) return;
  await ensureWorkspaceTerminal({
    workspaceId: current.id,
    cwd: current.workPath || current.sourcePath,
    title: current.name,
    projectId: current.projectId,
  });
}

const addMenu = ref<{ x: number; y: number } | null>(null);
const addMenuItems = computed<ContextMenuItem[]>(() => ADD_TABS.map((item) => ({
  id: item.id,
  label: t(item.label),
  icon: item.icon,
})));

function openAddMenu(event: MouseEvent) {
  const button = event.currentTarget;
  if (!(button instanceof HTMLElement)) return;
  const rect = button.getBoundingClientRect();
  addMenu.value = { x: rect.left, y: rect.bottom };
}

function onAddMenu(id: string) {
  addMenu.value = null;
  if (id === "browser" || id === "search") openSurface(id, "fresh");
}

function closeSurface(key: string) {
  const next = surfaces.value.filter((item) => item.key !== key);
  surfaces.value = next;
  if (activeSurfaceKey.value === key) activeSurfaceKey.value = next.at(-1)?.key ?? "";
  writeLayout();
}

function closeEditor(tab: EditorTab) {
  const next = editorTabs.value.filter((item) => item.key !== tab.key);
  editorTabs.value = next;
  if (activeEditorKey.value === tab.key) {
    activeEditorKey.value = next.at(-1)?.key ?? "";
    if (activeEditorKey.value) activeSurfaceKey.value = "";
  }
  writeLayout();
}

function place(tab: EditorTab) {
  editorTabs.value = placeEditorTab(editorTabs.value, tab, tabDirty);
  activateEditor(tab);
}

async function openFile(workspaceId: string, path: string, pin = false) {
  const key = `${workspaceId}\n${path}`;
  const existing = editorTabs.value.find((tab) => tab.key === key);
  if (existing) {
    if (pin) existing.pinned = true;
    activateEditor(existing);
    return;
  }
  let file: FileContent;
  try {
    file = await backend.readWorkspaceFile(workspaceId, path);
  } catch {
    file = { workspaceId, path, content: "", language: "plaintext", size: 0, readOnly: true };
  }
  place({
    key,
    workspaceId,
    path,
    name: fileNameOf(path),
    pinned: pin,
    content: file.content,
    original: file.content,
    language: file.language,
    readOnly: Boolean(file.readOnly),
    diff: null,
    commitFiles: null,
    commitId: "",
  });
}

async function readSide(workspaceId: string, path: string) {
  try {
    const file = await backend.readWorkspaceFile(workspaceId, path);
    return { content: file.content, language: file.language };
  } catch (cause) {
    if (cause instanceof backend.FileMissingError) return null;
    throw cause;
  }
}

function diffReaders(workspaceId: string) {
  return {
    readWorktree: (path: string) => readSide(workspaceId, path),
    readIndex: async (path: string) => {
      const file = await backend.gitIndexFile(workspaceId, path);
      return { content: file.content, binary: file.binary };
    },
    readRevision: async (revision: string, path: string) => {
      const file = await backend.gitRevisionFile(workspaceId, revision, path);
      return { content: file.content, binary: file.binary };
    },
  };
}

async function openChange(target: ScmOpenTarget) {
  if (scmOpensAsFile(target.group)) {
    await openFile(target.workspaceId, target.path, true);
    return;
  }
  try {
    const loaded = await loadScmDiff(target, diffReaders(target.workspaceId));
    const id = scmDiffId(target);
    const existing = editorTabs.value.find((tab) => tab.diff?.id === id);
    if (existing) {
      existing.pinned = true;
      activateEditor(existing);
      return;
    }
    place({
      key: id,
      workspaceId: target.workspaceId,
      path: target.path,
      name: loaded.tabName || fileNameOf(target.path),
      pinned: true,
      content: loaded.modified,
      original: loaded.modified,
      language: loaded.language,
      readOnly: true,
      diff: { id, original: loaded.original, modified: loaded.modified, binary: loaded.binary, language: loaded.language },
      commitFiles: null,
      commitId: "",
    });
  } catch {
    await openFile(target.workspaceId, target.path, true);
  }
}

async function openChanges(payload: ScmOpenChangesPayload) {
  if (!payload.targets.length) return;
  try {
    const files = await Promise.all(payload.targets.map(async (target) => {
      const loaded = await loadScmDiff(target, diffReaders(target.workspaceId));
      return {
        path: target.path,
        status: target.status || "M",
        original: loaded.original,
        modified: loaded.modified,
        language: loaded.language,
        binary: loaded.binary,
      } satisfies CommitDiffFile;
    }));
    const id = `${payload.workspaceId}:commit-multi:${payload.commit}`;
    const existing = editorTabs.value.find((tab) => tab.commitId === id);
    if (existing) {
      existing.commitFiles = files;
      activateEditor(existing);
      return;
    }
    place({
      key: id,
      workspaceId: payload.workspaceId,
      path: files[0]?.path ?? payload.commit,
      name: payload.title,
      pinned: false,
      content: "",
      original: "",
      language: "plaintext",
      readOnly: true,
      diff: null,
      commitFiles: files,
      commitId: id,
    });
  } catch {
    const first = payload.targets[0];
    if (first) await openFile(first.workspaceId, first.path, true);
  }
}

function onEditorChange(content: string, fromUser: boolean) {
  const tab = activeEditor.value;
  if (!tab || !fromUser) return;
  tab.content = content;
  if (tab.content !== tab.original) tab.pinned = true;
}

async function applySearchReplace(workspaceId: string, path: string, content: string) {
  const tab = editorTabs.value.find((item) => item.workspaceId === workspaceId && item.path === path && !item.diff);
  if (tab && tabDirty(tab)) return false;
  await backend.writeWorkspaceFile(workspaceId, path, content);
  if (tab) {
    tab.content = content;
    tab.original = content;
  }
  return true;
}

async function createTerminal(profile?: string) {
  const current = workspace.value;
  if (!current) return;
  terminalOpen.value = true;
  await openTerminalSession({
    workspaceId: current.id,
    cwd: current.workPath || current.sourcePath,
    title: current.name,
    projectId: current.projectId,
    profile: typeof profile === "string" ? profile : undefined,
  });
  writeLayout();
}

async function splitTerminal() {
  const active = sessions.value.find((item) => item.id === activeTerminalId.value);
  const current = workspace.value;
  if (!active || !current) {
    await createTerminal();
    return;
  }
  await openTerminalSession({
    workspaceId: active.workspaceId,
    cwd: active.cwd,
    title: active.title,
    projectId: current.projectId,
    profile: active.profileId,
    groupId: active.groupId || active.id,
  });
}

function selectTerminal(id: string) {
  const tab = sessions.value.find((item) => item.id === id);
  const owner = props.workspaces.find((item) => item.id === tab?.workspaceId);
  if (!tab || !owner) return;
  if (!tab.pending) {
    selectTerminalSession(id);
    return;
  }
  void openTerminalSession({
    workspaceId: owner.id,
    cwd: tab.cwd,
    title: tab.title,
    projectId: owner.projectId,
  });
}

function beginPanelResize(event: PointerEvent) {
  if (!stageOpen.value) return;
  event.preventDefault();
  const startX = event.clientX;
  const startWidth = panelWidth.value;
  const max = Math.round(window.innerWidth * 0.5);
  document.body.classList.add("is-resizing-panels");
  const move = (moveEvent: PointerEvent) => {
    panelWidth.value = Math.min(max, Math.max(INBOX_TOOL_MIN_WIDTH, startWidth - (moveEvent.clientX - startX)));
    keepFloatInside();
  };
  const finish = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", finish);
    window.removeEventListener("pointercancel", finish);
    document.body.classList.remove("is-resizing-panels");
    writeLayout();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", finish);
  window.addEventListener("pointercancel", finish);
}

function beginFilesResize(event: PointerEvent) {
  event.preventDefault();
  event.stopPropagation();
  const startX = event.clientX;
  const startWidth = filesWidth.value;
  const max = panelWidth.value;
  document.body.classList.add("is-resizing-panels");
  const move = (moveEvent: PointerEvent) => {
    filesWidth.value = Math.min(max, Math.max(INBOX_TOOL_MIN_WIDTH, startWidth - (moveEvent.clientX - startX)));
  };
  const finish = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", finish);
    window.removeEventListener("pointercancel", finish);
    document.body.classList.remove("is-resizing-panels");
    writeLayout();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", finish);
  window.addEventListener("pointercancel", finish);
}

function beginTerminalResize(event: PointerEvent) {
  const startY = event.clientY;
  const start = terminalHeight.value;
  const limit = Math.round(window.innerHeight * 0.7);
  const move = (moveEvent: PointerEvent) => {
    terminalHeight.value = Math.min(limit, Math.max(TERMINAL_MIN_HEIGHT, start + startY - moveEvent.clientY));
  };
  const finish = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", finish);
    localStorage.setItem(TERMINAL_HEIGHT_KEY, String(terminalHeight.value));
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", finish);
}

function onKeydown(event: KeyboardEvent) {
  const ctrl = event.metaKey || event.ctrlKey;
  if (!ctrl || event.altKey || event.shiftKey || event.code !== "Backquote") return;
  const target = event.target;
  if (target instanceof HTMLElement && target.closest("input, textarea, [contenteditable='true']")) return;
  event.preventDefault();
  void toggleTerminal();
}

function closeTerminal() {
  terminalOpen.value = false;
  writeLayout();
}

function setDefaultProfile(id: string) {
  defaultProfile.value = id;
  localStorage.setItem("multara.terminal.defaultProfile", id);
}

function showBrowser(url: string) {
  browserRequest.value = `${Date.now()}\n${url}`;
  const current = activeSurface.value?.kind === "browser" ? activeSurface.value : null;
  const existing = current ?? surfaces.value.find((item) => item.kind === "browser") ?? null;
  if (existing) {
    activeSurfaceKey.value = existing.key;
    writeLayout();
    return;
  }
  openSurface("browser", "fresh");
}

function openTerminalPath(payload: { path: string; isDirectory: boolean }) {
  const match = props.workspaces.find((item) => {
    const root = item.workPath || item.sourcePath;
    return payload.path === root || payload.path.startsWith(`${root}/`);
  });
  if (!match) return;
  const root = match.workPath || match.sourcePath;
  const relative = payload.path.slice(root.length).replace(/^\//, "");
  if (payload.isDirectory) {
    emit("pick-workspace", match.id);
    filesWanted.value = true;
    activeSurfaceKey.value = "";
    writeLayout();
    return;
  }
  void openFile(match.id, relative, true);
}

function closeAll() {
  surfaces.value = [];
  editorTabs.value = [];
  activeEditorKey.value = "";
  activeSurfaceKey.value = "";
  filesWanted.value = false;
  terminalOpen.value = false;
  writeLayout();
}

defineExpose({ closeAll });

watch([stageOpen, panelWidth, terminalOpen], () => writeLayout(), { immediate: true });

watch(() => props.kind, (kind) => {
  if (kind !== "task" && activeSurface.value?.kind === "details") activeSurfaceKey.value = "";
  if (kind !== "project" && activeSurface.value?.kind === "changes") activeSurfaceKey.value = "";
});

function workspaceExpanded(id: string) {
  return openWorkspaceIds.value.has(id);
}

function toggleWorkspaceSection(id: string) {
  const next = new Set(openWorkspaceIds.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  openWorkspaceIds.value = next;
}

watch(() => props.workspaces.map((item) => item.id).join("\n"), () => {
  const next = new Set(openWorkspaceIds.value);
  for (const item of props.workspaces) next.add(item.id);
  openWorkspaceIds.value = next;
}, { immediate: true });

watch(() => props.sessionId, () => {
  editorTabs.value = [];
  activeEditorKey.value = "";
  writeLayout();
});

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  void nextTick(watchFloatHost);
  void backend.terminalProfiles().then((list) => {
    profiles.value = list ?? [];
  }).catch(() => {
    profiles.value = [];
  });
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  floatHostObserver?.disconnect();
  floatHostObserver = null;
  document.body.classList.remove("is-dragging-inbox-float");
});
</script>

<template>
  <section v-if="stageOpen" class="inbox-stage">
    <div
      class="inbox-tool-resize"
      role="separator"
      aria-orientation="vertical"
      :aria-label="t('inbox.tool.resize')"
      tabindex="0"
      @pointerdown="beginPanelResize"
    />
    <div class="inbox-stage-main">
      <header class="editor-tabbar">
        <div class="inbox-tabbar-main">
        <div class="editor-tabs vs-scroll">
          <EditorTabButton
            v-for="tab in editorTabs"
            :key="tab.key"
            :tip="tabTip(tab)"
            :name="tab.emptyFiles ? t('workbench.files') : tab.name"
            :active="!activeSurface && tab.key === activeEditorKey"
            :preview="!tab.pinned"
            :dirty="tabDirty(tab)"
            @select="activateEditor(tab)"
            @pin="tab.pinned = true"
            @close="closeEditor(tab)"
          >
            <Codicon v-if="tab.emptyFiles" name="files" :size="16" />
            <FileTypeIcon v-else :name="tab.name" />
          </EditorTabButton>
          <EditorTabButton
            v-for="tab in visibleSurfaces"
            :key="tab.key"
            :tip="t(surfaceLabel[tab.kind])"
            :name="t(surfaceLabel[tab.kind])"
            :active="activeSurfaceKey === tab.key"
            :close-tip="t('inbox.tool.close')"
            @select="selectSurface(tab.key)"
            @close="closeSurface(tab.key)"
          >
            <Codicon v-if="tab.kind === 'details'" name="symbol-property" :size="16" />
            <Codicon v-else-if="tab.kind === 'browser'" name="globe" :size="16" />
            <Codicon v-else-if="tab.kind === 'search'" name="search" :size="16" />
            <Codicon v-else-if="tab.kind === 'changes'" name="source-control" :size="16" />
            <Codicon v-else-if="tab.kind === 'canvas'" name="dashboard" :size="16" />
          </EditorTabButton>
        </div>
        <div class="inbox-tool-add">
          <UiButton variant="ghost" size="icon" :tip="t('inbox.tool.add')" @click="openAddMenu">
            <Codicon name="add" :size="16" />
          </UiButton>
        </div>
        </div>
        <div v-if="fileTabActive" class="inbox-tabbar-actions">
          <UiTooltip :content="t('workbench.files')">
            <button
              type="button"
              class="action-item menu-entry"
              :aria-pressed="filesVisible"
              :aria-label="t('workbench.files')"
              @click="toggleFiles"
            >
              <Codicon class="action-label" :class="{ checked: filesVisible }" name="list-selection" :size="16" />
            </button>
          </UiTooltip>
        </div>
      </header>
      <div class="inbox-stage-body" :class="{ 'has-files': filesVisible }">
        <div
          v-for="tab in visibleSurfaces"
          :key="tab.key"
          v-show="activeSurfaceKey === tab.key"
          class="inbox-surface-pane"
        >
          <InboxBrowser
            v-if="tab.kind === 'browser'"
            :links="links"
            :open-url="activeSurfaceKey === tab.key ? browserRequest : ''"
          />
          <SearchView
            v-else-if="tab.kind === 'search'"
            :workspaces="workspaces"
            :preview="false"
            :active="activeSurfaceKey === tab.key"
            :apply-replace="applySearchReplace"
            @open="(workspaceId, path) => openFile(workspaceId, path)"
          />
          <TaskIssueSide
            v-else-if="tab.kind === 'details' && taskId"
            :task-id="taskId"
          />
          <InboxCanvas
            v-else-if="tab.kind === 'canvas'"
            :session-id="sessionId"
          />
          <SourceControlPanel
            v-else-if="tab.kind === 'changes'"
            :project-id="workspace?.projectId ?? ''"
            :workspaces="workspaces"
            @open-change="openChange"
            @open-changes="openChanges"
            @open-file="(target) => openFile(target.workspaceId, target.path, true)"
          />
        </div>
        <div v-if="activeEditor && !activeSurface" class="editor-area">
          <div v-if="activeEditor.emptyFiles" class="empty-file-editor">
            <p>{{ t("inbox.tool.filesEmpty") }}</p>
          </div>
          <div v-else class="editor-main">
            <CommitMultiDiffEditor
              v-if="activeEditor.commitFiles?.length"
              :diff-id="activeEditor.commitId"
              :files="activeEditor.commitFiles"
              :theme="theme"
            />
            <GitDiffEditor
              v-else-if="activeEditor.diff"
              :diff-id="activeEditor.diff.id"
              :original="activeEditor.diff.original"
              :modified="activeEditor.diff.modified"
              :language="activeEditor.diff.language"
              :file-path="activeEditor.path"
              :theme="theme"
              :binary="activeEditor.diff.binary"
            />
            <EditorWithDiff
              v-else
              :file-path="activeEditor.path"
              :workspace-id="activeEditor.workspaceId"
              :content="activeEditor.content"
              :language="activeEditor.language"
              :read-only="activeEditor.readOnly"
              :theme="theme"
              :tab-size="indent.tabSize"
              :insert-spaces="indent.insertSpaces"
              :sticky-scroll="stickyScroll"
              :word-wrap="wordWrap"
              @change="onEditorChange"
            />
          </div>
        </div>
        <aside
          v-if="filesVisible"
          class="inbox-files-pane"
          :style="{ width: `${filesWidth}px` }"
        >
          <div
            class="inbox-files-resize"
            role="separator"
            aria-orientation="vertical"
            :aria-label="t('inbox.tool.resize')"
            @pointerdown="beginFilesResize"
          />
          <div class="file-tree vs-scroll inbox-files-tree">
            <p v-if="!workspaces.length" class="tree-empty">{{ t("inbox.tool.noWorkspace") }}</p>
            <section
              v-for="item in workspaces"
              :key="item.id"
              class="tree-workspace"
            >
              <button
                type="button"
                class="tree-workspace-head"
                :class="`is-${workspaceMode(item)}`"
                @click="toggleWorkspaceSection(item.id)"
              >
                <Codicon
                  name="chevron-down"
                  class="tree-chevron"
                  :class="{ 'is-open': workspaceExpanded(item.id) }"
                  :size="10"
                />
                <FileTypeIcon
                  :name="item.kind === 'git' ? '.gitignore' : item.name"
                  :directory="item.kind !== 'git'"
                  :open="workspaceExpanded(item.id)"
                />
                <UiTooltip :content="workspaceModeHint(item)">
                  <div class="tree-workspace-copy">
                    <strong>{{ item.name }}</strong>
                    <small>{{ workspaceSubtitle(item) }}</small>
                  </div>
                </UiTooltip>
                <span class="workspace-mode-pill" :class="`pill-${workspaceMode(item)}`">{{ workspaceModeLabel(item) }}</span>
              </button>
              <WorkspaceFileTree
                v-if="workspaceExpanded(item.id)"
                embedded
                state-key="inbox"
                :workspace="item"
                @open="openFile(item.id, $event)"
                @pin="openFile(item.id, $event, true)"
              />
            </section>
          </div>
        </aside>
      </div>
    </div>
  </section>
  <Teleport to=".inbox-detail.chat-surface">
    <nav
      ref="floatEl"
      class="inbox-float"
      :style="floatStyle"
      :aria-label="t('inbox.tool.rail')"
    >
      <div
        class="inbox-float-handle"
        :class="{ 'is-dragging': floatDragging }"
        aria-hidden="true"
        @pointerdown="onFloatDown"
        @pointermove="onFloatMove"
        @pointerup="onFloatUp"
        @pointercancel="onFloatUp"
      >
        <Codicon name="gripper" class="inbox-float-grip" />
      </div>
      <UiButton
        v-if="kind === 'project'"
        variant="ghost"
        size="icon"
        :tip="t('inbox.tool.changes')"
        :aria-pressed="activeSurface?.kind === 'changes'"
        @click="openSurface('changes', 'singleton')"
      >
        <Codicon name="diff-multiple" :size="16" />
      </UiButton>
      <UiButton
        v-if="kind === 'project'"
        variant="ghost"
        size="icon"
        :tip="t('workbench.files')"
        :aria-pressed="Boolean(activeEditor?.emptyFiles) && !activeSurface"
        @click="showFiles"
      >
        <Codicon name="files" :size="16" />
      </UiButton>
      <UiButton
        v-if="kind === 'task'"
        variant="ghost"
        size="icon"
        :tip="t('tasks.propertiesPanel')"
        :aria-pressed="activeSurface?.kind === 'details'"
        @click="openSurface('details', 'singleton')"
      >
        <Codicon name="symbol-property" :size="16" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :tip="t('inbox.tool.browser')"
        :aria-pressed="activeSurface?.kind === 'browser'"
        @click="openSurface('browser', 'fresh')"
      >
        <Codicon name="globe" :size="16" />
      </UiButton>
      <UiButton
        v-if="surfaceAllowed('canvas')"
        variant="ghost"
        size="icon"
        :tip="t('canvas.surface')"
        :aria-pressed="activeSurface?.kind === 'canvas'"
        @click="openSurface('canvas', 'singleton')"
      >
        <Codicon name="dashboard" :size="16" />
      </UiButton>
      <span class="inbox-float-separator" role="separator" />
      <UiButton
        variant="ghost"
        size="icon"
        :tip="t('inbox.tool.terminal')"
        :aria-pressed="terminalOpen"
        @click="toggleTerminal"
      >
        <Codicon name="terminal" :size="16" />
      </UiButton>
    </nav>
  </Teleport>
  <ContextMenu
    v-if="addMenu"
    fit
    :x="addMenu.x"
    :y="addMenu.y"
    :items="addMenuItems"
    @select="onAddMenu"
    @close="addMenu = null"
  />
  <div v-if="terminalOpen" class="inbox-chat-terminal">
    <TerminalPanel
      :sessions="sessions"
      :active-id="activeTerminalId"
      :height="terminalHeight"
      new-title=""
      :profiles="profiles"
      :default-profile="defaultProfile"
      :open="terminalOpen"
      @close="closeTerminal"
      @new="createTerminal"
      @split="splitTerminal"
      @select="selectTerminal"
      @kill="killTerminalSession"
      @reorder="reorderTerminalSessions"
      @resize="beginTerminalResize"
      @patch="patchTerminalSession"
      @open-url="showBrowser"
      @open-path="openTerminalPath"
      @default-profile="setDefaultProfile"
    />
  </div>
</template>
