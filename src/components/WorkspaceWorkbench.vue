<script setup lang="ts">
import {
  computed,
  markRaw,
  nextTick,
  onActivated,
  onBeforeUnmount,
  onDeactivated,
  onMounted,
  ref,
  shallowRef,
  watch,
  watchEffect,
} from "vue";
import { useRoute } from "vue-router";
import { t } from "../i18n";
import { titlebarDetail } from "../stores/titlebar";
import {
  ArrowUpRight,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  X,
} from "@lucide/vue";
import { monaco } from "../monaco";
import {
  AUTO_DETECT_LANGUAGE,
  editorLanguageKey,
  listSelectableLanguages,
} from "../monaco-language";
import type * as Monaco from "monaco-editor";
import type {
  ChatMessage,
  EditorSelectionContext,
  FileContent,
  FileEntry,
  GitFileStatus,
  GitRepositoryStatus,
  Workspace,
} from "../domain/types";
import ChatPanel from "./ChatPanel.vue";
import AddWorkspaceModal from "./dashboard/AddWorkspaceModal.vue";
import ExplorerTreeList from "./workbench/ExplorerTreeList.vue";
import FileTypeIcon from "./FileTypeIcon.vue";
import SourceControlPanel from "./SourceControlPanel.vue";
import GitDiffEditor from "./scm/GitDiffEditor.vue";
import ContextMenu, { type ContextMenuItem } from "./ui/ContextMenu.vue";
import { explorerKey, explorerShortcutLabels, type ExplorerKeyResult } from "./workbench/explorer-keys";
import ExplorerDialog from "./workbench/explorer/ExplorerDialog.vue";
import { applyFind, buildExplorerRows, type ExplorerRow, type ExplorerRowNode } from "./workbench/explorer/explorer-rows";
import { isExcluded } from "./workbench/explorer/explorer-glob";
import { explorerHoverTitle, highlightName } from "./workbench/explorer/explorer-format";
import { renameSelection, validateFileName, wellFormedFileName } from "./workbench/explorer/explorer-names";
import { isCopyDrag } from "./workbench/explorer/explorer-dnd";
import { collectDroppedItems } from "./workbench/explorer/explorer-import";
import { explorerItemKey, useExplorer, type ExplorerItem } from "./workbench/explorer/use-explorer";
import * as explorerBackend from "../services/explorer-backend";
import {
  askUnsaved,
  registerDirtyEditorPaths,
  registerUnsavedDirtyChecker,
  registerUnsavedLeaveHandler,
} from "./workbench/unsaved-editor";
import {
  fileNameOf,
  joinWorkPath,
  detachWorkspacePrompt,
  workspaceBranchName,
  workspaceMode,
  workspaceModeHint,
  workspaceModeLabel,
  workspaceSubtitle,
} from "./workbench/workspace-mode";
import {
  mergeFetchedRecordEntry,
  recordFromFetchedKeys,
} from "./workbench/async-record";
import { folderGitStatus, gitStatusClass, gitStatusLetter, gitStatusTitle } from "./workbench/tree-git-status";
import {
  filePathMatchesReference,
  isAbsoluteFsPath,
  matchLongestWorkspaceRoot,
  normalizeFsPath,
  relativeToolPath,
  pathWithoutWorktreeDir,
  workspaceFileLabel,
  type LocatedFile,
  type WorkspacePathRoot,
} from "./workbench/workspace-file-location";
import QuickOpen from "./workbench/QuickOpen.vue";
import type { QuickOpenFile } from "./workbench/quick-open";
import { loadScmDiff, scmDiffId } from "./workbench/scm-diff";
import { localHistoryEntry, recordLocalHistory, renameLocalHistoryPath } from "./workbench/local-history";
import { scmGroups, type ScmOpenTarget } from "./workbench/scm-groups";
import { bindEditorZoom } from "./workbench/monaco-zoom";
import { createRefreshScheduler } from "./workbench/refresh-scheduler";
import EditorWithDiff from "./workbench/EditorWithDiff.vue";
import MediaPreview from "./workbench/MediaPreview.vue";
import MarkdownPreview from "./workbench/MarkdownPreview.vue";
import EditorBreadcrumbs from "./workbench/EditorBreadcrumbs.vue";
import EditorOutline from "./workbench/EditorOutline.vue";
import ExplorerTimeline from "./workbench/ExplorerTimeline.vue";
import SearchView from "./workbench/SearchView.vue";
import TerminalPanel, { type TerminalTab } from "./workbench/TerminalPanel.vue";
import { useDiffStore } from "../stores/diff";
import EditorZoomHud from "./workbench/EditorZoomHud.vue";
import { detectEol, detectIndent, convertEol, type EditorEol } from "../editor/eol";
import {
  editorOptionsFromConfig,
  matchEditorConfig,
  parseEditorConfig,
  type EditorConfigSection,
} from "../editor/editorconfig";
import { enclosingOutline, fileBreadcrumbs, outlineFromContent } from "../editor/outline";
import {
  compilerOptionsFromTsconfig,
  parseTsconfigJson,
  type WorkspaceCompilerOptions,
} from "../editor/tsconfig-options";
import { monacoThemeName, currentDocumentTheme, watchDocumentTheme } from "../editor/monaco-workspace";
import { codeIndex } from "../editor/code-index";
import {
  installCodeNavigation,
  refreshNavigationIndex,
  registerWorkspaceOpener,
  releaseNavigationWorkspaces,
  setNavigationWorkspaces,
} from "../editor/code-navigation";
import {
  PREVIEW_PROJECT_ID,
  previewDirectoryChildren,
  previewFileContent,
  previewGitRepository,
  previewGitRepositoryB,
  previewRootEntries,
  previewWorkspace,
  previewWorkspaceB,
} from "./workbench/preview-workspace";
import {
  beginWorkspaceFilePointerDrag,
  isWorkspaceFileDrag,
  readWorkspaceFileDragData,
  resolvePathInWorkspaces,
} from "./chat/workspace-file-drag";
import { itemKind, toolPath } from "./chat/timeline-display";
import * as backend from "../services/backend";
import { forgetTerminal, runningTerminals, terminalFocus, trackTerminal } from "../stores/running-terminals";
import { useAppStore } from "../stores/app";
import { useEditorStatusStore } from "../stores/editor-status";

const props = withDefaults(
  defineProps<{
    projectId?: string | null;
    preview?: boolean;
    previewInteractive?: boolean;
    previewMessages?: ChatMessage[];
    previewComposerText?: string;
    previewTitle?: string;
    previewSessionTitle?: string | null;
    previewBusy?: boolean;
  }>(),
  {
    projectId: null,
    preview: false,
    previewInteractive: false,
    previewMessages: () => [],
    previewComposerText: "",
    previewTitle: t("workbench.previewTitle"),
    previewSessionTitle: null,
    previewBusy: false,
  },
);

const route = useRoute();
const store = useAppStore();
const diffStore = useDiffStore();
const editorStatus = useEditorStatusStore();

function ownsEditorStatus() {
  return props.preview ? route.path === "/preview" : route.path.startsWith("/projects/");
}
const projectId = computed(() => props.projectId ?? "");
const project = computed(() =>
  store.data?.projects.find((item) => item.id === projectId.value),
);
const detachedWorkspaceIds = ref(new Set<string>());
const workspaces = computed(() => {
  const source = props.preview
    ? [previewWorkspace, previewWorkspaceB]
    : (() => {
        const projectWorkspaces =
          store.data?.workspaces.filter(
            (item) => item.projectId === projectId.value,
          ) ?? [];
        const dedicated = projectWorkspaces.filter(
          (item) => item.sessionId === store.activeSessionId,
        );
        return dedicated.length
          ? dedicated
          : projectWorkspaces.filter((item) => !item.sessionId);
      })();
  return source.filter((item) => !detachedWorkspaceIds.value.has(item.id));
});
const chatPanel = ref<{
  insertChip: (item: EditorSelectionContext) => void;
  insertText: (value: string) => void;
  addPastedImage: () => void;
  addAttachment: (name: string, mimeType: string) => void;
  attachWorkspaceFile: (file: {
    workspaceId: string;
    path: string;
    isDirectory?: boolean;
  }) => void;
  clearComposer: () => void;
  focus: () => void;
} | null>(null);
const workspaceRoots = ref<Record<string, FileEntry[]>>({});
const directoryChildren = ref<Record<string, FileEntry[]>>({});
const expandedDirectories = ref(new Set<string>());
const expandedWorkspaces = ref(new Set<string>());
const loadingDirectories = ref(new Set<string>());
const activeFile = ref<FileContent | null>(null);
const originalContent = ref("");
type EditorTab = {
  workspaceId: string;
  path: string;
  pinned: boolean;
  file: FileContent;
  original: string;
  /** 用户在编辑器里改过；打开、换行符同步、智能体刷盘都不算。 */
  userEdited?: boolean;
  detectedLanguage: string;
  gitDiff?: {
    id: string;
    original: string;
    modified: string;
    binary: boolean;
    label: string;
    short: string;
  };
};
const editorTabs = ref<EditorTab[]>([]);
const markdownModes = ref<Record<string, "preview" | "source">>({});
const isMacOS = /Mac|iPhone|iPad/.test(navigator.platform);
const explorerKeys = explorerShortcutLabels(isMacOS);
const editorShortcutHints = computed(() => {
  const mod = isMacOS ? "⌘" : "Ctrl";
  return [
    { label: t("workbench.shortcut.send"), keys: [[ "Enter" ]] },
    { label: t("workbench.shortcut.newline"), keys: [[ "Shift", "Enter" ]] },
    { label: t("workbench.shortcut.newSession"), keys: [[ mod, "T" ]] },
    { label: t("workbench.shortcut.save"), keys: [[ mod, "S" ]] },
    { label: t("workbench.shortcut.attach"), keys: [[ mod, "I" ]] },
    { label: t("workbench.shortcut.closeTab"), keys: [[ mod, "W" ]] },
    { label: t("workbench.shortcut.closeSaved"), keys: [[ mod, "K" ], [ "U" ]] },
    { label: t("workbench.shortcut.closeAll"), keys: [[ mod, "K" ], [ "W" ]] },
    { label: t("workbench.shortcut.search"), keys: [[ mod, "Shift", "F" ]] },
    { label: t("workbench.shortcut.newTerminal"), keys: [[ mod, "`" ]] },
    { label: t("workbench.shortcut.zoomIn"), keys: [[ mod, "=" ]] },
    { label: t("workbench.shortcut.zoomOut"), keys: [[ mod, "-" ]] },
  ];
});
const activeEditorKey = ref("");
const editorTabsEl = ref<HTMLElement | null>(null);
const tabMenu = ref<{ x: number; y: number; tab: EditorTab } | null>(null);
let tabChord = false;
let tabChordTimer = 0;
const loading = ref(false);
const saving = ref(false);
const error = ref("");
const AGENT_DISK_CHANGE = () => t("workbench.agentDiskChange");
const GIT_DISK_CHANGE = () => t("workbench.gitDiskChange");
const externalChangePending = ref(false);
const externalChangeMessage = ref(AGENT_DISK_CHANGE());
const diskChangeNotes = new Map<string, string>();
const LEFT_TABS = ["files", "git", "search"] as const;
type LeftTab = (typeof LEFT_TABS)[number];
const leftTab = ref<LeftTab>("files");
const searchView = ref<{
  findIn: (workspaceId: string, include: string) => void;
  focusSearch: () => void;
} | null>(null);
const terminalOpen = ref(false);
const terminalHeight = ref(Number(localStorage.getItem("multara.panel.terminalHeight")) || 240);
const terminalTabs = ref<TerminalTab[]>([]);
const activeTerminalId = ref("");
const terminalProfiles = ref<backend.TerminalProfile[]>([]);
const defaultTerminalProfile = ref(localStorage.getItem("multara.terminal.defaultProfile") || "");
const pendingTerminalProfile = ref<string | undefined>();
const terminalPanel = ref<{ focus: () => void; write: (id: string, text: string) => void } | null>(null);
const terminalPick = ref(false);
const terminalPickQuery = ref("");
const terminalPickIndex = ref(0);
const terminalPickInput = ref<HTMLInputElement | null>(null);
const terminalPickEl = ref<HTMLElement | null>(null);
const newTerminalTitle = computed(() => t("workbench.newTerminal", { keys: isMacOS ? "⌘⇧`" : "Ctrl+Shift+`" }));
const terminalPickChoices = computed(() => {
  const query = terminalPickQuery.value.trim().toLowerCase();
  return workspaces.value.filter((workspace) => {
    if (!query) return true;
    const path = workspace.workPath || workspace.sourcePath;
    return workspace.name.toLowerCase().includes(query) || path.toLowerCase().includes(query);
  });
});
const sidebarTabsEl = ref<HTMLElement | null>(null);
const sidebarTabsOverflow = ref(false);
let sidebarTabsObserver: ResizeObserver | null = null;
const editorConfigByWorkspace = ref<Record<string, EditorConfigSection[]>>({});
const indentOverrides = ref<Record<string, { insertSpaces: boolean; tabSize: number }>>({});
const eolOverrides = ref<Record<string, EditorEol>>({});
const editorTheme = ref(monacoThemeName(currentDocumentTheme()));
const gitRepositories = ref<GitRepositoryStatus[]>([]);
const gitEpoch = ref(0);
const gitChangeCount = computed(() =>
  gitRepositories.value.reduce((sum, repository) => sum + repository.changes.length, 0),
);
const sourceControlPanel = ref<InstanceType<typeof SourceControlPanel> | null>(
  null,
);
const gitChangeRefreshScheduler = createRefreshScheduler(refreshGitViews);
const leftPanelWidth = ref(
  Number(localStorage.getItem("multara.workspace.leftWidth")) || 268,
);
const treeMenu = ref<{
  x: number;
  y: number;
  workspaceId: string;
  entry: FileEntry | null;
} | null>(null);
/** 复用仪表盘「添加项目」弹窗，由文件树右键「添加项目」或拖入文件夹打开 */
const showAddWorkspaceModal = ref(false);
const addWorkspaceInitialPaths = ref<string[]>([]);

function closeAddWorkspaceModal() {
  showAddWorkspaceModal.value = false;
  addWorkspaceInitialPaths.value = [];
}

function openAddWorkspaceModal(paths: string[] = []) {
  if (props.preview || !projectId.value) return;
  addWorkspaceInitialPaths.value = paths;
  showAddWorkspaceModal.value = true;
}

const treeEdit = ref<{
  workspaceId: string;
  parentPath: string;
  mode: "file" | "folder" | "rename";
  name: string;
  path?: string;
  depth: number;
} | null>(null);
const treeEditInput = ref<HTMLInputElement | null>(null);
const lastExplorerWorkspaceId = ref("");
const selectedTree = computed(() => {
  const item = explorer.focusedItem.value;
  return item ? { workspaceId: item.workspaceId, path: item.path, isDirectory: item.isDirectory } : null;
});

function rememberWorkspace(workspaceId: string) {
  lastExplorerWorkspaceId.value = workspaceId;
}

function treeItem(workspaceId: string, path: string, isDirectory: boolean): ExplorerItem {
  const workspace = workspaceById(workspaceId);
  return {
    key: explorerItemKey(workspaceId, path),
    workspaceId,
    path,
    name: path ? fileNameOf(path) : (workspace?.name ?? ""),
    isDirectory,
    readonly: false,
  };
}

function selectTreeEntry(
  workspaceId: string,
  path: string,
  isDirectory: boolean,
) {
  rememberWorkspace(workspaceId);
  explorer.select(treeItem(workspaceId, path, isDirectory));
}

function clearTreeSelection() {
  explorer.clearSelection();
}

function isTreeSelected(workspaceId: string, path: string) {
  return explorer.isSelected(explorerItemKey(workspaceId, path));
}

function isWorkspaceTreeSelected(workspaceId: string) {
  return explorer.isSelected(explorerItemKey(workspaceId, ""));
}

/** 没有选中项时与 VS Code 一样落到上次使用的工作区（否则第一个）的根目录。 */
function startCreateInSelection(isFolder: boolean) {
  const selected = selectedTree.value;
  if (!selected) {
    const workspace = workspaceById(lastExplorerWorkspaceId.value) ?? workspaces.value[0];
    if (workspace) startCreate(workspace.id, "", isFolder);
    return;
  }
  const parentPath = selected.isDirectory
    ? selected.path
    : parentPathOf(selected.path);
  startCreate(selected.workspaceId, parentPath, isFolder);
}

function setTreeEditInput(el: unknown) {
  treeEditInput.value =
    el instanceof HTMLInputElement ? el : treeEditInput.value;
}

const treeEditProblem = computed(() => {
  const edit = treeEdit.value;
  if (!edit) return null;
  const siblings = new Set(readTreeLevel(edit.workspaceId, edit.parentPath).map((entry) => entry.name));
  return validateFileName(edit.name, {
    currentName: edit.mode === "rename" && edit.path ? fileNameOf(edit.path) : undefined,
    siblingExists: (name) => siblings.has(name),
    isWindows: explorer.isWindows,
  });
});

function onTreeEditKeydown(event: KeyboardEvent) {
  if (event.key === "Enter") {
    event.preventDefault();
    if (treeEditProblem.value?.severity === "error") return;
    void commitTreeEdit();
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    treeEdit.value = null;
    void nextTick(focusSelectedTreeRow);
  }
}
const workspaceTitle = computed(() =>
  props.preview ? props.previewTitle : (project.value?.name ?? t("workbench.projectFallback")),
);
const workspaceSummary = computed(() => {
  if (props.preview) return t("workbench.testTitle");
  const gitCount = workspaces.value.filter(
    (item) => item.kind === "git",
  ).length;
  const folderCount = workspaces.value.length - gitCount;
  if (!gitCount && !folderCount) return t("workbench.noProjects");
  if (gitCount && folderCount) return t("workbench.gitAndFolders", { git: gitCount, folder: folderCount });
  if (gitCount) return `${gitCount} Git`;
  return t("workbench.foldersOnly", { n: folderCount });
});
// 需求名与项目数写到顶部标题栏。工作台被 keep-alive 缓存时不会卸载，
// 离开需求菜单只是 deactivated，必须在停用时清掉，避免标题残留到其他页面。
const titlebarActive = ref(!props.preview);
watchEffect(() => {
  if (props.preview || !titlebarActive.value) return;
  const title = workspaceTitle.value;
  const summary = workspaceSummary.value;
  const current = titlebarDetail.value;
  if (current?.title === title && current.summary === summary) return;
  titlebarDetail.value = { title, summary };
});
function releaseTitlebar() {
  if (props.preview) return;
  titlebarActive.value = false;
  titlebarDetail.value = null;
}
onActivated(() => {
  if (!props.preview) titlebarActive.value = true;
});
onDeactivated(releaseTitlebar);
onBeforeUnmount(releaseTitlebar);

function updateSidebarTabOverflow() {
  const el = sidebarTabsEl.value;
  if (!el) {
    sidebarTabsOverflow.value = false;
    return;
  }
  // 与整条 Tab 栏（含箭头位置）比较：箭头出现后列表会变窄，若只比列表自身宽度，箭头永远不会消失
  const available = el.parentElement?.clientWidth ?? el.clientWidth;
  sidebarTabsOverflow.value = el.scrollWidth > available + 1;
}

function scrollActiveSidebarTabIntoView() {
  const el = sidebarTabsEl.value;
  const active = el?.querySelector<HTMLElement>(".ui-button.active");
  if (!el || !active) return;
  const left = active.offsetLeft;
  const right = left + active.offsetWidth;
  if (left < el.scrollLeft) el.scrollLeft = left;
  else if (right > el.scrollLeft + el.clientWidth) {
    el.scrollLeft = right - el.clientWidth;
  }
}

function shiftSidebarTab(delta: -1 | 1) {
  const index = LEFT_TABS.indexOf(leftTab.value);
  const next = LEFT_TABS[index + delta];
  if (!next) return;
  leftTab.value = next;
}

function bindSidebarTabs() {
  const el = sidebarTabsEl.value;
  if (!el) return;
  sidebarTabsObserver?.disconnect();
  sidebarTabsObserver = new ResizeObserver(() => updateSidebarTabOverflow());
  sidebarTabsObserver.observe(el);
  const sidebar = el.closest(".workspace-sidebar");
  if (sidebar) sidebarTabsObserver.observe(sidebar);
  el.addEventListener("wheel", onSidebarTabsWheel, { passive: false });
  updateSidebarTabOverflow();
}

function entryGitChange(workspaceId: string, path: string) {
  if (workspaceById(workspaceId)?.kind !== "git") return null;
  return gitRepositoryByWorkspace.value.get(workspaceId)?.changes.find((change) => change.path === path) ?? null;
}

function entryGitChanges(workspaceId: string, path: string, isDirectory: boolean) {
  if (workspaceById(workspaceId)?.kind !== "git") return [];
  const changes = gitRepositoryByWorkspace.value.get(workspaceId)?.changes ?? [];
  if (!isDirectory) return changes.filter((change) => change.path === path);
  const prefix = path ? `${path}/` : "";
  return changes.filter((change) => !prefix || change.path === path || change.path.startsWith(prefix));
}

const treeMenuItems = computed<ContextMenuItem[]>(() => {
  const menu = treeMenu.value;
  if (!menu) return [];
  const workspace = workspaceById(menu.workspaceId);
  const targets = explorer.actionItems();
  const multi = targets.length > 1;
  const hasClipboard = Boolean(explorer.clipboard.value?.items.length) || !props.preview;
  const compareSource = explorer.compareSource.value;
  const pasteItem = { id: "paste", label: t("workbench.paste"), key: explorerKeys.paste, disabled: !hasClipboard };
  if (!menu.entry) {
    const items: ContextMenuItem[] = [
      { id: "new-file", label: t("workbench.newFile") },
      { id: "new-folder", label: t("workbench.newFolder") },
      { id: "add-project", label: t("workspace.addTitle") },
      { type: "sep" },
      { id: "reveal", label: t("workbench.reveal"), key: explorerKeys.reveal },
      { id: "open-terminal", label: t("workbench.openTerminal") },
      { type: "sep" },
      { id: "find-in-folder", label: t("workbench.findInWorkspace"), key: explorerKeys.findInFolder },
      { type: "sep" },
      pasteItem,
      { type: "sep" },
      { id: "copy-path", label: t("workbench.copyPath"), key: explorerKeys.copyPath },
      { id: "copy-rel", label: t("workbench.copyRel"), key: explorerKeys.copyRelative },
      { id: "copy-name", label: t("workbench.copyName") },
      { type: "sep" },
      { id: "refresh", label: t("workbench.refreshExplorer") },
      { id: "collapse", label: t("workbench.collapseProject") },
      { type: "sep" },
      { id: "detach", label: t("workbench.detachProject") },
    ];
    if (workspace?.kind === "git") {
      items.push({
        id: "copy-branch",
        label: t("workbench.copyBranch"),
        disabled: !workspaceBranchName(
          workspace,
          gitRepositoryByWorkspace.value.get(workspace.id)?.branch,
        ),
      });
    }
    return items;
  }
  const entry = menu.entry;
  const files = targets.filter((item) => !item.isDirectory);
  const gitChanges = targets.flatMap((item) => entryGitChanges(item.workspaceId, item.path, item.isDirectory));
  const isGit = workspace?.kind === "git";
  const items: ContextMenuItem[] = [];
  if (entry.isDirectory) {
    if (!multi) {
      items.push(
        { id: "new-file", label: t("workbench.newFile") },
        { id: "new-folder", label: t("workbench.newFolder") },
        { type: "sep" },
      );
    }
  } else {
    items.push(
      { id: "open", label: multi ? t("workbench.openFiles", { n: files.length }) : t("common.open") },
      { id: "open-pinned", label: t("workbench.openPinned"), key: explorerKeys.openSide, disabled: multi },
      { id: "open-external", label: t("workbench.openExternal"), disabled: multi || props.preview },
      { type: "sep" },
    );
  }
  items.push(
    { id: "attach", label: multi ? t("workbench.attachChatN", { n: targets.length }) : t("workbench.attachChat") },
    { id: "add-project", label: t("workspace.addTitle") },
    { type: "sep" },
    { id: "reveal", label: t("workbench.reveal"), key: explorerKeys.reveal },
    { id: "open-terminal", label: t("workbench.openTerminal") },
    { type: "sep" },
  );
  if (!entry.isDirectory) {
    if (multi && files.length === 2) items.push({ id: "compare-selected", label: t("workbench.compareSelected") });
    else if (!multi) {
      items.push({ id: "select-compare", label: t("workbench.selectCompare") });
      if (compareSource && compareSource.key !== explorerItemKey(menu.workspaceId, entry.path)) {
        items.push({ id: "compare-with", label: t("workbench.compareWith", { name: compareSource.name }) });
      }
    }
    items.push({ id: "open-timeline", label: t("workbench.openTimeline"), disabled: multi }, { type: "sep" });
  } else if (!multi) {
    items.push({ id: "find-in-folder", label: t("workbench.findInFolder"), key: explorerKeys.findInFolder }, { type: "sep" });
  }
  items.push(
    { id: "cut", label: t("workbench.cut"), key: explorerKeys.cut },
    { id: "copy", label: t("common.copy"), key: explorerKeys.copy },
    { ...pasteItem, disabled: multi || !hasClipboard },
    { type: "sep" },
    { id: "copy-path", label: t("workbench.copyPath"), key: explorerKeys.copyPath },
    { id: "copy-rel", label: t("workbench.copyRel"), key: explorerKeys.copyRelative },
    { id: "copy-name", label: t("workbench.copyName") },
  );
  if (isGit && !multi && !props.preview) items.push({ id: "copy-remote", label: t("workbench.copyRemote") });
  if (isGit) {
    const staged = gitChanges.filter((change) => change.indexStatus !== " " && change.indexStatus !== "?");
    const unstaged = gitChanges.filter((change) => change.worktreeStatus !== " ");
    items.push(
      { type: "sep" },
      { id: "git-open-changes", label: t("workbench.gitOpenChanges"), disabled: multi || entry.isDirectory || !gitChanges.length },
      { id: "git-stage", label: t("workbench.gitStage"), disabled: !unstaged.length },
      { id: "git-unstage", label: t("workbench.gitUnstage"), disabled: !staged.length },
      { id: "git-discard", label: t("workbench.gitDiscard"), disabled: !unstaged.length },
      { id: "git-ignore", label: t("workbench.gitIgnore") },
    );
  }
  items.push(
    { type: "sep" },
    { id: "rename", label: t("workbench.rename"), key: explorerKeys.rename, disabled: multi },
    { id: "delete", label: multi ? t("workbench.deleteMany", { count: targets.length }) : t("common.delete"), key: explorerKeys.delete },
  );
  return items;
});

function workspaceById(id: string): Workspace | undefined {
  return workspaces.value.find((item) => item.id === id);
}
const rightPanelWidth = ref(
  Number(localStorage.getItem("multara.workspace.rightWidth")) || 320,
);
const workbenchEl = ref<HTMLElement | null>(null);
const fileTreeEl = ref<HTMLElement | null>(null);
let stickyTreeObserver: ResizeObserver | null = null;

function syncTreeStickyHead() {
  const root = fileTreeEl.value;
  const head = root?.querySelector(".tree-workspace-head");
  if (!root || !(head instanceof HTMLElement) || head.offsetHeight <= 0) return;
  root.style.setProperty("--tree-sticky-head", `${head.offsetHeight}px`);
}

function bindTreeStickyHead() {
  syncTreeStickyHead();
  stickyTreeObserver?.disconnect();
  const head = fileTreeEl.value?.querySelector(".tree-workspace-head");
  if (!(head instanceof HTMLElement)) return;
  stickyTreeObserver = new ResizeObserver(() => syncTreeStickyHead());
  stickyTreeObserver.observe(head);
}

watch(
  () => workspaces.value.map((item) => item.id).join("\0"),
  () => nextTick(bindTreeStickyHead),
);
const editorWithDiff = ref<{
  getValue: () => string | undefined;
  getEditor: () => Monaco.editor.IStandaloneCodeEditor | null;
  layout: (force?: boolean) => void;
  reveal: (start: number, end?: number, column?: number, endColumn?: number) => void;
  ready: () => boolean;
  acceptAll: () => Promise<void>;
  undoAll: () => Promise<void>;
  revealDiffGroup: (index: number) => void;
} | null>(null);
const editorPendingChunks = ref(0);
const editorDiffGroupCount = ref(0);
const editorDiffGroupIndex = ref(0);
const codeEditor = shallowRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
const pendingReveal = ref<{ start: number; end?: number; column?: number; endColumn?: number } | null>(null);
const filePick = ref<{
  start?: number;
  end?: number;
  choices: Array<LocatedFile & { label: string }>;
} | null>(null);
const filePickDialog = ref<HTMLElement | null>(null);
const workbenchStyle = computed(() => ({
  gridTemplateColumns: `${leftPanelWidth.value}px 4px minmax(0, 1fr) 4px ${rightPanelWidth.value}px`,
}));
const dirty = computed(() =>
  Boolean(
    activeFile.value && activeFile.value.content !== originalContent.value,
  ),
);
const formatOnSave = computed(() => store.data?.settings.formatOnSave === "true");
const stickyScroll = computed(() => store.data?.settings.stickyScroll !== "false");
const activeEditorOptions = computed(() => {
  const file = activeFile.value;
  if (!file) return { insertSpaces: true, tabSize: 4, eol: "LF" as EditorEol };
  const key = editorLanguageKey(file.workspaceId, file.path);
  const fromConfig = editorOptionsFromConfig(
    matchEditorConfig(file.path, editorConfigByWorkspace.value[file.workspaceId] ?? []),
  );
  const detected = detectIndent(file.content);
  return {
    insertSpaces: indentOverrides.value[key]?.insertSpaces ?? fromConfig.insertSpaces ?? detected.insertSpaces,
    tabSize: indentOverrides.value[key]?.tabSize ?? fromConfig.tabSize ?? detected.tabSize,
    eol: eolOverrides.value[key] ?? fromConfig.eol ?? detectEol(file.content),
  };
});
const activeOutline = computed(() => {
  const file = activeFile.value;
  if (!file) return [];
  return outlineFromContent(file.language, file.content);
});
const activeBreadcrumbs = computed(() =>
  activeFile.value ? fileBreadcrumbs(activeFile.value.path) : [],
);
const activeSymbol = computed(() =>
  enclosingOutline(activeOutline.value, editorStatus.line),
);

async function loadWorkspaceEditorSettings(workspaceId: string) {
  if (props.preview) return;
  const sections: EditorConfigSection[] = [];
  for (const path of [".editorconfig", "src/.editorconfig"]) {
    try {
      const file = await backend.readWorkspaceFile(workspaceId, path);
      sections.push(...parseEditorConfig(file.content));
    } catch {
      // optional
    }
  }
  editorConfigByWorkspace.value = {
    ...editorConfigByWorkspace.value,
    [workspaceId]: sections,
  };
  // Vite 模板的 tsconfig.json 只放 references，paths 等写在 tsconfig.app.json，所以合并而不是取第一个。
  let compilerOptions: WorkspaceCompilerOptions = {};
  for (const name of ["tsconfig.json", "tsconfig.app.json", "tsconfig.web.json"]) {
    try {
      const file = await backend.readWorkspaceFile(workspaceId, name);
      compilerOptions = { ...compilerOptions, ...compilerOptionsFromTsconfig(parseTsconfigJson(file.content)) };
    } catch {
      // optional
    }
  }
  codeIndex.setCompilerOptions(workspaceId, compilerOptions);
}

function syncEditorDocument() {
  if (!ownsEditorStatus() || !activeFile.value) {
    if (ownsEditorStatus()) editorStatus.syncDocument(null);
    return;
  }
  const editor = codeEditor.value;
  const position = editor?.getPosition();
  const options = activeEditorOptions.value;
  editorStatus.syncDocument({
    line: position?.lineNumber ?? 1,
    column: position?.column ?? 1,
    eol: options.eol,
    insertSpaces: options.insertSpaces,
    tabSize: options.tabSize,
    outline: activeOutline.value,
  });
}

function diffFileKey(workspaceId: string, filePath: string) {
  return `${workspaceId}\n${filePath}`;
}

const pendingFileContents = ref(new Map<string, string>());
const pendingDiffFiles = computed(() => {
  void diffStore.diskEpoch;
  void diffStore.chunkStates;
  void diffStore.baselines;
  void diffStore.snapshots;
  void diffStore.turnOrder;
  const contents = new Map(pendingFileContents.value);
  for (const tab of editorTabs.value) {
    contents.set(diffFileKey(tab.workspaceId, tab.path), tab.file.content);
  }
  if (activeFile.value) {
    contents.set(
      diffFileKey(activeFile.value.workspaceId, activeFile.value.path),
      activeFile.value.content,
    );
  }
  return diffStore.listPendingDiffFiles(
    workspaces.value.map((workspace) => workspace.id),
    contents,
  );
});
const pendingDiffIndex = computed(() => {
  const file = activeFile.value;
  if (!file) return -1;
  return pendingDiffFiles.value.findIndex(
    (item) => item.workspaceId === file.workspaceId && item.filePath === file.path,
  );
});
const pendingDiffLabel = computed(() => {
  const total = pendingDiffFiles.value.length;
  const index = pendingDiffIndex.value;
  return `${index >= 0 ? index + 1 : 1}/${total}`;
});
const gitRepositoryByWorkspace = computed(
  () =>
    new Map(
      gitRepositories.value.map((repository) => [
        repository.workspaceId,
        repository,
      ]),
    ),
);

function treeKey(workspaceId: string, path: string) {
  return `${workspaceId}:${path}`;
}

const TREE_ROW_INSET = 18;
const TREE_INDENT = 14;
const TREE_CHEVRON = 16;

function treeRowPad(depth: number) {
  return TREE_ROW_INSET + depth * TREE_INDENT;
}

function treeGuideLeft(level: number) {
  return TREE_ROW_INSET + (level - 1) * TREE_INDENT + TREE_CHEVRON / 2;
}

let stopResizing: (() => void) | null = null;

const MIN_LEFT_PANEL = 160;
const MIN_RIGHT_PANEL = 240;
const MIN_EDITOR_PANEL = 160;

function panelBounds() {
  const total = workbenchEl.value?.clientWidth ?? window.innerWidth;
  const reserved = 8;
  const leftover = total - reserved - MIN_EDITOR_PANEL;
  const maxSide = Math.round(total * 0.72);
  return {
    maxLeft: Math.max(
      MIN_LEFT_PANEL,
      Math.min(maxSide, leftover - rightPanelWidth.value),
    ),
    maxRight: Math.max(
      MIN_RIGHT_PANEL,
      Math.min(maxSide, leftover - leftPanelWidth.value),
    ),
  };
}

function resizePanel(side: "left" | "right", delta: number) {
  const { maxLeft, maxRight } = panelBounds();
  if (side === "left") {
    leftPanelWidth.value = Math.min(
      maxLeft,
      Math.max(MIN_LEFT_PANEL, leftPanelWidth.value + delta),
    );
  } else {
    rightPanelWidth.value = Math.min(
      maxRight,
      Math.max(MIN_RIGHT_PANEL, rightPanelWidth.value + delta),
    );
  }
}

function setEditorsLiveLayout(_enabled: boolean) {
  editorWithDiff.value?.layout(true);
}

function beginResize(side: "left" | "right", event: PointerEvent) {
  event.preventDefault();
  stopResizing?.();
  const handle = event.currentTarget as HTMLElement;
  handle.setPointerCapture(event.pointerId);
  const startX = event.clientX;
  const startWidth =
    side === "left" ? leftPanelWidth.value : rightPanelWidth.value;
  const { maxLeft, maxRight } = panelBounds();
  const min = side === "left" ? MIN_LEFT_PANEL : MIN_RIGHT_PANEL;
  const max = side === "left" ? maxLeft : maxRight;
  document.body.classList.add("is-resizing-panels");
  setEditorsLiveLayout(false);

  let frame = 0;
  let pending: number | null = null;
  const commit = (width: number) => {
    if (side === "left") leftPanelWidth.value = width;
    else rightPanelWidth.value = width;
  };
  const move = (moveEvent: PointerEvent) => {
    const next =
      side === "left"
        ? startWidth + (moveEvent.clientX - startX)
        : startWidth - (moveEvent.clientX - startX);
    pending = Math.min(max, Math.max(min, next));
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (pending != null) commit(pending);
    });
  };
  const finish = () => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", finish);
    handle.removeEventListener("pointercancel", finish);
    if (frame) cancelAnimationFrame(frame);
    if (pending != null) commit(pending);
    if (handle.hasPointerCapture(event.pointerId))
      handle.releasePointerCapture(event.pointerId);
    document.body.classList.remove("is-resizing-panels");
    localStorage.setItem(
      "multara.workspace.leftWidth",
      String(leftPanelWidth.value),
    );
    localStorage.setItem(
      "multara.workspace.rightWidth",
      String(rightPanelWidth.value),
    );
    setEditorsLiveLayout(true);
    stopResizing = null;
  };
  stopResizing = finish;
  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", finish);
  handle.addEventListener("pointercancel", finish);
}

function resizeWithKeyboard(side: "left" | "right", event: KeyboardEvent) {
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  event.preventDefault();
  const direction = event.key === "ArrowRight" ? 1 : -1;
  resizePanel(side, side === "left" ? direction * 12 : direction * -12);
  layoutEditor();
}

/** 文件嵌套父项的展开状态；记录的是与 `explorer.fileNesting.expand` 默认值相反的项。 */
const toggledNests = ref(new Set<string>());

function isNestExpanded(workspaceId: string, path: string) {
  const toggled = toggledNests.value.has(treeKey(workspaceId, path));
  return explorer.settings.value.fileNestingExpand ? !toggled : toggled;
}

function toggleNest(workspaceId: string, path: string) {
  const next = new Set(toggledNests.value);
  const key = treeKey(workspaceId, path);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  toggledNests.value = next;
}

/** 已在编辑器打开的文件及其上级：即使被 files.exclude 隐藏也要显示。 */
const openEditorTreeKeys = computed(() => {
  const keys = new Set<string>();
  for (const tab of editorTabs.value) {
    if (tab.gitDiff) continue;
    let path = tab.path;
    while (path) {
      keys.add(treeKey(tab.workspaceId, path));
      path = parentPathOf(path);
    }
  }
  return keys;
});

function treeEntryHidden(workspaceId: string, entry: FileEntry, siblings: ReadonlySet<string>) {
  const settings = explorer.settings.value;
  if (isExcluded(entry.path, entry.name, settings.filesExclude, (name) => siblings.has(name))) return true;
  return settings.excludeGitIgnore && explorer.isIgnored(workspaceId, entry.path);
}

/** 新建/重命名中的文件夹不参与紧凑链，否则输入框没有位置。 */
function breakCompactAt(workspaceId: string, path: string) {
  const edit = treeEdit.value;
  if (!edit || edit.workspaceId !== workspaceId) return false;
  if (edit.mode === "rename") return edit.path === path || Boolean(edit.path?.startsWith(`${path}/`));
  return edit.parentPath === path || edit.parentPath.startsWith(`${path}/`);
}

const explorerRowsByWorkspace = computed(() => {
  const settings = explorer.settings.value;
  const sort = {
    sortOrder: settings.sortOrder,
    lexicographic: settings.sortOrderLexicographicOptions,
    reverse: settings.sortOrderReverse,
  };
  const query = explorer.findOpen.value ? explorer.findQuery.value.trim() : "";
  const result: Record<string, ExplorerRowNode[]> = {};
  for (const workspace of workspaces.value) {
    const nodes = buildExplorerRows({
      workspaceId: workspace.id,
      workspaceName: workspace.name,
      roots: workspaceRoots.value[workspace.id],
      childrenOf: (path) => directoryChildren.value[treeKey(workspace.id, path)],
      isExpanded: (path) => expandedDirectories.value.has(treeKey(workspace.id, path)),
      isNestExpanded: (path) => isNestExpanded(workspace.id, path),
      sort,
      compactFolders: settings.compactFolders,
      nestingPatterns: settings.fileNestingEnabled ? settings.fileNestingPatterns : null,
      isHidden: (entry, siblings) => treeEntryHidden(workspace.id, entry, siblings),
      forceVisible: (path) => openEditorTreeKeys.value.has(treeKey(workspace.id, path)),
      breakCompactAt: (path) => breakCompactAt(workspace.id, path),
      missingFiles: diffStore.listMissingFiles([workspace.id]).map((file) => file.filePath),
    });
    result[workspace.id] = query ? applyFind(nodes, query, explorer.findMode.value) : nodes;
  }
  return result;
});

function explorerNodes(workspaceId: string) {
  return explorerRowsByWorkspace.value[workspaceId] ?? [];
}

const visibleExplorerItems = computed<ExplorerItem[]>(() => {
  const items: ExplorerItem[] = [];
  const walk = (nodes: ExplorerRowNode[]) => {
    for (const node of nodes) {
      items.push({
        key: treeKey(node.entry.workspaceId, node.entry.path),
        workspaceId: node.entry.workspaceId,
        path: node.entry.path,
        name: node.entry.compact ? node.entry.compact.names.at(-1)! : node.entry.name,
        isDirectory: node.entry.isDirectory,
        readonly: node.entry.readonly,
      });
      if (node.expanded) walk(node.children);
    }
  };
  for (const workspace of workspaces.value) {
    items.push(treeItem(workspace.id, "", true));
    if (expandedWorkspaces.value.has(workspace.id)) walk(explorerNodes(workspace.id));
  }
  return items;
});

function listTreeLevel(workspaceId: string, path?: string): Promise<FileEntry[]> {
  if (props.preview) return backend.listWorkspaceDirectory(workspaceId, path);
  return explorerBackend.listDirectory(workspaceId, path).then((entries) => {
    explorer.noteEntries(workspaceId, entries);
    return entries;
  });
}

function loadedTreePaths(workspaceId: string) {
  const paths = (workspaceRoots.value[workspaceId] ?? []).map((entry) => entry.path);
  const prefix = `${workspaceId}:`;
  for (const [key, entries] of Object.entries(directoryChildren.value)) {
    if (key.startsWith(prefix)) paths.push(...entries.map((entry) => entry.path));
  }
  return paths;
}

function renameEditorsUnder(workspaceId: string, from: string, to: string) {
  for (const tab of [...editorTabs.value]) {
    if (tab.workspaceId !== workspaceId || tab.gitDiff) continue;
    if (tab.path === from) renameEditorTab(workspaceId, from, to);
    else if (tab.path.startsWith(`${from}/`)) renameEditorTab(workspaceId, tab.path, `${to}${tab.path.slice(from.length)}`);
  }
}

function editorsUnder(workspaceId: string, path: string) {
  return editorTabs.value.filter(
    (tab) => tab.workspaceId === workspaceId && (tab.path === path || tab.path.startsWith(`${path}/`)),
  );
}

function closeEditorsUnder(workspaceId: string, path: string) {
  for (const tab of editorsUnder(workspaceId, path)) {
    discardEditorChanges(tab);
    removeEditorTab(tab);
  }
}

async function expandTreeFolder(workspaceId: string, path: string) {
  expandedWorkspaces.value = new Set([...expandedWorkspaces.value, workspaceId]);
  if (!path) return;
  await loadTreeAncestors(workspaceId, `${path}/_`);
  expandTreeAncestors(workspaceId, `${path}/_`);
  const key = treeKey(workspaceId, path);
  if (!directoryChildren.value[key] && !props.preview) await reloadTreeLevel(workspaceId, path);
  expandedDirectories.value = new Set([...expandedDirectories.value, key]);
}

const explorer = useExplorer({
  preview: () => props.preview,
  workspaces: () => workspaces.value,
  visibleItems: () => visibleExplorerItems.value,
  levelEntries: (workspaceId, dirPath) =>
    dirPath ? directoryChildren.value[treeKey(workspaceId, dirPath)] : workspaceRoots.value[workspaceId],
  reloadLevel: (workspaceId, dirPath) => reloadTreeLevel(workspaceId, dirPath),
  reloadWorkspace: () => loadWorkspaceRoots({ silent: true, preserve: true }),
  loadedPaths: loadedTreePaths,
  isGitWorkspace: (workspaceId) => workspaceById(workspaceId)?.kind === "git",
  afterMutation: () => refreshGitViews(),
  gitChanged: () => gitChangeRefreshScheduler.request(),
  renameEditors: (workspaceId, from, to) => {
    renameEditorsUnder(workspaceId, from, to);
    renameLocalHistoryPath(workspaceId, from, to);
  },
  closeEditors: closeEditorsUnder,
  dirtyEditors: (workspaceId, path) => editorsUnder(workspaceId, path).filter((tab) => isEditorTabDirty(tab)).length,
  openFile: (workspaceId, path) => openEditorFile(workspaceId, path, { pin: true }),
  revealEntry: async (workspaceId, path, isDirectory) => {
    await loadTreeAncestors(workspaceId, path);
    expandTreeAncestors(workspaceId, path);
    selectTreeEntry(workspaceId, path, isDirectory);
    await nextTick();
    scrollTreeRowIntoView(path);
  },
  expandFolder: expandTreeFolder,
  absolutePath: (workspaceId, path) => {
    const workspace = workspaceById(workspaceId);
    return workspace ? joinWorkPath(workspace.workPath || workspace.sourcePath, path) : path;
  },
  previewDelete: (item) => {
    applyLocalDelete(item.workspaceId, parentPathOf(item.path), item.path);
    closeEditorsUnder(item.workspaceId, item.path);
  },
    reportError: (message) => {
    error.value = message;
  },
});

const explorerFindOpen = explorer.findOpen;
const explorerFindQuery = explorer.findQuery;
const explorerFindMode = explorer.findMode;
const explorerDialog = explorer.dialog;
const explorerCutKeys = explorer.cutKeys;
const explorerDragOver = explorer.dragOver;
let explorerChordK = false;
let explorerChordTimer = 0;
const outlineFollowCursor = ref(localStorage.getItem("multara.explorer.outlineFollow") !== "0");
const outlinePane = ref<{ collapseAll: () => void } | null>(null);
const openEditorsMenu = ref<{ x: number; y: number; tab: EditorTab } | null>(null);

function explorerRowOf(workspaceId: string, path: string): ExplorerRow | null {
  const walk = (nodes: ExplorerRowNode[]): ExplorerRow | null => {
    for (const node of nodes) {
      if (node.entry.path === path) return node.entry;
      const found = walk(node.children);
      if (found) return found;
    }
    return null;
  };
  return walk(explorerNodes(workspaceId));
}

function parseTreeKey(key: string): { workspaceId: string; path: string } | null {
  const split = key.indexOf(":");
  if (split <= 0) return null;
  return { workspaceId: key.slice(0, split), path: key.slice(split + 1) };
}

function treeItemFromPoint(x: number, y: number): ExplorerItem | null {
  const el = document.elementFromPoint(x, y)?.closest("[data-tree-key]");
  const key = el?.getAttribute("data-tree-key");
  if (!key) return null;
  const parsed = parseTreeKey(key);
  if (!parsed) return null;
  const row = parsed.path ? explorerRowOf(parsed.workspaceId, parsed.path) : null;
  return treeItem(parsed.workspaceId, parsed.path, row ? row.isDirectory : true);
}

async function focusExplorerItem(item: ExplorerItem) {
  rememberWorkspace(item.workspaceId);
  await nextTick();
  const row = fileTreeEl.value?.querySelector(`[data-tree-key="${CSS.escape(item.key)}"]`);
  if (row instanceof HTMLElement) {
    row.focus({ preventScroll: true });
    row.scrollIntoView({ block: "nearest" });
  }
}

function scrollTreeRowIntoView(path: string) {
  const row = fileTreeEl.value?.querySelector(`[data-tree-path="${CSS.escape(path)}"]`);
  if (row instanceof HTMLElement) row.scrollIntoView({ block: "nearest" });
}

let treeLoadGeneration = 0;

function hasExplorerTree() {
  return workspaces.value.some((workspace) => workspace.id in workspaceRoots.value);
}

function applyPreviewTree(expandDirectories = true) {
  workspaceRoots.value = {
    [previewWorkspace.id]: previewRootEntries,
    [previewWorkspaceB.id]: [{ name: "src", path: "src", isDirectory: true, depth: 0 }],
  };
  directoryChildren.value = { ...previewDirectoryChildren };
  if (expandDirectories) {
    expandedDirectories.value = new Set(Object.keys(previewDirectoryChildren));
  }
}

async function loadWorkspaceRoot(workspaceId: string) {
  if (props.preview) {
    applyPreviewTree();
    return;
  }
  workspaceRoots.value = await mergeFetchedRecordEntry(
    () => workspaceRoots.value,
    workspaceId,
    listTreeLevel(workspaceId),
  );
}

async function loadWorkspaceRoots(options?: { silent?: boolean; preserve?: boolean }) {
  const silent = Boolean(options?.silent);
  // 智能体改文件时只更新已加载的目录，不收起列表，也不跟着选中、打开文件。
  const preserve = Boolean(options?.preserve);
  const generation = ++treeLoadGeneration;
  const showLoading = !silent && !preserve && !hasExplorerTree();
  if (showLoading) loading.value = true;
  error.value = "";
  try {
    if (props.preview) {
      if (generation !== treeLoadGeneration) return;
      applyPreviewTree(!silent);
      if (silent) expandedDirectories.value = new Set();
      await revealOpenedFile();
      return;
    }
    const ids = workspaces.value.map((workspace) => workspace.id);
    const nextRoots = await recordFromFetchedKeys(ids, (id) =>
      listTreeLevel(id),
    );
    if (generation !== treeLoadGeneration) return;
    workspaceRoots.value = nextRoots;
    if (preserve) {
      const nextChildren = { ...directoryChildren.value };
      await Promise.all(Object.keys(directoryChildren.value).map(async (key) => {
        const split = key.indexOf(":");
        if (split < 0) return;
        const workspaceId = key.slice(0, split);
        const path = key.slice(split + 1);
        if (!ids.includes(workspaceId)) {
          delete nextChildren[key];
          return;
        }
        try {
          nextChildren[key] = await listTreeLevel(workspaceId, path);
        } catch {
          /* 这一层刷新失败时保留原来的列表 */
        }
      }));
      if (generation !== treeLoadGeneration) return;
      directoryChildren.value = nextChildren;
      return;
    }
    directoryChildren.value = {};
    expandedDirectories.value = new Set();
    await revealOpenedFile();
  } catch (cause) {
    if (generation !== treeLoadGeneration) return;
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (generation === treeLoadGeneration && showLoading) loading.value = false;
  }
}

function refreshExplorer() {
  void loadWorkspaceRoots({ silent: true });
  void refreshGitViews();
}

async function refreshGitViews() {
  if (sourceControlPanel.value) {
    await sourceControlPanel.value.refresh();
    return;
  }
  await refreshGitStatuses();
}

function noteDiskChange(workspaceId: string, path: string, message: string) {
  const key = editorTabKey(workspaceId, path);
  if (key === activeEditorKey.value) {
    externalChangeMessage.value = message;
    externalChangePending.value = true;
    return;
  }
  diskChangeNotes.set(key, message);
}

function editorPathAffected(tabPath: string, changedPath: string) {
  const normalized = changedPath.replace(/\/+$/, "");
  if (!normalized) return false;
  return tabPath === normalized || tabPath.startsWith(`${normalized}/`);
}

function openedFileTarget() {
  const tab = activeEditorTab();
  const workspaceId = tab?.workspaceId ?? activeFile.value?.workspaceId;
  const path = tab?.path ?? activeFile.value?.path;
  if (!workspaceId || !path) return null;
  if (!workspaces.value.some((workspace) => workspace.id === workspaceId)) {
    return null;
  }
  return { workspaceId, path };
}

function treeLevelEntries(workspaceId: string, dirPath: string) {
  if (!dirPath) return workspaceRoots.value[workspaceId] ?? [];
  return directoryChildren.value[treeKey(workspaceId, dirPath)] ?? [];
}

async function loadTreeAncestors(workspaceId: string, path: string) {
  if (!workspaceRoots.value[workspaceId] && !props.preview) {
    await loadWorkspaceRoot(workspaceId);
  }
  const parts = path.split("/").filter(Boolean);
  if (!parts.length) return false;
  const ancestors = parts.slice(0, -1);
  let prefix = "";
  for (const part of ancestors) {
    prefix = prefix ? `${prefix}/${part}` : part;
    const hit = treeLevelEntries(workspaceId, parentPathOf(prefix)).find(
      (entry) => entry.path === prefix,
    );
    if (!hit?.isDirectory) return false;
    const key = treeKey(workspaceId, prefix);
    if (!directoryChildren.value[key] && !props.preview) {
      try {
        directoryChildren.value = await mergeFetchedRecordEntry(
          () => directoryChildren.value,
          key,
          listTreeLevel(workspaceId, prefix),
        );
      } catch (cause) {
        error.value = cause instanceof Error ? cause.message : String(cause);
        return false;
      }
    }
  }
  return true;
}

function expandTreeAncestors(workspaceId: string, path: string) {
  expandedWorkspaces.value = new Set([...expandedWorkspaces.value, workspaceId]);
  const ancestors = path.split("/").filter(Boolean).slice(0, -1);
  const expanded = new Set(expandedDirectories.value);
  let prefix = "";
  for (const part of ancestors) {
    prefix = prefix ? `${prefix}/${part}` : part;
    expanded.add(treeKey(workspaceId, prefix));
  }
  expandedDirectories.value = expanded;
}

async function refreshTreeLevel(workspaceId: string, dirPath: string) {
  if (props.preview) return;
  if (!dirPath) {
    await loadWorkspaceRoot(workspaceId);
    return;
  }
  const key = treeKey(workspaceId, dirPath);
  directoryChildren.value = await mergeFetchedRecordEntry(
    () => directoryChildren.value,
    key,
    listTreeLevel(workspaceId, dirPath),
  );
}

let revealTreeToken = 0;

async function revealOpenedFile() {
  const token = ++revealTreeToken;
  const file = openedFileTarget();
  if (!file) {
    clearTreeSelection();
    return;
  }
  const ancestorsLoaded = await loadTreeAncestors(file.workspaceId, file.path);
  if (token !== revealTreeToken) return;
  const parent = parentPathOf(file.path);
  const listed = () =>
    treeLevelEntries(file.workspaceId, parent).some((entry) => entry.path === file.path);
  let onDisk = listed();
  const missing = diffStore.missingFiles.has(`${file.workspaceId}\n${file.path}`);
  if (ancestorsLoaded && !onDisk && !missing) {
    try {
      await refreshTreeLevel(file.workspaceId, parent);
      onDisk = listed();
    } catch {
      onDisk = false;
    }
  }
  if (token !== revealTreeToken) return;
  if (!ancestorsLoaded || (!onDisk && !missing)) {
    clearTreeSelection();
    return;
  }
  const settings = explorer.settings.value;
  if (!settings.autoReveal) return;
  if (isExcluded(file.path, fileNameOf(file.path), settings.autoRevealExclude, () => false)) return;
  expandTreeAncestors(file.workspaceId, file.path);
  selectTreeEntry(file.workspaceId, file.path, false);
  await nextTick();
  if (token !== revealTreeToken) return;
  if (settings.autoReveal === "focusNoScroll") {
    focusSelectedTreeRow();
    return;
  }
  fileTreeEl.value
    ?.querySelector(".tree-entry.selected")
    ?.scrollIntoView({ block: "nearest" });
}

async function toggleDirectory(workspaceId: string, entry: FileEntry) {
  const key = treeKey(workspaceId, entry.path);
  const expanded = new Set(expandedDirectories.value);
  if (expanded.has(key)) {
    expanded.delete(key);
    expandedDirectories.value = expanded;
    revealCollapsedFolder(entry.path);
    return;
  }
  if (!directoryChildren.value[key]) {
    if (props.preview) {
      expanded.add(key);
      expandedDirectories.value = expanded;
      return;
    }
    loadingDirectories.value = new Set([...loadingDirectories.value, key]);
    try {
      directoryChildren.value = await mergeFetchedRecordEntry(
        () => directoryChildren.value,
        key,
        listTreeLevel(workspaceId, entry.path),
      );
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause);
      return;
    } finally {
      const next = new Set(loadingDirectories.value);
      next.delete(key);
      loadingDirectories.value = next;
    }
  }
  expanded.add(key);
  expandedDirectories.value = expanded;
}

async function toggleExpandable(workspaceId: string, entry: ExplorerRow, recursive: boolean) {
  if (entry.nestParent) {
    toggleNest(workspaceId, entry.path);
    return;
  }
  if (!entry.isDirectory) return;
  const expanding = !expandedDirectories.value.has(treeKey(workspaceId, entry.path));
  if (!recursive) {
    await toggleDirectory(workspaceId, entry);
    return;
  }
  if (!expanding) {
    collapseDirectoryRecursive(workspaceId, entry.path);
    return;
  }
  await expandDirectoryRecursive(workspaceId, entry.path);
}

function collapseDirectoryRecursive(workspaceId: string, path: string) {
  const prefix = path ? `${path}/` : "";
  const next = new Set(expandedDirectories.value);
  next.delete(treeKey(workspaceId, path));
  for (const key of [...next]) {
    if (key.startsWith(`${workspaceId}:${prefix}`) || (path === "" && key.startsWith(`${workspaceId}:`))) {
      next.delete(key);
    }
  }
  expandedDirectories.value = next;
}

async function expandDirectoryRecursive(workspaceId: string, path: string) {
  await expandTreeFolder(workspaceId, path);
  const queue = [path];
  while (queue.length) {
    const current = queue.shift()!;
    const children = current ? directoryChildren.value[treeKey(workspaceId, current)] : workspaceRoots.value[workspaceId];
    for (const child of children ?? []) {
      if (!child.isDirectory) continue;
      await expandTreeFolder(workspaceId, child.path);
      queue.push(child.path);
    }
  }
}

function toggleWorkspace(workspaceId: string) {
  selectTreeEntry(workspaceId, "", true);
  const expanded = new Set(expandedWorkspaces.value);
  if (expanded.has(workspaceId)) expanded.delete(workspaceId);
  else expanded.add(workspaceId);
  expandedWorkspaces.value = expanded;
}

function collapseWorkspace(workspaceId: string) {
  rememberWorkspace(workspaceId);
  expandedWorkspaces.value = new Set(
    [...expandedWorkspaces.value].filter((item) => item !== workspaceId),
  );
}

let gitStatusTicket = 0;

async function refreshGitStatuses() {
  const ticket = ++gitStatusTicket;
  if (props.preview) {
    gitRepositories.value = [previewGitRepository, previewGitRepositoryB];
    gitEpoch.value += 1;
    return;
  }
  try {
    const next = await backend.listProjectGitStatus(projectId.value);
    if (ticket !== gitStatusTicket) return;
    gitRepositories.value = next;
  } catch {
    if (ticket !== gitStatusTicket) return;
    gitRepositories.value = [];
  }
  gitEpoch.value += 1;
}

function treeGitStatus(workspaceId: string, entry: FileEntry) {
  if (!entry.isDirectory && diffStore.missingFiles.has(`${workspaceId}\n${entry.path}`)) {
    return "D";
  }
  if (workspaceById(workspaceId)?.kind !== "git") return "";
  const statuses =
    gitRepositoryByWorkspace.value.get(workspaceId)?.changes ?? [];
  if (entry.isDirectory) {
    const prefix = `${entry.path}/`;
    return folderGitStatus(
      statuses
        .filter((change) => change.path === entry.path || change.path.startsWith(prefix))
        .map(gitStatusLetter),
    );
  }
  const status = statuses.find((item) => item.path === entry.path);
  return status ? gitStatusLetter(status) : "";
}

function treeGitBadgeTitle(workspaceId: string, entry: FileEntry) {
  const letter = treeGitStatus(workspaceId, entry);
  if (!letter) return "";
  if (!entry.isDirectory) return gitStatusTitle(letter);
  const statuses = gitRepositoryByWorkspace.value.get(workspaceId)?.changes ?? [];
  const prefix = `${entry.path}/`;
  const count = statuses.filter(
    (change) => change.path === entry.path || change.path.startsWith(prefix),
  ).length;
  return count > 0 ? t("git.folderDirty", { count }) : gitStatusTitle(letter);
}

function editorTabTitle(tab: EditorTab) {
  const diff = tab.gitDiff ? ` · ${tab.gitDiff.label}` : "";
  const base = tab.pinned ? tab.path : `${tab.path} · ${t("workbench.tabPreview")}`;
  if (!isEditorTabDirty(tab)) return `${base}${diff}`;
  return `${base}${diff} · ${t("workbench.tabDirtyHint")}`;
}

function treeGitClass(workspaceId: string, entry: FileEntry) {
  if (diffStore.missingFiles.has(`${workspaceId}\n${entry.path}`)) return "git-deleted";
  return gitStatusClass(treeGitStatus(workspaceId, entry));
}

function collapseExplorer() {
  expandedDirectories.value = new Set();
  expandedWorkspaces.value = new Set();
}

function dropWorkspaceUi(workspaceId: string) {
  for (const tab of editorTabs.value.filter((item) => item.workspaceId === workspaceId)) {
    removeEditorTab(tab);
  }
  for (const tab of terminalTabs.value.filter((item) => item.workspaceId === workspaceId)) {
    void killTerminal(tab.id);
  }
  if (selectedTree.value?.workspaceId === workspaceId) clearTreeSelection();
  expandedWorkspaces.value = new Set(
    [...expandedWorkspaces.value].filter((id) => id !== workspaceId),
  );
  const prefix = `${workspaceId}:`;
  expandedDirectories.value = new Set(
    [...expandedDirectories.value].filter((key) => !key.startsWith(prefix)),
  );
  const roots = { ...workspaceRoots.value };
  delete roots[workspaceId];
  workspaceRoots.value = roots;
  const children = { ...directoryChildren.value };
  for (const key of Object.keys(children)) {
    if (key.startsWith(prefix)) delete children[key];
  }
  directoryChildren.value = children;
  gitRepositories.value = gitRepositories.value.filter(
    (item) => item.workspaceId !== workspaceId,
  );
}

async function confirmDetachWorkspace(workspace: Workspace) {
  const repository =
    workspace.kind === "git"
      ? gitRepositoryByWorkspace.value.get(workspace.id)
      : undefined;
  // 文件夹没有 Git 状态。Git 项目在状态还没回来时用 null，避免把未知当成干净。
  const dirtyCount =
    workspace.kind === "git" ? (repository ? repository.changes.length : null) : 0;
  const unsavedCount = editorTabs.value.filter(
    (tab) => tab.workspaceId === workspace.id && isEditorTabDirty(tab),
  ).length;
  const sharedCheckout = (store.data?.workspaces ?? []).some(
    (item) => item.id !== workspace.id && item.workPath === workspace.workPath,
  );
  const message = detachWorkspacePrompt({
    name: workspace.name,
    mode: workspaceMode(workspace),
    isolationMode: workspace.isolationMode,
    branch: workspaceBranchName(workspace, repository?.branch),
    dirtyCount,
    unsavedCount,
    sharedCheckout,
  });
  if (!window.confirm(message)) return;
  try {
    if (props.preview) {
      detachedWorkspaceIds.value = new Set([
        ...detachedWorkspaceIds.value,
        workspace.id,
      ]);
    } else {
      await store.detachWorkspace(workspace.id);
    }
    dropWorkspaceUi(workspace.id);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function findInFolder(workspaceId: string, folderPath: string) {
  const folder = folderPath.replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
  leftTab.value = "search";
  void nextTick(() => searchView.value?.findIn(workspaceId, folder ? `./${folder}` : ""));
}

function openTreeMenu(
  workspaceId: string,
  entry: FileEntry | null,
  event: MouseEvent,
) {
  event.preventDefault();
  event.stopPropagation();
  rememberWorkspace(workspaceId);
  explorer.contextSelect(treeItem(workspaceId, entry?.path ?? "", entry ? entry.isDirectory : true));
  treeMenu.value = { x: event.clientX, y: event.clientY, workspaceId, entry };
}

async function copyText(value: string) {
  try {
    await navigator.clipboard.writeText(value);
  } catch {
    error.value = t("explorer.clipboard.failed");
  }
}

function parentPathOf(path: string) {
  const parts = path.split("/").filter(Boolean);
  return parts.length > 1 ? parts.slice(0, -1).join("/") : "";
}

function joinRelative(parentPath: string, name: string) {
  return parentPath ? `${parentPath}/${name}` : name;
}

function sortTreeEntries(entries: FileEntry[]) {
  return [...entries].sort((left, right) => {
    if (left.isDirectory !== right.isDirectory)
      return left.isDirectory ? -1 : 1;
    return left.name.localeCompare(right.name, "en");
  });
}

function readTreeLevel(workspaceId: string, parentPath: string) {
  return parentPath
    ? (directoryChildren.value[treeKey(workspaceId, parentPath)] ?? [])
    : (workspaceRoots.value[workspaceId] ?? []);
}

function writeTreeLevel(
  workspaceId: string,
  parentPath: string,
  entries: FileEntry[],
) {
  const next = sortTreeEntries(entries);
  if (!parentPath) {
    workspaceRoots.value = { ...workspaceRoots.value, [workspaceId]: next };
    return;
  }
  directoryChildren.value = {
    ...directoryChildren.value,
    [treeKey(workspaceId, parentPath)]: next,
  };
}

function applyLocalCreate(
  workspaceId: string,
  parentPath: string,
  name: string,
  isDirectory: boolean,
  depth: number,
) {
  const path = joinRelative(parentPath, name);
  const current = readTreeLevel(workspaceId, parentPath);
  if (current.some((item) => item.name === name)) {
    throw new Error(isDirectory ? t("workbench.error.folderExists") : t("workbench.error.fileExists"));
  }
  writeTreeLevel(workspaceId, parentPath, [
    ...current,
    { name, path, isDirectory, depth },
  ]);
  return path;
}

function applyLocalRename(
  workspaceId: string,
  parentPath: string,
  from: string,
  to: string,
  name: string,
) {
  const current = readTreeLevel(workspaceId, parentPath);
  if (current.some((item) => item.path !== from && item.name === name)) {
    throw new Error(t("workbench.error.targetExists"));
  }
  writeTreeLevel(
    workspaceId,
    parentPath,
    current.map((item) =>
      item.path === from ? { ...item, path: to, name } : item,
    ),
  );
}

function applyLocalDelete(
  workspaceId: string,
  parentPath: string,
  path: string,
) {
  writeTreeLevel(
    workspaceId,
    parentPath,
    readTreeLevel(workspaceId, parentPath).filter((item) => item.path !== path),
  );
}

async function reloadTreeLevel(workspaceId: string, parentPath: string) {
  if (props.preview) return;
  if (!parentPath) {
    workspaceRoots.value = await mergeFetchedRecordEntry(
      () => workspaceRoots.value,
      workspaceId,
      listTreeLevel(workspaceId),
    );
    return;
  }
  const key = treeKey(workspaceId, parentPath);
  directoryChildren.value = await mergeFetchedRecordEntry(
    () => directoryChildren.value,
    key,
    listTreeLevel(workspaceId, parentPath),
  );
}

function startCreate(
  workspaceId: string,
  parentPath: string,
  isFolder: boolean,
) {
  rememberWorkspace(workspaceId);
  expandedWorkspaces.value = new Set([
    ...expandedWorkspaces.value,
    workspaceId,
  ]);
  if (parentPath) {
    expandedDirectories.value = new Set([
      ...expandedDirectories.value,
      treeKey(workspaceId, parentPath),
    ]);
    if (
      !directoryChildren.value[treeKey(workspaceId, parentPath)] &&
      !props.preview
    ) {
      void (async () => {
        await reloadTreeLevel(workspaceId, parentPath);
        treeEdit.value = {
          workspaceId,
          parentPath,
          mode: isFolder ? "folder" : "file",
          name: "",
          depth: parentPath.split("/").length,
        };
        await nextTick();
        treeEditInput.value?.focus();
      })();
      return;
    }
  }
  treeEdit.value = {
    workspaceId,
    parentPath,
    mode: isFolder ? "folder" : "file",
    name: "",
    depth: parentPath ? parentPath.split("/").length : 0,
  };
  void nextTick(() => treeEditInput.value?.focus());
}

function startRename(workspaceId: string, entry: FileEntry) {
  rememberWorkspace(workspaceId);
  // 紧凑文件夹行的 name 是整条链，重命名只针对最后一级。
  const name = fileNameOf(entry.path);
  treeEdit.value = {
    workspaceId,
    parentPath: parentPathOf(entry.path),
    mode: "rename",
    name,
    path: entry.path,
    depth: entry.depth,
  };
  const [start, end] = renameSelection(name, entry.isDirectory);
  void nextTick(() => {
    treeEditInput.value?.focus({ preventScroll: true });
    treeEditInput.value?.setSelectionRange(start, end);
  });
}

async function commitTreeEdit() {
  const edit = treeEdit.value;
  if (!edit) return;
  // 失焦提交时名称有错就直接放弃，与 VS Code 一致。
  if (treeEditProblem.value?.severity === "error") {
    treeEdit.value = null;
    return;
  }
  const name = wellFormedFileName(edit.name).replace(/\\/g, "/").replace(/\/{2,}/g, "/");
  const nested = name.includes("/");
  const nextPath = joinRelative(edit.parentPath, name);
  const isDirectory =
    edit.mode === "folder" ||
    (edit.mode === "rename" && Boolean(readTreeLevel(edit.workspaceId, edit.parentPath).find((item) => item.path === edit.path)?.isDirectory));
  try {
    if (edit.mode === "rename" && edit.path && nextPath !== edit.path) {
      if (props.preview) {
        applyLocalRename(
          edit.workspaceId,
          edit.parentPath,
          edit.path,
          nextPath,
          name,
        );
        renameEditorsUnder(edit.workspaceId, edit.path, nextPath);
      } else {
        if (nested) {
          const [result] = await explorerBackend.transferEntries(
            [{ sourceWorkspaceId: edit.workspaceId, source: edit.path, targetWorkspaceId: edit.workspaceId, target: nextPath }],
            false,
          );
          if (result?.error) throw new Error(result.error.replace(explorerBackend.TARGET_EXISTS_PREFIX, t("explorer.error.existsPrefix")));
        } else {
          await backend.renameWorkspaceEntry(edit.workspaceId, edit.path, nextPath);
        }
        renameEditorsUnder(edit.workspaceId, edit.path, nextPath);
        explorer.recordRename(edit.workspaceId, edit.path, nextPath);
        await reloadTreeLevel(edit.workspaceId, edit.parentPath);
        if (nested) await reloadTreeLevel(edit.workspaceId, parentPathOf(nextPath)).catch(() => undefined);
        refreshGitViews();
      }
      await revealTreeEntry(edit.workspaceId, nextPath, isDirectory);
    } else if (edit.mode === "file" || edit.mode === "folder") {
      if (props.preview) {
        applyLocalCreate(edit.workspaceId, edit.parentPath, name, isDirectory, edit.depth);
      } else {
        await explorerBackend.createEntry(edit.workspaceId, nextPath, isDirectory).catch((cause) => {
          const message = cause instanceof Error ? cause.message : String(cause);
          throw new Error(message.replace(explorerBackend.TARGET_EXISTS_PREFIX, t("explorer.error.existsPrefix")));
        });
        explorer.recordCreate(edit.workspaceId, nextPath, isDirectory);
        await reloadTreeLevel(edit.workspaceId, edit.parentPath);
        if (nested) await loadTreeAncestors(edit.workspaceId, nextPath);
        refreshGitViews();
      }
      treeEdit.value = null;
      await revealTreeEntry(edit.workspaceId, nextPath, isDirectory);
      if (!isDirectory) {
        await openFile(edit.workspaceId, { name: fileNameOf(nextPath), path: nextPath, isDirectory: false, depth: edit.depth }, true);
      }
    }
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    treeEdit.value = null;
  }
}

async function revealTreeEntry(workspaceId: string, path: string, isDirectory: boolean) {
  expandTreeAncestors(workspaceId, path);
  selectTreeEntry(workspaceId, path, isDirectory);
  await nextTick();
  scrollTreeRowIntoView(path);
  focusSelectedTreeRow();
}

function onTreeMenuSelect(id: string) {
  const menu = treeMenu.value;
  treeMenu.value = null;
  if (!menu) return;
  const workspace = workspaceById(menu.workspaceId);
  if (!workspace) return;
  const createParent = menu.entry?.isDirectory
    ? menu.entry.path
    : menu.entry
      ? parentPathOf(menu.entry.path)
      : "";
  if (id === "new-file") {
    startCreate(workspace.id, createParent, false);
    return;
  }
  if (id === "new-folder") {
    startCreate(workspace.id, createParent, true);
    return;
  }
  if (id === "find-in-folder") {
    const folder = menu.entry?.isDirectory ? menu.entry.path : "";
    void findInFolder(workspace.id, folder);
    return;
  }
  if (id === "refresh") {
    refreshExplorer();
    return;
  }
  if (id === "collapse") {
    collapseWorkspace(workspace.id);
    return;
  }
  if (id === "detach") {
    void confirmDetachWorkspace(workspace);
    return;
  }
  const targets = explorer.actionItems();
  if (id === "copy-path") {
    void explorer.copyPaths(targets.length ? targets : [treeItem(workspace.id, "", true)], false);
    return;
  }
  if (id === "copy-branch") {
    const branch = workspaceBranchName(
      workspace,
      gitRepositoryByWorkspace.value.get(workspace.id)?.branch,
    );
    if (branch) void copyText(branch);
    return;
  }
  if (id === "copy-rel") {
    void explorer.copyPaths(targets.length ? targets : [treeItem(workspace.id, "", true)], true);
    return;
  }
  if (id === "copy-name") {
    const names = (targets.length ? targets : [treeItem(workspace.id, "", true)]).map((item) => item.name).filter(Boolean);
    if (names.length) void copyText(names.join("\n"));
    return;
  }
  if (id === "cut" || id === "copy") {
    void explorer.setClipboard(targets, id === "cut");
    return;
  }
  if (id === "paste") {
    void explorer.paste(menu.entry ? treeItem(workspace.id, menu.entry.path, menu.entry.isDirectory) : treeItem(workspace.id, "", true));
    return;
  }
  if (id === "open-external" && menu.entry) {
    void explorerBackend.openExternal(workspace.id, menu.entry.path).catch((cause) => {
      error.value = cause instanceof Error ? cause.message : String(cause);
    });
    return;
  }
  if (id === "open-pinned" && menu.entry) {
    onTreeOpenPinned(workspace.id, menu.entry);
    return;
  }
  if (id === "select-compare" && menu.entry) {
    explorer.compareSource.value = treeItem(workspace.id, menu.entry.path, false);
    return;
  }
  if (id === "compare-with" && menu.entry && explorer.compareSource.value) {
    void compareTreeFiles(explorer.compareSource.value, treeItem(workspace.id, menu.entry.path, false));
    return;
  }
  if (id === "compare-selected") {
    const files = targets.filter((item) => !item.isDirectory);
    if (files.length === 2) void compareTreeFiles(files[0], files[1]);
    return;
  }
  if (id === "open-timeline" && menu.entry) {
    void openTreeTimeline(workspace.id, menu.entry.path);
    return;
  }
  if (id === "copy-remote" && menu.entry) {
    void copyRemoteLink(workspace.id, menu.entry.path);
    return;
  }
  if (id === "git-open-changes" && menu.entry) {
    const change = entryGitChange(workspace.id, menu.entry.path);
    if (change) void openSourceControlChange({ workspaceId: workspace.id, path: change.path, group: gitChangeGroup(change) });
    return;
  }
  if (id === "git-stage" || id === "git-unstage" || id === "git-discard") {
    void runTreeGitAction(id, targets);
    return;
  }
  if (id === "git-ignore") {
    void addToGitignore(workspace.id, targets);
    return;
  }
  if (id === "open-terminal") {
    const relative = !menu.entry
      ? ""
      : menu.entry.isDirectory
        ? menu.entry.path
        : parentPathOf(menu.entry.path);
    void openIntegratedTerminal(workspace, relative);
    return;
  }
  if (id === "reveal") {
    void backend
      .revealWorkspacePath(workspace.id, menu.entry?.path)
      .catch((cause) => {
        error.value = cause instanceof Error ? cause.message : String(cause);
      });
    return;
  }
  if (id === "attach" && menu.entry) {
    for (const item of targets) {
      attachWorkspacePath({ workspaceId: item.workspaceId, path: item.path, isDirectory: item.isDirectory });
    }
    return;
  }
  // 与首页「添加项目」完全一致：打开空的 AddWorkspaceModal，由用户自己选目录
  if (id === "add-project") {
    openAddWorkspaceModal();
    return;
  }
  if (id === "rename" && menu.entry) {
    startRename(workspace.id, menu.entry);
    return;
  }
  if (id === "delete" && menu.entry) {
    void explorer.deleteItems(targets, false);
    return;
  }
  if (id === "open" && menu.entry) {
    const files = targets.filter((item) => !item.isDirectory);
    if (files.length > 1) {
      for (const item of files) void openEditorFile(item.workspaceId, item.path, { pin: true });
      return;
    }
    void onTreeOpen(workspace.id, menu.entry);
  }
}

function gitChangeGroup(change: GitFileStatus) {
  const groups = scmGroups([change]);
  if (groups.merge.length) return "merge" as const;
  if (groups.untracked.length) return "untracked" as const;
  if (groups.changes.length) return "changes" as const;
  return "staged" as const;
}

async function runTreeGitAction(id: "git-stage" | "git-unstage" | "git-discard", targets: ExplorerItem[]) {
  const byWorkspace = new Map<string, string[]>();
  for (const item of targets) {
    const changes = entryGitChanges(item.workspaceId, item.path, item.isDirectory).filter((change) =>
      id === "git-unstage"
        ? change.indexStatus !== " " && change.indexStatus !== "?"
        : change.worktreeStatus !== " ",
    );
    if (!changes.length) continue;
    byWorkspace.set(item.workspaceId, [...(byWorkspace.get(item.workspaceId) ?? []), ...changes.map((change) => change.path)]);
  }
  if (!byWorkspace.size) return;
  if (id === "git-discard") {
    const paths = [...byWorkspace.values()].flat();
    const untracked = [...byWorkspace].flatMap(([workspaceId, list]) =>
      list.filter((path) => entryGitChange(workspaceId, path)?.worktreeStatus === "?"),
    );
    const answer = await explorer.confirm({
      type: "warning",
      message:
        untracked.length === paths.length
          ? paths.length === 1
            ? t("workbench.gitDiscardDeleteOne", { name: fileNameOf(paths[0]) })
            : t("workbench.gitDiscardDeleteMany", { count: paths.length })
          : paths.length === 1
            ? t("workbench.gitDiscardRevertOne", { name: fileNameOf(paths[0]) })
            : t("workbench.gitDiscardRevertMany", { count: paths.length }),
      detail: untracked.length ? t("workbench.gitDiscardUntrackedDetail") : t("workbench.gitDiscardIrreversible"),
      primary: untracked.length === paths.length ? t("workbench.gitDiscardDeleteFiles") : t("workbench.gitDiscard"),
      danger: true,
    });
    if (!answer.confirmed) return;
  }
  try {
    for (const [workspaceId, paths] of byWorkspace) {
      if (id === "git-discard") await backend.gitDiscardFiles(workspaceId, paths);
      else await backend.gitStageFiles(workspaceId, paths, id === "git-stage");
    }
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
  refreshGitViews();
  if (id === "git-discard") {
    await loadWorkspaceRoots({ silent: true, preserve: true });
    for (const [workspaceId, paths] of byWorkspace) void reconcileOpenEditors(workspaceId, paths);
  }
}

async function addToGitignore(workspaceId: string, targets: ExplorerItem[]) {
  if (props.preview) return;
  const lines = targets.filter((item) => item.workspaceId === workspaceId && item.path).map((item) => `/${item.path}${item.isDirectory ? "/" : ""}`);
  if (!lines.length) return;
  try {
    let content = "";
    try {
      content = (await backend.readWorkspaceFile(workspaceId, ".gitignore")).content;
    } catch {
      content = "";
    }
    const existing = new Set(content.split(/\r?\n/).map((line) => line.trim()));
    const added = lines.filter((line) => !existing.has(line));
    if (!added.length) return;
    const eol = content.includes("\r\n") ? "\r\n" : "\n";
    const next = `${content}${content && !content.endsWith("\n") ? eol : ""}${added.join(eol)}${eol}`;
    if (content) await backend.writeWorkspaceFile(workspaceId, ".gitignore", next);
    else await backend.createWorkspaceFile(workspaceId, ".gitignore", next);
    await reloadTreeLevel(workspaceId, "");
    refreshGitViews();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function remoteWebUrl(remote: string) {
  let url = remote.trim().replace(/\.git$/, "");
  const scp = /^(?:[\w.-]+@)?([\w.-]+):(?!\/\/)(.+)$/.exec(url);
  if (scp) url = `https://${scp[1]}/${scp[2]}`;
  url = url.replace(/^ssh:\/\/(?:[\w.-]+@)?([\w.-]+)(?::\d+)?\//, "https://$1/").replace(/^git:\/\//, "https://");
  url = url.replace(/^https?:\/\/[^@/]+@/, "https://");
  return /^https?:\/\//.test(url) ? url : null;
}

async function copyRemoteLink(workspaceId: string, path: string) {
  try {
    const info = await explorerBackend.gitRemote(workspaceId);
    const base = info ? remoteWebUrl(info.remote) : null;
    if (!info || !base) {
      error.value = t("workbench.error.noOrigin");
      return;
    }
    const ref = info.commit || info.branch;
    const file = `${info.prefix}${path}`.split("/").map(encodeURIComponent).join("/");
    const host = new URL(base).hostname;
    const link = host.includes("gitlab")
      ? `${base}/-/blob/${ref}/${file}`
      : host.includes("bitbucket")
        ? `${base}/src/${ref}/${file}`
        : `${base}/blob/${ref}/${file}`;
    await copyText(link);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function readCompareSide(item: ExplorerItem) {
  if (props.preview) {
    const file = previewFileContent(item.path, item.workspaceId);
    return { content: file.content, language: file.language };
  }
  const file = await backend.readWorkspaceFile(item.workspaceId, item.path);
  return { content: file.content, language: file.language };
}

async function compareTreeFiles(left: ExplorerItem, right: ExplorerItem) {
  try {
    const [original, modified] = await Promise.all([readCompareSide(left), readCompareSide(right)]);
    const id = `compare:${left.key}\n${right.key}`;
    const label = `${left.name} ↔ ${right.name}`;
    const file: FileContent = {
      workspaceId: right.workspaceId,
      path: right.path,
      content: modified.content,
      language: modified.language,
      size: modified.content.length,
      readOnly: true,
    };
    const existing = editorTabs.value.find((tab) => tab.gitDiff?.id === id);
    const gitDiff = { id, original: original.content, modified: modified.content, binary: false, label, short: t("workbench.compareShort") };
    if (existing) {
      existing.gitDiff = gitDiff;
      existing.file = file;
      existing.original = modified.content;
      activateEditorTab(existing);
      return;
    }
    const tab: EditorTab = {
      workspaceId: right.workspaceId,
      path: right.path,
      pinned: true,
      file,
      original: modified.content,
      detectedLanguage: modified.language,
      gitDiff,
    };
    editorTabs.value = [...editorTabs.value, tab];
    activateEditorTab(tab);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function openTreeTimeline(workspaceId: string, path: string) {
  await openEditorFile(workspaceId, path, { pin: true });
  if (!showTimeline.value) toggleExplorerView("timeline");
  await nextTick();
  timelineUiPane.value?.setExpanded(true);
}

function editorTabKey(workspaceId: string, path: string) {
  return `${workspaceId}:${path}`;
}

function tabKey(tab: EditorTab) {
  return tab.gitDiff?.id ?? editorTabKey(tab.workspaceId, tab.path);
}

const activeGitDiff = computed(() => {
  const tab = editorTabs.value.find((item) => tabKey(item) === activeEditorKey.value);
  return tab?.gitDiff ?? null;
});

function isMarkdownFile(file: { path: string; language: string }) {
  return file.language === "markdown" || file.language === "mdx" || /\.(?:md|markdown|mdx)$/i.test(file.path);
}

const activeMedia = computed(() => {
  if (activeGitDiff.value) return "";
  const media = activeFile.value?.media;
  if (media === "image" || media === "audio" || media === "video") return media;
  return "";
});

const markdownToggleVisible = computed(() =>
  Boolean(activeFile.value && !activeGitDiff.value && isMarkdownFile(activeFile.value)),
);

const markdownMode = computed(() => {
  const file = activeFile.value;
  if (!file || !markdownToggleVisible.value) return "source";
  return markdownModes.value[editorLanguageKey(file.workspaceId, file.path)] ?? "source";
});

function setMarkdownMode(mode: "preview" | "source") {
  const file = activeFile.value;
  if (!file) return;
  if (mode === "preview") flushActiveEditorTab();
  const key = editorLanguageKey(file.workspaceId, file.path);
  markdownModes.value = { ...markdownModes.value, [key]: mode };
  if (mode === "source") void nextTick(() => editorWithDiff.value?.layout(true));
}

function openMarkdownLink(path: string) {
  const file = activeFile.value;
  if (!file) return;
  void openEditorFile(file.workspaceId, path, { pin: true });
}

function applyFileLanguage(file: FileContent): FileContent {
  return editorStatus.applyOverride(file);
}

function setActiveLanguage(language: string) {
  const file = activeFile.value;
  if (!file) return;
  activeFile.value = { ...file, language };
  const tab = findEditorTab(file.workspaceId, file.path);
  if (tab) tab.file = { ...tab.file, language };
}

function findEditorTab(workspaceId: string, path: string) {
  return editorTabs.value.find(
    (item) => !item.gitDiff && item.workspaceId === workspaceId && item.path === path,
  );
}

function readLiveEditorValue() {
  const component = editorWithDiff.value;
  if (component?.ready()) {
    const value = component.getValue();
    return value == null ? null : value;
  }
  const editor = codeEditor.value;
  const model = editor?.getModel();
  if (!editor || !model || model.isDisposed()) return null;
  return editor.getValue();
}

function flushActiveEditorTab() {
  const tab = editorTabs.value.find((item) => tabKey(item) === activeEditorKey.value);
  if (!tab || tab.gitDiff || !activeFile.value) return;
  const editorValue = readLiveEditorValue();
  if (editorValue != null) activeFile.value.content = editorValue;
  tab.file = activeFile.value;
  tab.original = originalContent.value;
}

const ignoreDirtyPin = ref(false);

async function refreshPendingFileContents() {
  if (props.preview) return;
  const workspaceIds = workspaces.value.map((workspace) => workspace.id);
  const next = new Map(pendingFileContents.value);
  for (const file of diffStore.listSnapshotFiles(workspaceIds)) {
    if (findEditorTab(file.workspaceId, file.filePath)) continue;
    if (
      activeFile.value?.workspaceId === file.workspaceId &&
      activeFile.value.path === file.filePath
    ) {
      continue;
    }
    try {
      const loaded = await backend.readWorkspaceFile(file.workspaceId, file.filePath);
      next.set(diffFileKey(file.workspaceId, file.filePath), loaded.content);
      diffStore.markFilePresent(file.workspaceId, file.filePath);
    } catch {
      if (diffStore.isMissingAccepted(file.workspaceId, file.filePath)) continue;
      next.set(diffFileKey(file.workspaceId, file.filePath), "");
      diffStore.markFileMissing(file.workspaceId, file.filePath);
    }
  }
  pendingFileContents.value = next;
}

const diffFileActionBusy = ref(false);

function isWriteTimelineItem(item: ChatMessage["timeline"][number]) {
  if (item.tool === "Edit" || item.tool === "Write" || item.tool === "Create") return true;
  const kind = itemKind(item);
  return kind === "edit" || kind === "write";
}

const filesBeingWritten = computed(() => {
  const keys = new Set<string>();
  for (const event of Object.values(store.runStates)) {
    if (event.status !== "running") continue;
    const timeline =
      store.messages[event.sessionId]?.find((message) => message.runId === event.runId)?.timeline ??
      [];
    const pending = new Map<string, string>();
    for (const item of timeline) {
      if (item.eventType === "tool_result") {
        pending.delete(item.callId || "__anon__");
        continue;
      }
      if (item.eventType !== "tool_use" || !isWriteTimelineItem(item)) continue;
      const raw = toolPath(item);
      if (!raw) continue;
      const resolved = resolveToolPath(raw);
      if (!resolved?.path) continue;
      pending.set(item.callId || "__anon__", `${resolved.workspaceId}\n${resolved.path}`);
    }
    for (const key of pending.values()) keys.add(key);
  }
  return keys;
});

const activeFileBeingWritten = computed(() => {
  const file = activeFile.value;
  if (!file) return false;
  return filesBeingWritten.value.has(`${file.workspaceId}\n${file.path}`);
});
const activeFileMissing = computed(() => {
  const file = activeFile.value;
  if (!file) return false;
  void diffStore.missingFiles;
  return diffStore.missingFiles.has(`${file.workspaceId}\n${file.path}`);
});
const acceptFileActionDisabled = computed(() => {
  if (diffFileActionBusy.value) return true;
  const pendingHere = pendingDiffIndex.value >= 0 || activeFileMissing.value;
  if (editorPendingChunks.value > 0) return activeFileBeingWritten.value;
  return !pendingHere;
});
const undoFileActionDisabled = computed(
  () =>
    diffFileActionBusy.value ||
    activeFileBeingWritten.value ||
    editorPendingChunks.value === 0,
);

function onEditorPendingChunks(count: number) {
  editorPendingChunks.value = count;
}

function onEditorDiffGroups(count: number) {
  editorDiffGroupCount.value = count;
  if (editorDiffGroupIndex.value > count - 1) editorDiffGroupIndex.value = 0;
}

function stepDiffGroup(delta: number) {
  const count = editorDiffGroupCount.value;
  if (!count) return;
  const next = (editorDiffGroupIndex.value + delta + count) % count;
  editorDiffGroupIndex.value = next;
  editorWithDiff.value?.revealDiffGroup(next);
}

watch(
  () =>
    activeFile.value
      ? `${activeFile.value.workspaceId}\n${activeFile.value.path}`
      : "",
  () => {
    editorPendingChunks.value = 0;
    editorDiffGroupCount.value = 0;
    editorDiffGroupIndex.value = 0;
  },
);

async function acceptAllCurrentFile() {
  const file = activeFile.value;
  if (!file || acceptFileActionDisabled.value) return;
  diffFileActionBusy.value = true;
  try {
    if (editorPendingChunks.value > 0) {
      await editorWithDiff.value?.acceptAll();
    }
    await diffStore.acceptMissingFile(file.workspaceId, file.path);
    const next = new Map(pendingFileContents.value);
    next.delete(diffFileKey(file.workspaceId, file.path));
    pendingFileContents.value = next;
  } finally {
    diffFileActionBusy.value = false;
  }
}

async function undoAllCurrentFile() {
  if (!activeFile.value || undoFileActionDisabled.value) return;
  diffFileActionBusy.value = true;
  try {
    await editorWithDiff.value?.undoAll();
  } finally {
    diffFileActionBusy.value = false;
  }
}

async function goPendingDiffFile(delta: number) {
  const files = pendingDiffFiles.value;
  if (!files.length) return;
  const current = pendingDiffIndex.value;
  const nextIndex =
    current < 0
      ? delta > 0
        ? 0
        : files.length - 1
      : (current + delta + files.length) % files.length;
  const target = files[nextIndex];
  await openEditorFile(target.workspaceId, target.filePath, { revealTree: true });
}

function activateEditorTab(tab: EditorTab) {
  flushActiveEditorTab();
  ignoreDirtyPin.value = true;
  const key = tabKey(tab);
  activeEditorKey.value = key;
  activeFile.value = tab.file;
  originalContent.value = tab.original;
  if (tab.gitDiff) {
    externalChangePending.value = false;
  } else {
    const note = diskChangeNotes.get(editorTabKey(tab.workspaceId, tab.path));
    if (note) {
      diskChangeNotes.delete(editorTabKey(tab.workspaceId, tab.path));
      externalChangeMessage.value = note;
      externalChangePending.value = true;
    } else {
      externalChangePending.value = false;
    }
  }
  void nextTick(() => {
    ignoreDirtyPin.value = false;
  });
  void revealOpenedFile();
}

function pinEditorTab(tab: EditorTab) {
  tab.pinned = true;
}

function isEditorTabDirty(tab: EditorTab) {
  if (tab.gitDiff) return false;
  if (tabKey(tab) === activeEditorKey.value) {
    return Boolean(
      activeFile.value && activeFile.value.content !== originalContent.value,
    );
  }
  return tab.file.content !== tab.original;
}

function tabSavedContent(tab: EditorTab) {
  if (tabKey(tab) === activeEditorKey.value) return originalContent.value;
  return tab.original;
}

function tabBuffer(tab: EditorTab) {
  if (tabKey(tab) === activeEditorKey.value) {
    return readLiveEditorValue() ?? activeFile.value?.content ?? tab.file.content;
  }
  return tab.file.content;
}

function userHoldsUnsavedEdits(tab: EditorTab, diskContent: string) {
  if (!tab.userEdited) return false;
  const saved = tabSavedContent(tab);
  const buffer = tabBuffer(tab);
  return buffer !== saved && buffer !== diskContent;
}

function clearEditorTabs() {
  editorTabs.value = [];
  activeEditorKey.value = "";
  activeFile.value = null;
  originalContent.value = "";
  if (ownsEditorStatus()) editorStatus.syncActive(null, null);
}

function renameEditorTab(workspaceId: string, from: string, to: string) {
  const tab = findEditorTab(workspaceId, from);
  if (!tab) return;
  tab.path = to;
  tab.file = { ...tab.file, path: to };
  editorStatus.renameOverride(editorTabKey(workspaceId, from), editorTabKey(workspaceId, to));
  if (activeEditorKey.value === editorTabKey(workspaceId, from)) {
    activeEditorKey.value = editorTabKey(workspaceId, to);
  }
  if (
    activeFile.value?.workspaceId === workspaceId &&
    activeFile.value.path === from
  ) {
    activeFile.value = { ...activeFile.value, path: to };
  }
}

function dirtyEditorTabs() {
  flushActiveEditorTab();
  return editorTabs.value.filter((tab) => isEditorTabDirty(tab));
}

function activeEditorTab() {
  return editorTabs.value.find((tab) => tabKey(tab) === activeEditorKey.value) ?? null;
}

function tabWorkspace(tab: EditorTab) {
  return workspaceById(tab.workspaceId);
}

function tabFullPath(tab: EditorTab) {
  const workspace = tabWorkspace(tab);
  return workspace ? joinWorkPath(workspace.workPath, tab.path) : tab.path;
}

function editorModelUri(workspaceId: string, path: string) {
  return monaco.Uri.parse(`/${workspaceId}/${path}`);
}

function adoptEditorModel(workspaceId: string, path: string, content: string) {
  const model = monaco.editor.getModel(editorModelUri(workspaceId, path));
  if (model && model.getValue() !== content) model.setValue(content);
}

function discardEditorChanges(tab: EditorTab) {
  if (tab.gitDiff) return;
  const saved = tab.original;
  tab.userEdited = false;
  tab.file = { ...tab.file, content: saved };
  if (tabKey(tab) === activeEditorKey.value) {
    activeFile.value = tab.file;
    originalContent.value = saved;
  }
  adoptEditorModel(tab.workspaceId, tab.path, saved);
}

async function confirmCloseTab(tab: EditorTab) {
  if (!isEditorTabDirty(tab)) return true;
  const action = await askUnsaved(
    t("workbench.unsavedSaveOne", { name: fileNameOf(tab.path) }),
  );
  if (action === "cancel") return false;
  if (action === "save") return saveTab(tab);
  discardEditorChanges(tab);
  return true;
}

async function confirmLeaveDirtyEditors() {
  if (terminalTabs.value.some((item) => item.busy)) {
    if (!window.confirm(t("workbench.terminalBusyLeave"))) return false;
  }
  const dirtyTabs = dirtyEditorTabs();
  if (!dirtyTabs.length) return true;
  if (dirtyTabs.length === 1) return confirmCloseTab(dirtyTabs[0]);
  const action = await askUnsaved(
    t("workbench.unsavedSaveMany", { count: dirtyTabs.length }),
    t("dialog.unsavedTitle"),
  );
  if (action === "cancel") return false;
  if (action === "discard") {
    for (const tab of dirtyTabs) discardEditorChanges(tab);
    return true;
  }
  for (const tab of dirtyTabs) {
    if (!(await saveTab(tab))) return false;
  }
  return true;
}

function removeEditorTab(tab: EditorTab) {
  const index = editorTabs.value.findIndex((item) => item === tab);
  editorTabs.value = editorTabs.value.filter((item) => item !== tab);
  if (activeEditorKey.value !== tabKey(tab)) return;
  const next =
    editorTabs.value[index] ??
    editorTabs.value[index - 1] ??
    editorTabs.value.at(-1);
  if (next) activateEditorTab(next);
  else {
    activeEditorKey.value = "";
    activeFile.value = null;
    originalContent.value = "";
  }
}

async function closeEditorTab(tab: EditorTab, event?: Event) {
  event?.preventDefault();
  event?.stopPropagation();
  tabMenu.value = null;
  if (!(await confirmCloseTab(tab))) return false;
  removeEditorTab(tab);
  return true;
}

async function closeEditorTabs(tabs: EditorTab[]) {
  for (const tab of tabs) {
    if (!(await closeEditorTab(tab))) return false;
  }
  return true;
}

function closeOtherEditorTabs(tab: EditorTab) {
  return closeEditorTabs(editorTabs.value.filter((item) => item !== tab));
}

function closeEditorTabsToTheRight(tab: EditorTab) {
  const index = editorTabs.value.indexOf(tab);
  return closeEditorTabs(editorTabs.value.slice(index + 1));
}

function closeSavedEditorTabs() {
  return closeEditorTabs(
    editorTabs.value.filter((item) => !isEditorTabDirty(item)),
  );
}

function closeAllEditorTabs() {
  return closeEditorTabs([...editorTabs.value]);
}

async function saveAllOpenEditors() {
  for (const tab of [...dirtyEditorTabs()]) {
    if (!(await saveTab(tab))) return;
  }
}

function onOpenEditorAux(event: MouseEvent, tab: EditorTab) {
  if (event.button === 1) void closeEditorTab(tab, event);
}

const showOpenEditors = ref(localStorage.getItem("multara.explorer.showOpenEditors") !== "0");
const showOutlineView = ref(localStorage.getItem("multara.explorer.showOutline") !== "0");
const showTimeline = ref(localStorage.getItem("multara.explorer.showTimeline") !== "0");
const explorerViewMenu = ref<{ x: number; y: number } | null>(null);
const timelinePane = ref<{
  openFilter: (event: MouseEvent) => void;
  refresh: () => Promise<void> | void;
  togglePin: () => void;
  pinned: boolean;
} | null>(null);
const timelineUiPane = ref<{ setExpanded: (value: boolean) => void } | null>(null);
const explorerViewMenuItems = computed<ContextMenuItem[]>(() => [
  { id: "openEditors", label: t("workbench.openEditors"), checked: showOpenEditors.value },
  { id: "outline", label: t("workbench.outline"), checked: showOutlineView.value },
  { id: "timeline", label: t("workbench.timeline"), checked: showTimeline.value },
]);

function openExplorerViewMenu(event: MouseEvent) {
  explorerViewMenu.value = { x: event.clientX, y: event.clientY };
}

function toggleExplorerView(id: string) {
  const target = id === "openEditors" ? showOpenEditors : id === "outline" ? showOutlineView : showTimeline;
  target.value = !target.value;
  const key = id === "openEditors"
    ? "multara.explorer.showOpenEditors"
    : id === "outline"
      ? "multara.explorer.showOutline"
      : "multara.explorer.showTimeline";
  localStorage.setItem(key, target.value ? "1" : "0");
  explorerViewMenu.value = null;
}

function activeEditorIsGit() {
  const tab = activeEditorTab();
  return Boolean(tab && tabWorkspace(tab)?.kind === "git");
}

function openTabMenu(tab: EditorTab, event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  window.getSelection()?.removeAllRanges();
  tabMenu.value = { x: event.clientX, y: event.clientY, tab };
}

const tabMenuItems = computed<ContextMenuItem[]>(() => {
  const tab = tabMenu.value?.tab;
  if (!tab) return [];
  const index = editorTabs.value.indexOf(tab);
  return [
    { id: "close", label: t("common.close"), key: "Ctrl+F4" },
    {
      id: "close-others",
      label: t("workbench.closeOthers"),
      disabled: editorTabs.value.length < 2,
    },
    {
      id: "close-right",
      label: t("workbench.closeRight"),
      disabled: index < 0 || index >= editorTabs.value.length - 1,
    },
    {
      id: "close-saved",
      label: t("workbench.closeSaved"),
      key: "Ctrl+K U",
      disabled: editorTabs.value.every((item) => isEditorTabDirty(item)),
    },
    {
      id: "close-all",
      label: t("workbench.closeAll"),
      key: "Ctrl+K W",
      disabled: editorTabs.value.length === 0,
    },
    { type: "sep" },
    { id: "save", label: t("common.save"), disabled: !isEditorTabDirty(tab) },
    { id: "revert", label: t("workbench.revertFile"), disabled: !isEditorTabDirty(tab) },
    { id: "compare-saved", label: t("workbench.compareSaved"), disabled: !isEditorTabDirty(tab) },
    { id: "pin", label: t("workbench.pin"), disabled: tab.pinned },
    { id: "reveal", label: t("workbench.revealInExplorer") },
    { type: "sep" },
    { id: "copy-path", label: t("workbench.copyPath"), key: "Shift+Alt+C" },
    { id: "copy-relative", label: t("workbench.copyRel"), key: "Ctrl+K Ctrl+Shift+C" },
  ];
});

function onTabMenuSelect(id: string) {
  const tab = tabMenu.value?.tab;
  tabMenu.value = null;
  if (!tab) return;
  if (id === "close") void closeEditorTab(tab);
  if (id === "close-others") void closeOtherEditorTabs(tab);
  if (id === "close-right") void closeEditorTabsToTheRight(tab);
  if (id === "close-saved") void closeSavedEditorTabs();
  if (id === "close-all") void closeAllEditorTabs();
  if (id === "copy-path") void copyText(tabFullPath(tab));
  if (id === "copy-relative") void copyText(tab.path);
  if (id === "save") void saveTab(tab);
  if (id === "revert") discardEditorChanges(tab);
  if (id === "pin") pinEditorTab(tab);
  if (id === "reveal") void revealEditorInTree(tab);
  if (id === "compare-saved") void compareTabWithSaved(tab);
}

const openEditorsMenuItems = computed<ContextMenuItem[]>(() => {
  const tab = openEditorsMenu.value?.tab;
  if (!tab) return [];
  return [
    { id: "close", label: t("common.close") },
    { id: "close-others", label: t("workbench.closeOthers"), disabled: editorTabs.value.length < 2 },
    { id: "save", label: t("common.save"), disabled: !isEditorTabDirty(tab) },
    { id: "revert", label: t("workbench.revert"), disabled: !isEditorTabDirty(tab) },
    { id: "compare-saved", label: t("workbench.compareSaved"), disabled: !isEditorTabDirty(tab) },
    { type: "sep" },
    { id: "pin", label: t("workbench.pin"), disabled: tab.pinned },
    { id: "reveal", label: t("workbench.revealInExplorer") },
    { type: "sep" },
    { id: "copy-path", label: t("workbench.copyPath") },
    { id: "copy-relative", label: t("workbench.copyRel") },
  ];
});

function openOpenEditorsMenu(tab: EditorTab, event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  openEditorsMenu.value = { x: event.clientX, y: event.clientY, tab };
}

function onOpenEditorsMenuSelect(id: string) {
  tabMenu.value = openEditorsMenu.value;
  openEditorsMenu.value = null;
  onTabMenuSelect(id);
}

function revealEditorInTree(tab: EditorTab) {
  leftTab.value = "files";
  void explorer.revealEntry(tab.workspaceId, tab.path, false);
}

async function compareTabWithSaved(tab: EditorTab) {
  if (tab.gitDiff) return;
  flushActiveEditorTab();
  const current = isEditorTabDirty(tab)
    ? (tabKey(tab) === activeEditorKey.value && activeFile.value ? activeFile.value.content : tab.file.content)
    : tab.file.content;
  const id = `saved:${tabKey(tab)}`;
  const file: FileContent = { ...tab.file, content: current, readOnly: true };
  const gitDiff = {
    id,
    original: tab.original,
    modified: current,
    binary: false,
    label: t("workbench.savedCompareLabel", { name: fileNameOf(tab.path) }),
    short: t("workbench.savedShort"),
  };
  const existing = editorTabs.value.find((item) => item.gitDiff?.id === id);
  if (existing) {
    existing.gitDiff = gitDiff;
    existing.file = file;
    existing.original = current;
    activateEditorTab(existing);
    return;
  }
  const next: EditorTab = {
    workspaceId: tab.workspaceId,
    path: tab.path,
    pinned: true,
    file,
    original: current,
    detectedLanguage: tab.detectedLanguage,
    gitDiff,
  };
  editorTabs.value = [...editorTabs.value, next];
  activateEditorTab(next);
}

async function restoreLocalHistoryEntry(entry: { workspaceId: string; path: string; content: string }) {
  try {
    if (props.preview) {
      const tab = findEditorTab(entry.workspaceId, entry.path);
      if (tab) {
        tab.file = { ...tab.file, content: entry.content };
        tab.userEdited = true;
        if (tabKey(tab) === activeEditorKey.value) activeFile.value = tab.file;
        adoptEditorModel(entry.workspaceId, entry.path, entry.content);
      }
      return;
    }
    await backend.writeWorkspaceFile(entry.workspaceId, entry.path, entry.content);
    await openEditorFile(entry.workspaceId, entry.path, { pin: true, force: true });
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function revealActiveExplorerFile() {
  const file = openedFileTarget();
  if (!file) return;
  leftTab.value = "files";
  void explorer.revealEntry(file.workspaceId, file.path, false);
}

function treeNameParts(entry: ExplorerRow) {
  return highlightName(entry.name, entry.matches);
}

function toggleOutlineFollow() {
  outlineFollowCursor.value = !outlineFollowCursor.value;
  localStorage.setItem("multara.explorer.outlineFollow", outlineFollowCursor.value ? "1" : "0");
}

const quickOpenVisible = ref(false);
const quickOpenFiles = ref<QuickOpenFile[]>([]);
const quickOpenLoading = ref(false);
const quickOpenSeed = ref("");

async function openQuickOpen(query = "") {
  quickOpenSeed.value = query;
  quickOpenVisible.value = true;
  quickOpenLoading.value = true;
  const files: QuickOpenFile[] = [];
  const multiple = workspaces.value.length > 1;
  for (const workspace of workspaces.value) {
    let entries: FileEntry[] = [];
    try {
      entries = await backend.listQuickOpenFiles(workspace.id);
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (entry.isDirectory) continue;
      const path = entry.path.replaceAll("\\", "/");
      const slash = path.lastIndexOf("/");
      files.push({
        workspaceId: workspace.id,
        path,
        label: slash >= 0 ? path.slice(slash + 1) : path,
        detail: workspaceFileLabel(workspace.name, slash >= 0 ? path.slice(0, slash) : "", multiple),
      });
    }
  }
  if (!quickOpenVisible.value) return;
  quickOpenFiles.value = files;
  quickOpenLoading.value = false;
}

function closeQuickOpen() {
  quickOpenVisible.value = false;
}

function openQuickOpenFile(file: QuickOpenFile, startLine?: number, endLine?: number) {
  quickOpenVisible.value = false;
  void openFileAt(file.workspaceId, file.path, startLine, endLine);
}

function handleWorkbenchShortcuts(event: KeyboardEvent) {
  const ctrl = event.ctrlKey || event.metaKey;
  if (ctrl && event.shiftKey && !event.altKey && event.code === "KeyE") {
    event.preventDefault();
    event.stopPropagation();
    leftTab.value = "files";
    void nextTick(focusSelectedTreeRow);
    return;
  }
  if (ctrl && !event.altKey && !event.shiftKey && event.code === "KeyP") {
    const target = event.target as HTMLElement | null;
    if (target?.closest(".chat-composer, .composer-field, textarea, [contenteditable='true']")) return;
    if (target instanceof HTMLInputElement && !target.closest(".quick-open")) return;
    event.preventDefault();
    event.stopPropagation();
    void openQuickOpen();
    return;
  }
  if (ctrl && event.shiftKey && !event.altKey && event.code === "Backquote") {
    event.preventDefault();
    createTerminal();
    return;
  }
  if (ctrl && !event.altKey && !event.shiftKey && event.code === "Backquote") {
    event.preventDefault();
    toggleTerminal();
    return;
  }
  // macOS 按住 Alt 时 event.key 会变成特殊字符，只能认物理键 KeyF。
  // 捕获阶段处理，避免焦点在编辑器里时被 Monaco 的 textarea 吃掉。
  if (!ctrl && event.altKey && event.shiftKey && event.code === "KeyF") {
    event.preventDefault();
    event.stopPropagation();
    const selected = selectedTree.value;
    if (selected) {
      const folder = selected.isDirectory ? selected.path : parentPathOf(selected.path);
      void findInFolder(selected.workspaceId, folder);
      return;
    }
    leftTab.value = "search";
    void nextTick(() => searchView.value?.focusSearch());
    return;
  }
  const target = event.target as HTMLElement | null;
  if (target?.closest(".chat-composer, .composer-field, input, textarea, [contenteditable='true']")) return;
  if (target?.closest(".file-tree, .explorer-folders")) {
    onExplorerKeydown(event);
  }
}

function toggleTerminal() {
  if (terminalOpen.value) {
    terminalOpen.value = false;
    return;
  }
  terminalOpen.value = true;
  const current = terminalTabs.value.find((item) => item.id === activeTerminalId.value);
  if (!current || current.pending) {
    const first = terminalTabs.value.find((item) => !item.pending) ?? terminalTabs.value[0];
    if (first) void materializeTerminal(first);
  }
  void nextTick(() => terminalPanel.value?.focus());
}

async function openNewTerminalPicker() {
  terminalOpen.value = true;
  if (!workspaces.value.length) return;
  if (workspaces.value.length === 1) {
    await openTerminalFor(workspaces.value[0]!, pendingTerminalProfile.value);
    return;
  }
  terminalPick.value = true;
  terminalPickQuery.value = "";
  terminalPickIndex.value = 0;
  await nextTick();
  terminalPickInput.value?.focus();
}

function pendingTerminal(workspace: Workspace): TerminalTab {
  return {
    id: `pending:${workspace.id}`,
    title: workspace.name,
    cwd: workspace.workPath || workspace.sourcePath,
    workspaceId: workspace.id,
    live: false,
    pending: true,
    message: "",
    groupId: `pending:${workspace.id}`,
  };
}

async function startTerminal(
  cwd: string,
  title: string,
  workspaceId: string,
  options?: { profile?: string; groupId?: string },
): Promise<TerminalTab> {
  let id = "";
  let live = true;
  let message = "";
  let profileId = options?.profile || defaultTerminalProfile.value || undefined;
  let shell = "";
  let pid = 0;
  try {
    const opened = await backend.terminalOpen(cwd, profileId);
    id = opened.id;
    profileId = opened.profileId;
    shell = opened.shell;
    pid = opened.pid;
  } catch (error) {
    live = false;
    id = `offline-${crypto.randomUUID()}`;
    message = error instanceof Error ? error.message : t("workbench.error.terminalStart");
  }
  if (live && !props.preview) {
    trackTerminal({ id, title, projectId: projectId.value, workspaceId });
  }
  return {
    id,
    title,
    cwd,
    workspaceId,
    live,
    pending: false,
    message,
    groupId: options?.groupId ?? id,
    profileId,
    shell,
    pid,
  };
}

async function materializeTerminal(tab: TerminalTab) {
  if (!tab.pending) {
    activeTerminalId.value = tab.id;
    terminalOpen.value = true;
    return;
  }
  const created = await startTerminal(tab.cwd, tab.title, tab.workspaceId, {
    profile: pendingTerminalProfile.value,
    groupId: tab.groupId,
  });
  pendingTerminalProfile.value = undefined;
  terminalTabs.value = terminalTabs.value.map((item) => (item.id === tab.id ? created : item));
  activeTerminalId.value = created.id;
  terminalOpen.value = true;
}

function selectTerminal(id: string) {
  const tab = terminalTabs.value.find((item) => item.id === id);
  if (!tab) return;
  if (tab.pending) {
    void materializeTerminal(tab);
    return;
  }
  activeTerminalId.value = id;
}

async function openTerminalFor(workspace: Workspace, profile?: string) {
  terminalPick.value = false;
  terminalOpen.value = true;
  const chosen = profile ?? pendingTerminalProfile.value;
  pendingTerminalProfile.value = chosen;
  const pending = terminalTabs.value.find((tab) => tab.pending && tab.workspaceId === workspace.id);
  if (pending) {
    await materializeTerminal(pending);
    return;
  }
  const created = await startTerminal(
    workspace.workPath || workspace.sourcePath,
    workspace.name,
    workspace.id,
    { profile: chosen },
  );
  pendingTerminalProfile.value = undefined;
  terminalTabs.value = [...terminalTabs.value, created];
  activeTerminalId.value = created.id;
}

async function openIntegratedTerminal(workspace: Workspace, relativeDir: string) {
  terminalPick.value = false;
  terminalOpen.value = true;
  const cwd = joinWorkPath(workspace.workPath || workspace.sourcePath, relativeDir);
  const folder = relativeDir ? fileNameOf(relativeDir) : "";
  const title = folder ? `${workspace.name} · ${folder}` : workspace.name;
  const created = await startTerminal(cwd, title, workspace.id);
  terminalTabs.value = [...terminalTabs.value, created];
  activeTerminalId.value = created.id;
}

function createTerminal(profile?: string) {
  if (typeof profile === "string") pendingTerminalProfile.value = profile;
  void openNewTerminalPicker();
}

async function splitTerminal() {
  const active = terminalTabs.value.find((item) => item.id === activeTerminalId.value);
  terminalOpen.value = true;
  if (!active || active.pending) {
    createTerminal();
    return;
  }
  const groupId = active.groupId || active.id;
  if (!active.groupId) active.groupId = groupId;
  const created = await startTerminal(active.cwd, active.title, active.workspaceId, {
    profile: active.profileId,
    groupId,
  });
  const index = terminalTabs.value.findIndex((item) => item.id === active.id);
  const tabs = [...terminalTabs.value];
  tabs.splice(index + 1, 0, created);
  terminalTabs.value = tabs;
  activeTerminalId.value = created.id;
  void nextTick(() => terminalPanel.value?.focus());
}

function patchTerminal(id: string, patch: Partial<TerminalTab>) {
  terminalTabs.value = terminalTabs.value.map((item) => (item.id === id ? { ...item, ...patch } : item));
  const tab = terminalTabs.value.find((item) => item.id === id);
  if (!tab?.live) return;
  const running = runningTerminals.value.find((item) => item.id === id);
  if (running) running.title = tab.customTitle || tab.processTitle || tab.title;
}

function setDefaultTerminalProfile(id: string) {
  defaultTerminalProfile.value = id;
  localStorage.setItem("multara.terminal.defaultProfile", id);
}

async function openTerminalPath(payload: { path: string; isDirectory: boolean; line?: number; column?: number }) {
  const located = await locateWorkspaceFiles(payload.path);
  const match = located[0];
  if (!match) return;
  if (payload.isDirectory) {
    leftTab.value = "files";
    void explorer.revealEntry(match.workspaceId, match.path, true);
    return;
  }
  await openEditorFile(match.workspaceId, match.path, {
    start: payload.line,
    end: payload.line,
    column: payload.column,
  });
}

function runActiveFileInTerminal() {
  const file = activeFile.value;
  if (!file) return;
  const workspace = workspaces.value.find((item) => item.id === file.workspaceId);
  const full = joinWorkPath(workspace?.workPath || workspace?.sourcePath || "", file.path);
  const tab = terminalTabs.value.find((item) => item.id === activeTerminalId.value);
  const quoted = /\s/.test(full)
    ? (tab?.shell?.includes("命令提示符") ? `"${full}"` : `'${full.replaceAll("'", "''")}'`)
    : full;
  void sendToActiveTerminal(`${quoted}\n`);
}

function runSelectionInTerminal() {
  const editor = codeEditor.value ?? editorWithDiff.value?.getEditor?.() ?? null;
  const model = editor?.getModel();
  const selection = editor?.getSelection();
  if (!editor || !model || !selection || selection.isEmpty()) return;
  const text = model.getValueInRange(selection);
  if (!text.trim()) return;
  void sendToActiveTerminal(text.endsWith("\n") ? text : `${text}\n`);
}

async function sendToActiveTerminal(text: string) {
  terminalOpen.value = true;
  const current = terminalTabs.value.find((item) => item.id === activeTerminalId.value);
  if (!current || current.pending || !current.live) {
    const workspace = workspaces.value.find((item) => item.id === current?.workspaceId) ?? workspaces.value[0];
    if (workspace) await openTerminalFor(workspace);
  }
  await nextTick();
  const id = activeTerminalId.value;
  if (id) terminalPanel.value?.write(id, text);
}

function reorderTerminal(fromId: string, toId: string, after: boolean) {
  const tabs = [...terminalTabs.value];
  const from = tabs.findIndex((item) => item.id === fromId);
  let to = tabs.findIndex((item) => item.id === toId);
  if (from < 0 || to < 0 || from === to) return;
  const [item] = tabs.splice(from, 1);
  if (from < to) to -= 1;
  // 指针过了目标标签中线才插入到它后面，避免来回抖动。
  tabs.splice(after ? to + 1 : to, 0, item!);
  if (tabs.every((tab, index) => tab.id === terminalTabs.value[index]?.id)) return;
  terminalTabs.value = tabs;
}

async function killTerminal(id: string) {
  const tab = terminalTabs.value.find((item) => item.id === id);
  terminalTabs.value = terminalTabs.value.filter((item) => item.id !== id);
  if (tab?.live) {
    forgetTerminal(id);
    try {
      await backend.terminalClose(id);
    } catch {
      /* 终端进程已经结束 */
    }
  }
  if (activeTerminalId.value === id) {
    activeTerminalId.value = terminalTabs.value.find((item) => !item.pending)?.id
      ?? terminalTabs.value[0]?.id
      ?? "";
  }
}

function onTerminalPickOutside(event: PointerEvent) {
  if (!terminalPick.value || terminalPickQuery.value.trim()) return;
  const root = terminalPickEl.value;
  if (root && event.target instanceof Node && root.contains(event.target)) return;
  terminalPick.value = false;
}

watch(
  () => workspaces.value.map((workspace) => workspace.id).join("\0"),
  () => {
    const list = workspaces.value;
    if (!list.length) return;
    const known = new Set(terminalTabs.value.map((tab) => tab.workspaceId));
    const switched = terminalTabs.value.length > 0 && !list.some((workspace) => known.has(workspace.id));
    if (switched) {
      for (const tab of terminalTabs.value) {
        if (!tab.live) continue;
        forgetTerminal(tab.id);
        void backend.terminalClose(tab.id);
      }
      terminalTabs.value = [];
      activeTerminalId.value = "";
    }
    for (const workspace of list) {
      if (terminalTabs.value.some((tab) => tab.workspaceId === workspace.id)) continue;
      terminalTabs.value.push(pendingTerminal(workspace));
    }
    if (terminalOpen.value && !terminalTabs.value.some((tab) => tab.id === activeTerminalId.value && !tab.pending)) {
      const first = terminalTabs.value[0];
      if (first) void materializeTerminal(first);
    }
  },
  { immediate: true },
);

watch(terminalFocus, (request) => {
  if (!request || props.preview) return;
  const tab = terminalTabs.value.find((item) => item.id === request.id);
  if (!tab) return;
  if (!workspaces.value.some((workspace) => workspace.id === tab.workspaceId)) return;
  terminalOpen.value = true;
  activeTerminalId.value = tab.id;
  if (tab.pending) void materializeTerminal(tab);
});

watch(terminalPick, (open) => {
  if (open) window.addEventListener("pointerdown", onTerminalPickOutside, true);
  else window.removeEventListener("pointerdown", onTerminalPickOutside, true);
});

function confirmTerminalPick() {
  const choice = terminalPickChoices.value[terminalPickIndex.value] ?? terminalPickChoices.value[0];
  if (choice) void openTerminalFor(choice);
}

function moveTerminalPick(delta: number) {
  const count = terminalPickChoices.value.length;
  if (!count) return;
  terminalPickIndex.value = (terminalPickIndex.value + delta + count) % count;
}

function beginTerminalResize(event: PointerEvent) {
  event.preventDefault();
  const handle = event.currentTarget instanceof HTMLElement
    ? event.currentTarget
    : event.target instanceof HTMLElement
      ? event.target
      : null;
  try {
    handle?.setPointerCapture(event.pointerId);
  } catch {
    /* 合成事件不能捕获指针 */
  }
  const startY = event.clientY;
  const start = terminalHeight.value;
  const limit = Math.max(200, (workbenchEl.value?.clientHeight ?? window.innerHeight) - 160);
  document.body.classList.add("is-resizing-row");
  const move = (moveEvent: PointerEvent) => {
    terminalHeight.value = Math.min(limit, Math.max(120, start + startY - moveEvent.clientY));
  };
  const up = () => {
    localStorage.setItem("multara.panel.terminalHeight", String(terminalHeight.value));
    document.body.classList.remove("is-resizing-row");
    handle?.removeEventListener("pointermove", move);
    handle?.removeEventListener("pointerup", up);
    handle?.removeEventListener("pointercancel", up);
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
  };
  handle?.addEventListener("pointermove", move);
  handle?.addEventListener("pointerup", up);
  handle?.addEventListener("pointercancel", up);
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

async function applySearchReplace(workspaceId: string, path: string, content: string) {
  const tab = editorTabs.value.find((item) => item.workspaceId === workspaceId && item.path === path && !item.gitDiff);
  if (tab && isEditorTabDirty(tab)) return false;
  if (!props.preview) await backend.writeWorkspaceFile(workspaceId, path, content);
  if (tab) {
    tab.file = { ...tab.file, content, size: content.length };
    tab.original = content;
  }
  return true;
}

function handleEditorTabKeys(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  if (
    target?.closest(
      ".chat-composer, .composer-field, .ui-input, .ui-textarea, .tree-inline-input",
    )
  )
    return;
  const ctrl = event.ctrlKey || event.metaKey;
  const key = event.key.toLowerCase();
  if (tabChord) {
    window.clearTimeout(tabChordTimer);
    tabChord = false;
    if (ctrl && event.shiftKey && key === "c") {
      event.preventDefault();
      const tab = tabMenu.value?.tab ?? activeEditorTab();
      if (tab) void copyText(tab.path);
      return;
    }
    if (key === "u") {
      event.preventDefault();
      void closeSavedEditorTabs();
      return;
    }
    if (key === "w") {
      event.preventDefault();
      void closeAllEditorTabs();
      return;
    }
    return;
  }
  if (ctrl && !event.altKey && !event.shiftKey && key === "k") {
    event.preventDefault();
    tabChord = true;
    tabChordTimer = window.setTimeout(() => {
      tabChord = false;
    }, 1500);
    return;
  }
  if (
    ctrl &&
    !event.altKey &&
    !event.shiftKey &&
    (key === "w" || event.key === "F4")
  ) {
    const tab = activeEditorTab();
    if (!tab) return;
    event.preventDefault();
    void closeEditorTab(tab);
    return;
  }
  if (event.shiftKey && event.altKey && key === "c") {
    const tab = activeEditorTab();
    if (!tab) return;
    event.preventDefault();
    void copyText(tabFullPath(tab));
  }
}

function onEditorTabsWheel(event: WheelEvent) {
  const el = editorTabsEl.value;
  if (!el || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
  if (el.scrollWidth <= el.clientWidth) return;
  event.preventDefault();
  el.scrollLeft += event.deltaY;
}

function onSidebarTabsWheel(event: WheelEvent) {
  const el = sidebarTabsEl.value;
  if (!el || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
  if (el.scrollWidth <= el.clientWidth + 1) return;
  event.preventDefault();
  el.scrollLeft += event.deltaY;
  updateSidebarTabOverflow();
}

async function openEditorFile(
  workspaceId: string,
  path: string,
  options?: {
    pin?: boolean;
    start?: number;
    end?: number;
    column?: number;
    endColumn?: number;
    force?: boolean;
    notifyIfDirty?: boolean;
    revealTree?: boolean;
    activate?: boolean;
  },
) {
  const reveal = options?.start
    ? { start: options.start, end: options.end, column: options.column, endColumn: options.endColumn }
    : null;
  const existing = findEditorTab(workspaceId, path);
  if (existing) {
    if (options?.pin) existing.pinned = true;
    const isActive =
      editorTabKey(existing.workspaceId, existing.path) === activeEditorKey.value;
    const shouldReadDisk =
      !props.preview &&
      (Boolean(options?.force) ||
        Boolean(options?.notifyIfDirty) ||
        !isEditorTabDirty(existing));
    if (shouldReadDisk) {
      try {
        const file = await backend.readWorkspaceFile(workspaceId, path);
        if (options?.force || !userHoldsUnsavedEdits(existing, file.content)) {
          existing.detectedLanguage = file.language;
          adoptEditorModel(workspaceId, path, file.content);
          existing.file = applyFileLanguage(file);
          existing.original = file.content;
          existing.userEdited = false;
          if (isActive) {
            activeFile.value = existing.file;
            originalContent.value = file.content;
          }
        } else if (options?.notifyIfDirty) {
          noteDiskChange(workspaceId, path, AGENT_DISK_CHANGE());
        }
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        if (options?.force && message.includes("文件不存在")) {
          const empty = {
            ...(existing.file ?? activeFile.value!),
            workspaceId,
            path,
            content: "",
            size: 0,
          };
          existing.file = empty;
          existing.original = "";
          if (isActive) {
            activeFile.value = empty;
            originalContent.value = "";
          }
        }
      }
    }
    const activated = !isActive && options?.activate !== false;
    if (activated) activateEditorTab(existing);
    else if (
      options?.revealTree ||
      diffStore.missingFiles.has(`${workspaceId}\n${path}`)
    ) {
      void revealOpenedFile();
    }
    if (reveal) {
      pendingReveal.value = reveal;
      await nextTick();
      if (pendingReveal.value) {
        editorWithDiff.value?.reveal(
          pendingReveal.value.start,
          pendingReveal.value.end,
          pendingReveal.value.column,
          pendingReveal.value.endColumn,
        );
        if (codeEditor.value) {
          revealInEditor(
            codeEditor.value,
            pendingReveal.value.start,
            pendingReveal.value.end,
            pendingReveal.value.column,
            pendingReveal.value.endColumn,
          );
        }
        pendingReveal.value = null;
      }
    }
    return;
  }
  error.value = "";
  try {
    const file = props.preview
      ? previewFileContent(path, workspaceId)
      : await backend.readWorkspaceFile(workspaceId, path);
    adoptEditorModel(workspaceId, path, file.content);
    const tab: EditorTab = {
      workspaceId,
      path,
      pinned: Boolean(options?.pin),
      file: applyFileLanguage(file),
      original: file.content,
      detectedLanguage: file.language,
    };
    const preview = editorTabs.value.find((item) => !item.pinned);
    if (preview && isEditorTabDirty(preview)) preview.pinned = true;
    const replaceable = editorTabs.value.find((item) => !item.pinned);
    if (replaceable && !options?.pin) {
      editorTabs.value = editorTabs.value.map((item) =>
        item === replaceable ? tab : item,
      );
    } else {
      editorTabs.value = [...editorTabs.value, tab];
    }
    activateEditorTab(tab);
    pendingReveal.value = reveal;
    await nextTick();
    if (pendingReveal.value) {
      editorWithDiff.value?.reveal(
        pendingReveal.value.start,
        pendingReveal.value.end,
        pendingReveal.value.column,
        pendingReveal.value.endColumn,
      );
      pendingReveal.value = null;
    }
  } catch (cause) {
    const hasSnapshot = diffStore
      .listSnapshotFiles([workspaceId])
      .some((file) => file.filePath === path);
    if (hasSnapshot) {
      diffStore.markFileMissing(workspaceId, path);
      const ghost = applyFileLanguage({
        workspaceId,
        path,
        content: "",
        language: "plaintext",
        size: 0,
        readOnly: true,
      });
      const tab: EditorTab = {
        workspaceId,
        path,
        pinned: Boolean(options?.pin),
        file: ghost,
        original: "",
        detectedLanguage: ghost.language,
      };
      const preview = editorTabs.value.find((item) => !item.pinned);
      if (preview && isEditorTabDirty(preview)) preview.pinned = true;
      const replaceable = editorTabs.value.find((item) => !item.pinned);
      if (replaceable && !options?.pin) {
        editorTabs.value = editorTabs.value.map((item) =>
          item === replaceable ? tab : item,
        );
      } else {
        editorTabs.value = [...editorTabs.value, tab];
      }
      activateEditorTab(tab);
      return;
    }
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function openFile(workspaceId: string, entry: FileEntry, pin = false) {
  if (entry.isDirectory) {
    await toggleDirectory(workspaceId, entry);
    return;
  }
  await openEditorFile(workspaceId, entry.path, { pin });
}

async function saveTab(tab: EditorTab) {
  if (tab.gitDiff) return true;
  flushActiveEditorTab();
  const isActive = tabKey(tab) === activeEditorKey.value;
  const file = isActive && activeFile.value ? activeFile.value : tab.file;
  if (file.readOnly) return true;
  saving.value = true;
  error.value = "";
  try {
    if (!props.preview) {
      await backend.writeWorkspaceFile(tab.workspaceId, tab.path, file.content);
      refreshNavigationIndex();
    }
    if (tab.original !== file.content) {
      recordLocalHistory({
        workspaceId: tab.workspaceId,
        path: tab.path,
        source: t("workbench.historySaveSource"),
        content: file.content,
      });
    }
    tab.file = file;
    tab.original = file.content;
    tab.userEdited = false;
    pinEditorTab(tab);
    if (isActive) originalContent.value = file.content;
    refreshGitViews();
    return true;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    return false;
  } finally {
    saving.value = false;
  }
}

async function saveFile() {
  const tab = activeEditorTab();
  if (!tab || !activeFile.value || activeFile.value.readOnly) return;
  if (formatOnSave.value) {
    const action = codeEditor.value?.getAction("editor.action.formatDocument");
    if (action) await action.run();
  }
  flushActiveEditorTab();
  pinEditorTab(tab);
  if (!dirty.value || saving.value) return;
  await saveTab(tab);
}

async function applyDiskToOpenTab(tab: EditorTab) {
  const isActive = editorTabKey(tab.workspaceId, tab.path) === activeEditorKey.value;
  let content = "";
  let missing = false;
  try {
    const file = await backend.readWorkspaceFile(tab.workspaceId, tab.path);
    content = file.content;
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    if (!message.includes("文件不存在")) return;
    missing = true;
  }
  if (missing) {
    // 撤回未跟踪文件后磁盘已删除；关掉标签，避免编辑器里的旧内容再被保存回去。
    editorTabs.value = editorTabs.value.filter((item) => item !== tab);
    if (isActive) {
      const next = editorTabs.value.at(-1);
      if (next) activateEditorTab(next);
      else {
        activeFile.value = null;
        originalContent.value = "";
        activeEditorKey.value = "";
      }
    }
    return;
  }
  if (tab.gitDiff) return;
  if (!userHoldsUnsavedEdits(tab, content)) {
    const next = {
      ...tab.file,
      content,
      readOnly: tab.file.readOnly,
    };
    adoptEditorModel(tab.workspaceId, tab.path, content);
    tab.file = next;
    tab.original = content;
    tab.userEdited = false;
    if (isActive) {
      activeFile.value = next;
      originalContent.value = content;
    }
    return;
  }
  if (tabBuffer(tab) !== content) noteDiskChange(tab.workspaceId, tab.path, GIT_DISK_CHANGE());
}

async function reconcileOpenEditors(workspaceId: string, paths: string[]) {
  const tabs = editorTabs.value.filter((tab) => {
    if (tab.workspaceId !== workspaceId) return false;
    if (!paths.length) return !tab.gitDiff;
    return paths.some((path) => editorPathAffected(tab.path, path));
  });
  for (const tab of tabs) await applyDiskToOpenTab(tab);
}

async function refreshActiveFile(force = false) {
  if (!activeFile.value) return;
  const tab = findEditorTab(activeFile.value.workspaceId, activeFile.value.path);
  try {
    let file = props.preview
      ? previewFileContent(activeFile.value.path, activeFile.value.workspaceId)
      : await backend.readWorkspaceFile(
          activeFile.value.workspaceId,
          activeFile.value.path,
        )        .catch((cause) => {
          const message = cause instanceof Error ? cause.message : String(cause);
          if (!message.includes("文件不存在")) throw cause;
          diffStore.markFileMissing(activeFile.value!.workspaceId, activeFile.value!.path);
          return {
            ...activeFile.value!,
            content: "",
            size: 0,
            readOnly: true,
          };
        });
    if (!force && tab && userHoldsUnsavedEdits(tab, file.content)) {
      if (tabBuffer(tab) !== file.content) {
        externalChangeMessage.value = AGENT_DISK_CHANGE();
        externalChangePending.value = true;
      }
      return;
    }
    const detectedLanguage = file.language;
    adoptEditorModel(file.workspaceId, file.path, file.content);
    file = applyFileLanguage(file);
    activeFile.value = file;
    originalContent.value = file.content;
    if (tab) {
      tab.detectedLanguage = detectedLanguage;
      tab.file = file;
      tab.original = file.content;
      tab.userEdited = false;
    }
    externalChangePending.value = false;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

function onGitStatusUpdated(repositories: GitRepositoryStatus[]) {
  gitRepositories.value = repositories;
  gitEpoch.value += 1;
}

async function loadLocalHistoryDiff(workspaceId: string, path: string, localId: string) {
  const entry = localHistoryEntry(localId);
  if (!entry) throw new Error(t("workbench.error.localHistoryMissing"));
  const current = props.preview
    ? previewFileContent(path, workspaceId)
    : await backend.readWorkspaceFile(workspaceId, path).catch(() => null);
  return {
    original: entry.content,
    modified: current?.content ?? "",
    binary: false,
    language: current?.language || "plaintext",
    label: t("workbench.compareLocalHistory"),
    short: t("workbench.localHistoryShort"),
  };
}

async function openSourceControlChange(target: ScmOpenTarget) {
  error.value = "";
  if (target.group === "merge") {
    await openEditorFile(target.workspaceId, target.path, { pin: true, force: true });
    return;
  }
  try {
    const loaded = target.group === "local" && target.localId
      ? await loadLocalHistoryDiff(target.workspaceId, target.path, target.localId)
      : await loadScmDiff(target, {
      readWorktree: async (path) => {
        if (props.preview) {
          const file = previewFileContent(path, target.workspaceId);
          return { content: file.content, language: file.language };
        }
        try {
          const file = await backend.readWorkspaceFile(target.workspaceId, path);
          return { content: file.content, language: file.language };
        } catch (cause) {
          const message = cause instanceof Error ? cause.message : String(cause);
          if (message.includes("文件不存在")) return null;
          throw cause;
        }
      },
      readIndex: async (path) => {
        if (props.preview) return { content: "", binary: false };
        const file = await backend.gitIndexFile(target.workspaceId, path);
        return { content: file.content, binary: file.binary };
      },
      readRevision: async (revision, path) => {
        if (props.preview) return { content: "", binary: false };
        const file = await backend.gitRevisionFile(target.workspaceId, revision, path);
        return { content: file.content, binary: file.binary };
      },
    });
    const id = scmDiffId(target);
    const existing = editorTabs.value.find((tab) => tab.gitDiff?.id === id);
    const file: FileContent = {
      workspaceId: target.workspaceId,
      path: target.path,
      content: loaded.modified,
      language: loaded.language,
      size: loaded.modified.length,
      readOnly: true,
    };
    if (existing?.gitDiff) {
      existing.gitDiff = { ...existing.gitDiff, ...loaded };
      existing.file = file;
      existing.original = loaded.modified;
      existing.detectedLanguage = loaded.language;
      activateEditorTab(existing);
      return;
    }
    const tab: EditorTab = {
      workspaceId: target.workspaceId,
      path: target.path,
      pinned: true,
      file,
      original: loaded.modified,
      detectedLanguage: loaded.language,
      gitDiff: { id, ...loaded },
    };
    editorTabs.value = [...editorTabs.value, tab];
    activateEditorTab(tab);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

const draggingTreeKey = ref("");

function attachWorkspacePath(item: {
  workspaceId: string;
  path: string;
  isDirectory?: boolean;
}) {
  if (props.preview) {
    chatPanel.value?.attachWorkspaceFile(item);
    return;
  }
  void store.addWorkspaceFileContext(item.workspaceId, item.path, {
    isDirectory: item.isDirectory,
  });
}

function onTreePointerDown(
  workspaceId: string,
  entry: FileEntry,
  event: PointerEvent,
) {
  // 右键会把焦点落到行上，浏览器按布局位置滚动，粘性文件夹会被甩出视野。
  if (event.button === 2) {
    event.preventDefault();
    return;
  }
  if (treeEdit.value) return;
  const item = treeItem(workspaceId, entry.path, entry.isDirectory);
  beginWorkspaceFilePointerDrag(
    event,
    { workspaceId, path: entry.path, isDirectory: entry.isDirectory },
    {
      label: explorer.isSelected(item.key) && explorer.selectedItems.value.length > 1
        ? t("workbench.selectedItems", { count: explorer.selectedItems.value.length })
        : entry.name,
      onStart: () => {
        draggingTreeKey.value = treeKey(workspaceId, entry.path);
        explorer.startDrag(item);
      },
      onMove: (moveEvent) => {
        explorer.updateDragOver(treeItemFromPoint(moveEvent.clientX, moveEvent.clientY), isCopyDrag(moveEvent, explorer.isMac));
      },
      onCancel: () => {
        draggingTreeKey.value = "";
        explorer.endDrag();
      },
      onDrop: (payload, zone, dropEvent) => {
        requestAnimationFrame(() => {
          draggingTreeKey.value = "";
        });
        void finishTreeDrag(payload, zone, dropEvent);
      },
    },
  );
}

async function finishTreeDrag(
  payload: { workspaceId: string; path: string; isDirectory?: boolean },
  zone: string | null,
  event: PointerEvent | null,
) {
  const copy = event ? isCopyDrag(event, explorer.isMac) : false;
  if (zone === "composer") {
    const sources = explorer.dragSources.value.length ? explorer.dragSources.value : [treeItem(payload.workspaceId, payload.path, Boolean(payload.isDirectory))];
    explorer.endDrag();
    for (const item of sources) attachWorkspacePath(item);
    return;
  }
  if (zone === "terminal") {
    insertPathInTerminal(payload.workspaceId, payload.path);
    explorer.endDrag();
    return;
  }
  const target = event ? treeItemFromPoint(event.clientX, event.clientY) : null;
  if (target && (zone === "explorer" || target.path !== payload.path || target.workspaceId !== payload.workspaceId)) {
    await explorer.dropOnTarget(target, copy);
    return;
  }
  if (zone === "workbench" || zone === "editor") {
    const sources = explorer.dragSources.value.length ? explorer.dragSources.value : [treeItem(payload.workspaceId, payload.path, Boolean(payload.isDirectory))];
    explorer.endDrag();
    for (const item of sources) {
      if (!item.isDirectory && item.path) void openEditorFile(item.workspaceId, item.path, { pin: true });
    }
    return;
  }
  explorer.endDrag();
  if (zone) store.reportComposerNotice(t("workbench.composerDropHint"));
}

function insertPathInTerminal(workspaceId: string, path: string) {
  const tab = terminalTabs.value.find((item) => item.id === activeTerminalId.value);
  if (!tab?.live) {
    store.reportComposerNotice(t("workbench.openTerminalFirst"));
    return;
  }
  const workspace = workspaceById(workspaceId);
  const full = workspace ? joinWorkPath(workspace.workPath || workspace.sourcePath, path) : path;
  const quoted = /\s/.test(full) ? `"${full}"` : full;
  void backend.terminalWrite(tab.id, quoted);
}

function onExplorerKeydown(event: KeyboardEvent, workspaceId?: string, entry?: FileEntry) {
  if (event.isComposing) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest(".tree-inline-input, .explorer-find-input")) return;
  const result = explorerKey(event, { isMac: isMacOS, chordK: explorerChordK });
  if (result?.action === "chord-k") {
    event.preventDefault();
    event.stopPropagation();
    explorerChordK = true;
    window.clearTimeout(explorerChordTimer);
    explorerChordTimer = window.setTimeout(() => {
      explorerChordK = false;
    }, 800);
    return;
  }
  if (explorerChordK && result) explorerChordK = false;
  if (!result) {
    if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault();
      event.stopPropagation();
      const item = explorer.typeAhead(event.key);
      if (item) void focusExplorerItem(item);
    }
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  void runExplorerKey(result, workspaceId, entry);
}

async function runExplorerKey(result: ExplorerKeyResult, workspaceId?: string, entry?: FileEntry) {
  const item = entry && workspaceId
    ? treeItem(workspaceId, entry.path, entry.isDirectory)
    : explorer.focusedItem.value;
  const row = item?.path ? explorerRowOf(item.workspaceId, item.path) : null;
  const targets = item ? explorer.actionItems(item) : explorer.actionItems();

  if (result.action === "find") {
    explorer.findOpen.value = true;
    await nextTick();
    fileTreeEl.value?.parentElement?.querySelector<HTMLInputElement>(".explorer-find-input")?.focus();
    return;
  }
  if (result.action === "undo") {
    await explorer.undo();
    return;
  }
  if (result.action === "redo") {
    await explorer.redo();
    return;
  }
  if (result.action === "select-all") {
    explorer.selectAllVisible();
    return;
  }
  if (result.action === "collapse-all") {
    collapseExplorer();
    return;
  }
  if (result.action === "paste") {
    await explorer.paste(item ?? null);
    return;
  }
  if (result.action === "cancel-cut") {
    if (explorer.cancelCut()) return;
    if (explorer.findOpen.value) {
      explorer.closeFind();
      return;
    }
    clearTreeSelection();
    return;
  }

  const move: Record<string, "next" | "previous" | "first" | "last" | "pageDown" | "pageUp"> = {
    next: "next",
    previous: "previous",
    first: "first",
    last: "last",
    "page-down": "pageDown",
    "page-up": "pageUp",
  };
  if (move[result.action]) {
    const next = explorer.moveSelection(move[result.action], Boolean(result.extend));
    if (next) void focusExplorerItem(next);
    return;
  }
  if (!item) return;

  if (result.action === "left") {
    const expanded = Boolean(row?.expanded || (item.isDirectory && expandedDirectories.value.has(item.key)));
    if (item.path && (item.isDirectory || row?.expandable) && expanded) {
      if (row) await toggleExpandable(item.workspaceId, row, false);
      else collapseWorkspace(item.workspaceId);
      return;
    }
    const parent = item.path ? treeItem(item.workspaceId, parentPathOf(item.path), true) : null;
    if (parent) {
      explorer.select(parent);
      void focusExplorerItem(parent);
    }
    return;
  }
  if (result.action === "right") {
    if ((item.isDirectory || row?.expandable) && !(row?.expanded || expandedDirectories.value.has(item.key) || (item.path === "" && expandedWorkspaces.value.has(item.workspaceId)))) {
      if (!item.path) toggleWorkspace(item.workspaceId);
      else if (row) await toggleExpandable(item.workspaceId, row, false);
      return;
    }
    const next = explorer.moveSelection("next", false);
    if (next && next.workspaceId === item.workspaceId && (next.path === item.path || next.path.startsWith(`${item.path}/`) || !item.path)) {
      void focusExplorerItem(next);
    }
    return;
  }
  if (result.action === "rename") {
    if (item.path) startRename(item.workspaceId, fileEntryFromItem(item));
    return;
  }
  if (result.action === "delete" || result.action === "delete-permanent") {
    await explorer.deleteItems(targets, result.action === "delete-permanent");
    return;
  }
  if (result.action === "copy-path") {
    await explorer.copyPaths(targets.length ? targets : [item], false);
    return;
  }
  if (result.action === "copy-relative") {
    await explorer.copyPaths(targets.length ? targets : [item], true);
    return;
  }
  if (result.action === "copy" || result.action === "cut") {
    await explorer.setClipboard(targets, result.action === "cut");
    return;
  }
  if (result.action === "reveal-os") {
    await backend.revealWorkspacePath(item.workspaceId, item.path || undefined).catch((cause) => {
      error.value = cause instanceof Error ? cause.message : String(cause);
    });
    return;
  }
  if (result.action === "open-side") {
    if (item.path && !item.isDirectory) onTreeOpenPinned(item.workspaceId, fileEntryFromItem(item));
    return;
  }
  if (result.action === "open" || result.action === "open-preserve-focus") {
    const opened = fileEntryFromItem(item);
    if (!item.path) toggleWorkspace(item.workspaceId);
    else await onTreeOpen(item.workspaceId, opened);
    if (result.action === "open-preserve-focus") await focusExplorerItem(item);
  }
}

function fileEntryFromItem(item: ExplorerItem): FileEntry {
  return explorerRowOf(item.workspaceId, item.path) ?? {
    name: item.name,
    path: item.path,
    isDirectory: item.isDirectory,
    depth: item.path.split("/").filter(Boolean).length,
  };
}

function onTreeClick(workspaceId: string, entry: ExplorerRow, event: MouseEvent) {
  if (treeEdit.value || draggingTreeKey.value) return;
  const item = treeItem(workspaceId, entry.path, entry.isDirectory);
  if (event.altKey && entry.expandable) {
    explorer.select(item);
    void toggleExpandable(workspaceId, entry, true);
    return;
  }
  const shouldOpen = explorer.clickSelect(item, event);
  if (!shouldOpen) return;
  if (entry.expandable) void toggleExpandable(workspaceId, entry, false);
  else void onTreeOpen(workspaceId, entry);
}

function onCompactClick(workspaceId: string, path: string, event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  const item = treeItem(workspaceId, path, true);
  explorer.select(item);
  void focusExplorerItem(item);
}

function onTreeKeydown(
  workspaceId: string,
  entry: FileEntry,
  event: KeyboardEvent,
) {
  onExplorerKeydown(event, workspaceId, entry);
}

function focusSelectedTreeRow() {
  const selected = selectedTree.value;
  if (!selected) return;
  const row = fileTreeEl.value?.querySelector(
    `[data-tree-key="${CSS.escape(explorerItemKey(selected.workspaceId, selected.path))}"]`,
  );
  if (row instanceof HTMLElement) row.focus({ preventScroll: true });
}

function revealCollapsedFolder(path: string) {
  void nextTick(() => {
    const scroller = fileTreeEl.value;
    const row = scroller?.querySelector(`[data-tree-path="${CSS.escape(path)}"]`);
    if (!scroller || !(row instanceof HTMLElement)) return;
    const head = scroller.querySelector(".tree-workspace-head");
    const headHeight = head?.getBoundingClientRect().height ?? 0;
    const delta = row.getBoundingClientRect().top - scroller.getBoundingClientRect().top - headHeight;
    if (delta < 0) scroller.scrollTop += delta;
  });
}

async function onTreeOpen(workspaceId: string, entry: FileEntry) {
  if (draggingTreeKey.value) return;
  selectTreeEntry(workspaceId, entry.path, entry.isDirectory);
  await openFile(workspaceId, entry);
  await nextTick();
  focusSelectedTreeRow();
}

function onTreeOpenPinned(workspaceId: string, entry: FileEntry) {
  if (entry.isDirectory) return;
  selectTreeEntry(workspaceId, entry.path, false);
  void openFile(workspaceId, entry, true);
}

function onTreeBlankClick(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  if (target?.closest(".tree-entry, .tree-workspace-head, .context-menu"))
    return;
  clearTreeSelection();
}

function workbenchDropZone(event: DragEvent) {
  const fromTarget = event.target instanceof Element
    ? event.target
    : event.target instanceof Node
      ? event.target.parentElement
      : null;
  if (fromTarget?.closest("[data-workspace-drop=composer]")) return "composer";
  const fromPoint = document.elementFromPoint(event.clientX, event.clientY);
  return (fromPoint ?? fromTarget)?.closest("[data-workspace-drop]")?.getAttribute("data-workspace-drop") ?? null;
}

function onWorkbenchDragOver(event: DragEvent) {
  const zone = workbenchDropZone(event);
  if (zone === "composer") return;
  const types = event.dataTransfer ? Array.from(event.dataTransfer.types) : [];
  const fromTree = isWorkspaceFileDrag(event.dataTransfer);
  const hasFiles = types.includes("Files");
  if (!fromTree && !hasFiles) return;
  event.preventDefault();
  event.stopPropagation();
  const copy = isCopyDrag(event, explorer.isMac);
  if (event.dataTransfer) event.dataTransfer.dropEffect = fromTree && !copy && zone === "explorer" ? "move" : "copy";
  if (zone === "explorer") {
    explorer.updateDragOver(treeItemFromPoint(event.clientX, event.clientY), copy);
  }
}

function resolveDroppedOsFile(raw: string): { workspaceId: string; path: string } | null {
  return resolvePathInWorkspaces(raw, workspaces.value);
}

async function attachDroppedFiles(event: DragEvent) {
  const fromTree = readWorkspaceFileDragData(event.dataTransfer);
  const items = collectDroppedItems(event.dataTransfer);
  if (!fromTree && !items.length) return;
  const zone = workbenchDropZone(event);
  if (zone === "composer") return;
  // 必须拦住默认行为：WebView/Monaco 会按系统编码把文件正文插进当前文档，中文会乱码。
  event.preventDefault();
  event.stopPropagation();
  const target = treeItemFromPoint(event.clientX, event.clientY);
  if (fromTree) {
    if (target) await explorer.dropOnTarget(target, isCopyDrag(event, explorer.isMac));
    else await finishTreeDrag(fromTree, zone, null);
    return;
  }
  if (zone === "explorer" && !target) {
    const folders = items.filter((item) => item.isDirectory && item.nativePath && !resolveDroppedOsFile(item.nativePath));
    if (folders.length && folders.length === items.length) {
      openAddWorkspaceModal(folders.map((item) => item.nativePath));
      return;
    }
  }
  if (zone === "terminal" && items[0]?.nativePath) {
    const resolved = resolveDroppedOsFile(items[0].nativePath);
    if (resolved) insertPathInTerminal(resolved.workspaceId, resolved.path);
    return;
  }
  await explorer.importDropped(items, target ?? (selectedTree.value
    ? treeItem(selectedTree.value.workspaceId, selectedTree.value.path, selectedTree.value.isDirectory)
    : null));
}

function addTerminalSelectionToChat(payload: { text: string; startLine: number; endLine: number }) {
  const tab = terminalTabs.value.find((item) => item.id === activeTerminalId.value);
  if (!payload.text.trim()) return;
  const context: EditorSelectionContext = {
    id: crypto.randomUUID(),
    workspaceId: tab?.workspaceId ?? workspaces.value[0]?.id ?? "",
    path: tab?.title || t("workbench.terminalTab"),
    startLine: payload.startLine,
    endLine: payload.endLine,
    text: payload.text,
    source: "terminal",
  };
  if (props.preview) {
    chatPanel.value?.insertChip(context);
    return;
  }
  chatPanel.value?.insertChip(store.addComposerContext(context));
}

function addSelectionToChat(
  instance: Monaco.editor.IStandaloneCodeEditor,
  file: { workspaceId: string; path: string },
) {
  const model = instance.getModel();
  const selection = instance.getSelection();
  if (!model || !selection) return;
  const range = selection.isEmpty()
    ? new monaco.Range(
        selection.startLineNumber,
        1,
        selection.startLineNumber,
        model.getLineMaxColumn(selection.startLineNumber),
      )
    : selection;
  const text = model.getValueInRange(range);
  if (!text.trim()) {
    store.reportComposerNotice(t("workbench.emptyLineReference"));
    return;
  }
  const context = {
    id: crypto.randomUUID(),
    workspaceId: file.workspaceId,
    path: file.path,
    startLine: range.startLineNumber,
    endLine: range.endLineNumber,
    text,
  };
  if (props.preview) {
    chatPanel.value?.insertChip(context);
    return;
  }
  const stored = store.addComposerContext(context);
  chatPanel.value?.insertChip(stored);
}

function isAddToChatShortcut(event: KeyboardEvent) {
  if (event.repeat || event.isComposing || event.altKey || event.shiftKey) return false;
  if (!(event.metaKey || event.ctrlKey)) return false;
  return event.code === "KeyI" || event.key === "i" || event.key === "I";
}

function handleAddToChatKey(event: KeyboardEvent) {
  if (!isAddToChatShortcut(event)) return;
  const editor = codeEditor.value;
  const file = activeFile.value;
  if (!editor || !file || activeGitDiff.value) return;
  const host = editor.getDomNode();
  const target = event.target;
  const inEditor = Boolean(host && target instanceof Node && host.contains(target));
  if (!inEditor && !editor.hasTextFocus()) return;
  event.preventDefault();
  event.stopPropagation();
  addSelectionToChat(editor, file);
}

function onEditorContentChange(content: string, fromUser = false) {
  const file = activeFile.value;
  if (!file) return;
  if (file.content !== content) file.content = content;
  const tab = findEditorTab(file.workspaceId, file.path);
  if (tab && fromUser) {
    tab.userEdited = content !== tabSavedContent(tab);
  } else if (tab && !tab.userEdited) {
    tab.original = content;
    if (tabKey(tab) === activeEditorKey.value) originalContent.value = content;
  }
  syncEditorDocument();
}

function onEditorMount(editor: Monaco.editor.IStandaloneCodeEditor) {
  codeEditor.value = markRaw(editor);
  bindEditorZoom(editor, monaco);
  editor.onDidChangeCursorPosition(() => syncEditorDocument());
  editor.addAction({
    id: "multara-save",
    label: t("workbench.shortcut.save"),
    keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
    run: saveFile,
  });
  editor.addAction({
    id: "multara-add-to-chat",
    label: t("workbench.shortcut.attach"),
    keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyI],
    run: () => {
      if (activeFile.value) addSelectionToChat(editor, activeFile.value);
    },
  });
  layoutEditor();
  if (pendingReveal.value) {
    editorWithDiff.value?.reveal(
      pendingReveal.value.start,
      pendingReveal.value.end,
      pendingReveal.value.column,
      pendingReveal.value.endColumn,
    );
    pendingReveal.value = null;
  }
}

function isAgentWriteResult(event: {
  sessionId: string;
  status: string;
  timelineItem: ChatMessage["timeline"][number] | null;
}) {
  const item = event.timelineItem;
  if (event.status !== "running" || item?.eventType !== "tool_result") return false;
  if (item.tool === "Edit" || item.tool === "Write" || item.tool === "Create") return true;
  const kind = itemKind(item);
  return kind === "edit" || kind === "write";
}

function resolveAgentWritePath(event: { sessionId: string; timelineItem: ChatMessage["timeline"][number] | null }) {
  const item = event.timelineItem;
  if (!item) return "";
  const direct = toolPath(item);
  if (direct) return direct;
  const sessionMessages = store.messages[event.sessionId] ?? [];
  for (const message of sessionMessages) {
    const use = message.timeline.find(
      (entry) =>
        entry.eventType === "tool_use" &&
        entry.callId &&
        entry.callId === item.callId,
    );
    if (use) {
      const path = toolPath(use);
      if (path) return path;
    }
  }
  return "";
}

function workspacePathRoots(): WorkspacePathRoot[] {
  return workspaces.value.map((workspace) => ({
    id: workspace.id,
    name: workspace.name,
    roots: [workspace.workPath, workspace.sourcePath].filter(
      (root): root is string => Boolean(root),
    ),
  }));
}

function resolveToolPath(raw: string): LocatedFile | null {
  const path = normalizeFsPath(raw);
  if (!path) return null;
  if (isAbsoluteFsPath(path)) {
    return matchLongestWorkspaceRoot(path, workspacePathRoots());
  }
  const relative = relativeToolPath(path);
  if (!relative || workspaces.value.length !== 1) return null;
  return { workspaceId: workspaces.value[0].id, path: relative };
}

function revealInEditor(
  editor: Monaco.editor.IStandaloneCodeEditor,
  start: number,
  end?: number,
  column?: number,
  endColumn?: number,
) {
  const model = editor.getModel();
  if (!model) return;
  const startLine = Math.max(1, Math.min(start, model.getLineCount()));
  const endLine = Math.max(
    startLine,
    Math.min(end ?? start, model.getLineCount()),
  );
  const range = column
    ? new monaco.Range(startLine, column, endLine, endColumn ?? column)
    : new monaco.Range(startLine, 1, endLine, model.getLineMaxColumn(endLine));
  editor.revealRangeInCenter(range);
  editor.setSelection(range);
  editor.focus();
}

async function openFileAt(
  workspaceId: string,
  path: string,
  start?: number,
  end?: number,
) {
  await openEditorFile(workspaceId, path, { start, end });
}

async function fileExists(workspaceId: string, path: string) {
  try {
    await backend.readWorkspaceFile(workspaceId, path);
    return true;
  } catch {
    return false;
  }
}

async function locateWorkspaceFiles(raw: string): Promise<LocatedFile[]> {
  const path = normalizeFsPath(raw);
  if (!path) return [];
  if (isAbsoluteFsPath(path)) {
    const match = matchLongestWorkspaceRoot(path, workspacePathRoots());
    return match ? [match] : [];
  }
  const relative = relativeToolPath(path);
  if (!relative) return [];
  const exact: LocatedFile[] = [];
  for (const workspace of workspaces.value) {
    if (await fileExists(workspace.id, relative)) {
      exact.push({ workspaceId: workspace.id, path: relative });
    }
  }
  if (exact.length) return exact;
  const found: LocatedFile[] = [];
  for (const workspace of workspaces.value) {
    let entries: FileEntry[] = [];
    try {
      entries = await backend.listQuickOpenFiles(workspace.id);
    } catch {
      continue;
    }
    for (const entry of entries) {
      const entryPath = entry.path.replaceAll("\\", "/");
      if (entry.isDirectory || !filePathMatchesReference(entryPath, relative)) continue;
      found.push({ workspaceId: workspace.id, path: entryPath });
    }
  }
  return found;
}

function choosePickedFile(choice: LocatedFile) {
  const pick = filePick.value;
  filePick.value = null;
  if (!pick) return;
  void openFileAt(choice.workspaceId, choice.path, pick.start, pick.end);
}

watch(filePick, (pick) => {
  if (!pick) return;
  void nextTick(() => filePickDialog.value?.focus());
});

watch(
  () => store.quickOpenRequest?.nonce,
  () => {
    const request = store.quickOpenRequest;
    if (!request) return;
    void openQuickOpen(request.query);
  },
);

watch(
  () => store.fileReveal?.nonce,
  async () => {
    const request = store.fileReveal;
    if (!request) return;
    const nonce = request.nonce;
    const matches = request.workspaceId
      ? [{ workspaceId: request.workspaceId, path: request.path }]
      : await locateWorkspaceFiles(request.path);
    if (store.fileReveal?.nonce !== nonce) return;
    if (!matches.length || matches.some((match) => !match.path)) {
      const shown = pathWithoutWorktreeDir(request.path);
      store.reportComposerNotice(t("workbench.fileNotFound", { path: shown }), shown);
      return;
    }
    if (matches.length === 1) {
      await openFileAt(
        matches[0].workspaceId,
        matches[0].path,
        request.startLine,
        request.endLine,
      );
      return;
    }
    const multiple = workspaces.value.length > 1;
    filePick.value = {
      start: request.startLine,
      end: request.endLine,
      choices: matches.map((match) => {
        const workspace = workspaces.value.find((item) => item.id === match.workspaceId);
        return {
          ...match,
          label: workspaceFileLabel(
            workspace?.name || match.workspaceId,
            match.path,
            multiple,
          ),
        };
      }),
    };
  },
);

function dismissEditorError() {
  error.value = "";
}

function layoutEditor() {
  editorWithDiff.value?.layout(true);
}

let stopThemeWatch: (() => void) | undefined;
const navigationOwner = {};
let stopNavigation: (() => void) | undefined;

function startCodeNavigation() {
  if (props.preview) return;
  installCodeNavigation();
  const opener = registerWorkspaceOpener({
    owns: (editor) => editor === codeEditor.value,
    open: (target) =>
      openEditorFile(target.workspaceId, target.path, {
        start: target.startLine,
        end: target.endLine,
        column: target.startColumn,
        endColumn: target.endColumn,
      }),
  });
  const stopOpenChecker = codeIndex.registerOpenChecker((workspaceId, path) =>
    Boolean(findEditorTab(workspaceId, path)),
  );
  stopNavigation = () => {
    opener.dispose();
    stopOpenChecker();
    releaseNavigationWorkspaces(navigationOwner);
  };
}

onMounted(() => {
  startCodeNavigation();
  editorStatus.setLanguages(listSelectableLanguages(monaco.languages.getLanguages()));
  stopThemeWatch = watchDocumentTheme((theme) => {
    editorTheme.value = monacoThemeName(theme);
  });
  registerUnsavedLeaveHandler(confirmLeaveDirtyEditors);
  registerUnsavedDirtyChecker(() => dirtyEditorTabs().length > 0);
  registerDirtyEditorPaths(() => dirtyEditorTabs().map((tab) => tab.path));
  window.addEventListener("resize", layoutEditor);
  window.addEventListener("keydown", handleEditorTabKeys);
  window.addEventListener("keydown", handleWorkbenchShortcuts, true);
  window.addEventListener("keydown", handleAddToChatKey, true);
  resizePanel("left", 0);
  resizePanel("right", 0);
  void nextTick(() => {
    editorTabsEl.value?.addEventListener("wheel", onEditorTabsWheel, {
      passive: false,
    });
    bindSidebarTabs();
    bindTreeStickyHead();
  });
  void backend.terminalProfiles().then((list) => {
    terminalProfiles.value = list;
    if (!defaultTerminalProfile.value) {
      defaultTerminalProfile.value = list.find((item) => item.isDefault)?.id ?? "";
    }
  }).catch(() => undefined);
});

onBeforeUnmount(() => {
  gitChangeRefreshScheduler.dispose();
  window.removeEventListener("pointerdown", onTerminalPickOutside, true);
  registerUnsavedLeaveHandler(null);
  registerUnsavedDirtyChecker(null);
  registerDirtyEditorPaths(null);
  window.clearTimeout(tabChordTimer);
  stopResizing?.();
  editorTabsEl.value?.removeEventListener("wheel", onEditorTabsWheel);
  sidebarTabsEl.value?.removeEventListener("wheel", onSidebarTabsWheel);
  window.removeEventListener("resize", layoutEditor);
  window.removeEventListener("keydown", handleEditorTabKeys);
  window.removeEventListener("keydown", handleWorkbenchShortcuts, true);
  window.removeEventListener("keydown", handleAddToChatKey, true);
  sidebarTabsObserver?.disconnect();
  sidebarTabsObserver = null;
  stickyTreeObserver?.disconnect();
  stickyTreeObserver = null;
  stopThemeWatch?.();
  stopNavigation?.();
  codeEditor.value = null;
  if (ownsEditorStatus()) editorStatus.syncActive(null, null);
});

watch(
  () =>
    [
      diffStore.diskEpoch,
      store.activeSessionId,
      workspaces.value.map((workspace) => workspace.id).join("\n"),
    ] as const,
  () => {
    void refreshPendingFileContents();
  },
  { immediate: true },
);

watch(
  () => diffStore.diskEpoch,
  async (current, previous) => {
    if (props.preview || !current || current === previous) return;
    refreshNavigationIndex();
    await Promise.all([
      loadWorkspaceRoots({ silent: true, preserve: true }),
      refreshGitStatuses(),
      sourceControlPanel.value?.refresh(),
    ]);
    await refreshActiveFile();
  },
);

watch(
  () =>
    [
      route.path,
      activeFile.value?.workspaceId,
      activeFile.value?.path,
      activeFile.value?.language,
    ] as const,
  () => {
    if (!ownsEditorStatus()) return;
    const file = activeFile.value;
    editorStatus.syncActive(
      file ? editorLanguageKey(file.workspaceId, file.path) : null,
      file?.language ?? null,
    );
  },
  { immediate: true },
);

watch(
  () => editorStatus.selection,
  (sel) => {
    if (!ownsEditorStatus()) return;
    if (!sel || !activeFile.value) return;
    if (editorLanguageKey(activeFile.value.workspaceId, activeFile.value.path) !== sel.key) {
      return;
    }
    const tab = activeEditorTab();
    const language =
      sel.language === AUTO_DETECT_LANGUAGE
        ? (tab?.detectedLanguage ?? activeFile.value.language)
        : sel.language;
    setActiveLanguage(language);
    editorStatus.syncActive(sel.key, language);
  },
);

watch(
  () =>
    [
      activeFile.value?.workspaceId,
      activeFile.value?.path,
      activeFile.value?.content,
    ] as const,
  () => {
    const file = activeFile.value;
    const tab = activeEditorTab();
    if (!file || !tab || tab.gitDiff) return;
    if (tab.workspaceId !== file.workspaceId || tab.path !== file.path) return;
    tab.file = file;
  },
);

async function persistDirtyEditors() {
  if (props.preview) return;
  for (const tab of dirtyEditorTabs()) {
    if (!(await saveTab(tab))) return;
  }
}

let revertSidebarTab = false;
watch(leftTab, async (next, previous) => {
  await nextTick();
  scrollActiveSidebarTabIntoView();
  updateSidebarTabOverflow();
  if (next === "files") {
    fileTreeEl.value
      ?.querySelector(".tree-entry.selected")
      ?.scrollIntoView({ block: "center" });
  }
  if (revertSidebarTab) {
    revertSidebarTab = false;
    return;
  }
  if (next !== "git") return;
  const dirtyTabs = props.preview ? [] : dirtyEditorTabs();
  if (dirtyTabs.length) {
    const prompt = dirtyTabs.length === 1
      ? t("workbench.openGitSaveOne", { name: fileNameOf(dirtyTabs[0].path) })
      : t("workbench.openGitSaveMany", { count: dirtyTabs.length });
    if (!window.confirm(prompt)) {
      revertSidebarTab = true;
      leftTab.value = previous && previous !== "git" ? previous : "files";
      return;
    }
  }
  await persistDirtyEditors();
  await refreshGitStatuses();
  await sourceControlPanel.value?.refresh();
});

watch(dirty, (isDirty) => {
  if (ignoreDirtyPin.value || !isDirty || !activeFile.value) return;
  const tab = findEditorTab(
    activeFile.value.workspaceId,
    activeFile.value.path,
  );
  if (tab) tab.pinned = true;
});

watch(projectId, (id, previous) => {
  if (!previous || previous === id) return;
  clearEditorTabs();
  externalChangePending.value = false;
});

watch(
  () => workspaces.value.map((workspace) => workspace.id).join("\0"),
  (ids, previous) => {
    if (ids !== previous) {
      expandedWorkspaces.value = new Set(workspaces.value.map((item) => item.id));
      if (!props.preview) setNavigationWorkspaces(navigationOwner, workspacePathRoots());
      void loadWorkspaceRoots();
      void refreshGitStatuses();
      for (const workspace of workspaces.value) {
        void loadWorkspaceEditorSettings(workspace.id);
      }
    }
  },
  { immediate: true },
);

watch(
  () => [activeFile.value?.content, activeFile.value?.language, activeFile.value?.path] as const,
  () => syncEditorDocument(),
);

watch(
  () => editorStatus.documentAction,
  (action) => {
    if (!ownsEditorStatus() || !action || !activeFile.value) return;
    const key = editorLanguageKey(activeFile.value.workspaceId, activeFile.value.path);
    if (action.type === "indent" && action.tabSize && action.insertSpaces != null) {
      indentOverrides.value = {
        ...indentOverrides.value,
        [key]: { insertSpaces: action.insertSpaces, tabSize: action.tabSize },
      };
      codeEditor.value?.updateOptions({
        insertSpaces: action.insertSpaces,
        tabSize: action.tabSize,
      });
    }
    if (action.type === "eol" && action.eol) {
      eolOverrides.value = { ...eolOverrides.value, [key]: action.eol };
      const next = convertEol(activeFile.value.content, action.eol);
      if (next !== activeFile.value.content) {
        activeFile.value = { ...activeFile.value, content: next };
        codeEditor.value?.setValue(next);
      }
    }
    if (action.type === "goto") {
      void codeEditor.value?.getAction("editor.action.gotoLine")?.run();
    }
    if (action.type === "reveal" && action.line) {
      editorWithDiff.value?.reveal(action.line);
    }
    syncEditorDocument();
  },
);

watch(
  () =>
    Object.values(store.runStates)
      .filter((event) => {
        const session = store.data?.sessions.find(
          (item) => item.id === event.sessionId,
        );
        return (
          session?.projectId === projectId.value &&
          event.status === "running" &&
          isAgentWriteResult(event)
        );
      })
      .map((event) => `${event.runId}:${event.timelineItem?.seq ?? 0}`)
      .sort()
      .join("|"),
  async (current, previous) => {
    if (props.preview || !current || current === previous) return;
    refreshNavigationIndex();
    await Promise.all([
      loadWorkspaceRoots({ silent: true, preserve: true }),
      refreshGitStatuses(),
      sourceControlPanel.value?.refresh(),
    ]);
    const events = Object.values(store.runStates).filter((event) => {
      const session = store.data?.sessions.find(
        (item) => item.id === event.sessionId,
      );
      return session?.projectId === projectId.value && isAgentWriteResult(event);
    });
    const groups: LocatedFile[][] = [];
    for (const event of events) {
      const path = resolveAgentWritePath(event);
      groups.push(path ? await locateWorkspaceFiles(path) : []);
    }
    const written = groups.flat();
    if (!written.length) {
      await refreshActiveFile();
      return;
    }
    for (const file of written) {
      if (findEditorTab(file.workspaceId, file.path)) {
        await openEditorFile(file.workspaceId, file.path, {
          notifyIfDirty: true,
          activate: false,
        });
      }
    }
    diffStore.notifyDiskChanged();
  },
);

watch(
  () =>
    Object.values(store.runStates)
      .filter((event) => {
        const session = store.data?.sessions.find(
          (item) => item.id === event.sessionId,
        );
        return (
          session?.projectId === projectId.value &&
          ["completed", "failed", "stopped"].includes(event.status)
        );
      })
      .map((event) => `${event.runId}:${event.status}`)
      .sort()
      .join("|"),
  async (current, previous) => {
    if (props.preview || !current || current === previous) return;
    refreshNavigationIndex();
    await Promise.all([
      loadWorkspaceRoots({ silent: true, preserve: true }),
      refreshGitStatuses(),
      sourceControlPanel.value?.refresh(),
    ]);
    await refreshActiveFile();
  },
);

defineExpose({
  insertChip: (item: EditorSelectionContext) =>
    chatPanel.value?.insertChip(item),
  insertText: (value: string) => chatPanel.value?.insertText(value),
  addPastedImage: () => chatPanel.value?.addPastedImage(),
  addAttachment: (name: string, mimeType: string) =>
    chatPanel.value?.addAttachment(name, mimeType),
  attachWorkspaceFile: (file: {
    workspaceId: string;
    path: string;
    isDirectory?: boolean;
  }) => chatPanel.value?.attachWorkspaceFile(file),
  clearComposer: () => chatPanel.value?.clearComposer(),
  focusComposer: () => chatPanel.value?.focus(),
});
</script>

<template>
  <div
    ref="workbenchEl"
    class="workbench"
    data-workspace-drop="workbench"
    :style="workbenchStyle"
    @dragover.capture="onWorkbenchDragOver"
    @drop.capture="attachDroppedFiles"
  >
    <aside class="workspace-sidebar">
      <div
        class="workspace-sidebar-tabs"
        :class="{ 'is-overflowing': sidebarTabsOverflow }"
      >
        <UiButton
          v-if="sidebarTabsOverflow"
          variant="ghost"
          class="sidebar-tab-arrow"
          :title="t('workbench.sidebarPrev')"
          :disabled="leftTab === 'files'"
          @click="shiftSidebarTab(-1)"
        >
          <ChevronLeft :size="12" />
        </UiButton>
        <div ref="sidebarTabsEl" class="workspace-sidebar-tablist">
          <UiButton
            :class="{ active: leftTab === 'files' }"
            :title="t('workbench.files')"
            :aria-pressed="leftTab === 'files'"
            @click="leftTab = 'files'"
          >
            <Codicon name="files" /><span class="sidebar-tab-label">{{ t('workbench.files') }}</span>
          </UiButton>
          <UiButton
            :class="{ active: leftTab === 'git' }"
            :title="t('workbench.git')"
            :aria-label="gitChangeCount ? t('workbench.gitChangesAria', { count: gitChangeCount }) : t('workbench.git')"
            :aria-pressed="leftTab === 'git'"
            @click="leftTab = 'git'"
          >
            <Codicon name="source-control" /><span class="sidebar-tab-label">{{ t('workbench.git') }}</span>
            <span v-if="gitChangeCount" class="sidebar-tab-badge">{{ gitChangeCount > 99 ? "99+" : gitChangeCount }}</span>
          </UiButton>
          <UiButton
            :class="{ active: leftTab === 'search' }"
            :title="t('workbench.search')"
            :aria-pressed="leftTab === 'search'"
            @click="leftTab = 'search'"
          >
            <Codicon name="search" /><span class="sidebar-tab-label">{{ t('workbench.search') }}</span>
          </UiButton>
        </div>
        <UiButton
          v-if="sidebarTabsOverflow"
          variant="ghost"
          class="sidebar-tab-arrow"
          :title="t('workbench.sidebarNext')"
          :disabled="leftTab === 'search'"
          @click="shiftSidebarTab(1)"
        >
          <ChevronRight :size="12" />
        </UiButton>
      </div>
      <div class="workspace-sidebar-body">
      <SourceControlPanel
        v-show="leftTab === 'git'"
        ref="sourceControlPanel"
        :project-id="preview ? PREVIEW_PROJECT_ID : projectId"
        :workspaces="workspaces"
        @open-change="openSourceControlChange"
        @open-file="(target) => openEditorFile(target.workspaceId, target.path, { pin: true })"
        @updated="onGitStatusUpdated"
        @worktree-changed="reconcileOpenEditors"
      />
      <SearchView
        ref="searchView"
        v-show="leftTab === 'search'"
        :workspaces="workspaces"
        :preview="preview"
        :active="leftTab === 'search'"
        :apply-replace="applySearchReplace"
        @open="(workspaceId, path, line) => openFileAt(workspaceId, path, line, line)"
      />
      <template v-if="leftTab === 'files'">
      <div v-if="loading && !hasExplorerTree()" class="tree-empty">
        {{ t("workbench.loadingTree") }}
      </div>
      <div v-else-if="!workspaces.length" class="tree-empty">
        {{ t("workbench.noWorkspaces") }}
      </div>
      <UiPaneView v-else class="explorer-pane">
        <UiPane
          v-if="showOpenEditors"
          :title="t('workbench.openEditors')"
          storage-key="multara.explorer.openEditors"
          size-key="multara.explorer.openEditorsSize"
          :default-expanded="false"
          @contextmenu="openExplorerViewMenu"
        >
          <template #actions>
            <UiButton
              variant="ghost"
              size="icon"
              :title="t('workbench.saveAll')"
              :disabled="!dirtyEditorTabs().length"
              @click="saveAllOpenEditors"
            >
            <Codicon name="save-all" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon"
              :title="t('workbench.closeAll')"
              :disabled="!editorTabs.length"
              @click="closeAllEditorTabs"
            >
            <Codicon name="close-all" />
            </UiButton>
          </template>
          <p v-if="!editorTabs.length" class="ui-pane-empty">{{ t('workbench.noOpenEditors') }}</p>
          <div
            v-for="tab in editorTabs"
            :key="tabKey(tab)"
            class="ui-pane-item"
            role="button"
            tabindex="0"
            :class="{ 'is-active': activeEditorKey === tabKey(tab), 'is-preview': !tab.pinned }"
            :title="editorTabTitle(tab)"
            @click="activateEditorTab(tab)"
            @keydown.enter.prevent="activateEditorTab(tab)"
            @auxclick="onOpenEditorAux($event, tab)"
            @contextmenu="openOpenEditorsMenu(tab, $event)"
          >
            <FileTypeIcon :name="fileNameOf(tab.path)" />
            <span :class="{ 'is-preview-name': !tab.pinned }">{{ fileNameOf(tab.path) }}</span>
            <i v-if="isEditorTabDirty(tab)" class="ui-pane-dirty" />
            <UiButton
              class="ui-pane-close"
              variant="ghost"
              size="icon"
              :title="t('common.close')"
              @click="closeEditorTab(tab, $event)"
            >
              <Codicon name="close" />
            </UiButton>
          </div>
        </UiPane>
        <UiPane :title="t('workbench.folders')" fill actions-visible storage-key="multara.explorer.folders" :min-body="120" @contextmenu="openExplorerViewMenu">
          <template #actions>
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('workbench.newFile')"
            @click="startCreateInSelection(false)"
          >
            <Codicon name="new-file" />
          </UiButton>
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('workbench.newFolder')"
            @click="startCreateInSelection(true)"
          >
            <Codicon name="new-folder" />
          </UiButton>
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('workbench.refresh')"
            @click="refreshExplorer"
          >
            <Codicon name="refresh" />
          </UiButton>
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('workbench.collapseAll')"
            @click="collapseExplorer"
          >
            <Codicon name="collapse-all" />
          </UiButton>
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('workbench.revealActiveFile')"
            :disabled="!openedFileTarget()"
            @click="revealActiveExplorerFile"
          >
            <Codicon name="target" />
          </UiButton>
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('workbench.findWithShortcut', { keys: explorerKeys.find })"
            @click="explorerFindOpen = true"
          >
            <Codicon name="search" />
          </UiButton>
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('workbench.view')"
            @click="openExplorerViewMenu"
          >
            <Codicon name="more" />
          </UiButton>
          </template>
        <div class="explorer-folders">
        <div v-if="explorerFindOpen" class="explorer-find">
          <input
            v-model="explorerFindQuery"
            class="explorer-find-input"
            type="search"
            :placeholder="t('workbench.findPlaceholder')"
            spellcheck="false"
            @keydown.escape.stop.prevent="explorer.closeFind()"
          />
          <button
            type="button"
            class="explorer-find-mode"
            :title="explorerFindMode === 'filter' ? t('workbench.filterMode') : t('workbench.highlightMode')"
            @click="explorerFindMode = explorerFindMode === 'filter' ? 'highlight' : 'filter'"
          >
            <Codicon :name="explorerFindMode === 'filter' ? 'filter' : 'search'" />
          </button>
          <button type="button" class="explorer-find-mode" :title="t('common.close')" @click="explorer.closeFind()">
            <Codicon name="close" />
          </button>
        </div>
        <div
          ref="fileTreeEl"
          class="file-tree vs-scroll"
          data-workspace-drop="explorer"
          @click="onTreeBlankClick"
          @dragstart.prevent
        >
          <section
            v-for="workspace in workspaces"
            :key="workspace.id"
            class="tree-workspace"
          >
            <div
              class="tree-workspace-head"
              :class="[
                `is-${workspaceMode(workspace)}`,
                {
                  selected: isWorkspaceTreeSelected(workspace.id),
                  focused: explorer.isFocused(explorerItemKey(workspace.id, '')),
                  'is-cut': explorerCutKeys.has(explorerItemKey(workspace.id, '')),
                  'is-drop': explorerDragOver?.key === explorerItemKey(workspace.id, '') && explorerDragOver.valid,
                  'is-drop-invalid': explorerDragOver?.key === explorerItemKey(workspace.id, '') && !explorerDragOver.valid,
                },
              ]"
              role="button"
              tabindex="0"
              :data-tree-key="explorerItemKey(workspace.id, '')"
              :title="workspaceModeHint(workspace)"
              @click="explorer.clickSelect(treeItem(workspace.id, '', true), $event) && toggleWorkspace(workspace.id)"
              @keydown="onExplorerKeydown($event, workspace.id)"
              @contextmenu="openTreeMenu(workspace.id, null, $event)"
            >
              <Codicon
                name="chevron-down"
                class="tree-chevron"
                :class="{ 'is-open': expandedWorkspaces.has(workspace.id) }"
                :size="10"
              />
              <FileTypeIcon
                :name="workspace.kind === 'git' ? '.gitignore' : workspace.name"
                :directory="workspace.kind !== 'git'"
                :open="expandedWorkspaces.has(workspace.id)"
              />
              <div class="tree-workspace-copy">
                <strong>{{ workspace.name }}</strong>
                <small>{{
                  workspaceSubtitle(
                    workspace,
                    gitRepositoryByWorkspace.get(workspace.id)?.branch,
                  )
                }}</small>
              </div>
              <span
                class="workspace-mode-pill"
                :class="`pill-${workspaceMode(workspace)}`"
              >
                {{ workspaceModeLabel(workspace) }}
              </span>
            </div>
            <template v-if="expandedWorkspaces.has(workspace.id)">
              <div
                v-if="
                  treeEdit?.workspaceId === workspace.id &&
                  !treeEdit.path &&
                  !treeEdit.parentPath
                "
                class="tree-entry tree-edit"
                :style="{ paddingLeft: `${treeRowPad(0)}px` }"
              >
                <span class="tree-chevron" />
                <FileTypeIcon
                  :name="treeEdit.name || 'untitled'"
                  :directory="treeEdit.mode === 'folder'"
                />
                <input
                  :ref="setTreeEditInput"
                  v-model="treeEdit.name"
                  class="tree-inline-input"
                  spellcheck="false"
                  @pointerdown.stop
                  @keydown.stop="onTreeEditKeydown"
                  @blur="commitTreeEdit"
                />
                <span
                  v-if="treeEditProblem"
                  class="tree-edit-error"
                  :class="`is-${treeEditProblem.severity}`"
                >{{ treeEditProblem.message }}</span>
              </div>
              <ExplorerTreeList :nodes="explorerNodes(workspace.id)">
                <template #row="{ entry, sticky }">
                <div
                  class="tree-entry"
                  role="button"
                  tabindex="0"
                  :draggable="false"
                  :class="[
                    treeGitClass(workspace.id, entry),
                    {
                      'is-sticky': sticky,
                      selected: isTreeSelected(workspace.id, entry.path),
                      focused: explorer.isFocused(treeKey(workspace.id, entry.path)),
                      loading: loadingDirectories.has(treeKey(workspace.id, entry.path)),
                      dragging: draggingTreeKey === treeKey(workspace.id, entry.path),
                      'is-cut': explorerCutKeys.has(treeKey(workspace.id, entry.path)),
                      'is-ignored': explorer.isIgnored(workspace.id, entry.path),
                      'is-excluded': entry.excluded,
                      'is-nested': entry.nestedChild,
                      'is-drop': explorerDragOver?.key === treeKey(workspace.id, entry.path) && explorerDragOver.valid,
                      'is-drop-invalid': explorerDragOver?.key === treeKey(workspace.id, entry.path) && !explorerDragOver.valid,
                    },
                  ]"
                  :data-tree-path="entry.path"
                  :data-tree-key="treeKey(workspace.id, entry.path)"
                  :style="{
                    paddingLeft: `${treeRowPad(entry.depth)}px`,
                    '--tree-depth': entry.depth,
                  }"
                  :title="explorerHoverTitle(entry)"
                  @click="onTreeClick(workspace.id, entry, $event)"
                  @dblclick="onTreeOpenPinned(workspace.id, entry)"
                  @keydown="onTreeKeydown(workspace.id, entry, $event)"
                  @pointerdown="onTreePointerDown(workspace.id, entry, $event)"
                  @contextmenu="openTreeMenu(workspace.id, entry, $event)"
                >
                  <span
                    v-for="level in entry.depth"
                    :key="`guide:${level}`"
                    class="tree-indent-guide"
                    :class="{ innermost: level === entry.depth }"
                    :style="{ left: `${treeGuideLeft(level)}px` }"
                  />
                  <Codicon
                    v-if="entry.expandable"
                    name="chevron-down"
                    class="tree-chevron"
                    :class="{ 'is-open': entry.expanded }"
                    :size="10"
                  />
                  <span v-else class="tree-chevron" />
                  <FileTypeIcon
                    :name="entry.compact ? entry.compact.names.at(-1)! : entry.name"
                    :directory="entry.isDirectory"
                    :open="entry.expanded"
                  />
                  <input
                    v-if="treeEdit?.path === entry.path"
                    :ref="setTreeEditInput"
                    v-model="treeEdit.name"
                    class="tree-inline-input"
                    spellcheck="false"
                    @click.stop
                    @pointerdown.stop
                    @keydown.stop="onTreeEditKeydown"
                    @blur="commitTreeEdit"
                  />
                  <span v-else-if="entry.compact" class="tree-name tree-compact">
                    <template v-for="(name, index) in entry.compact.names" :key="entry.compact.paths[index]">
                      <span v-if="index" class="tree-compact-sep">/</span>
                      <span class="tree-compact-seg" @click="onCompactClick(workspace.id, entry.compact!.paths[index], $event)">{{ name }}</span>
                    </template>
                  </span>
                  <span v-else class="tree-name">
                    <span
                      v-for="(part, index) in treeNameParts(entry)"
                      :key="index"
                      :class="{ 'is-match': part.hit }"
                    >{{ part.text }}</span>
                  </span>
                  <span
                    v-if="treeEdit?.path === entry.path && treeEditProblem"
                    class="tree-edit-error"
                    :class="`is-${treeEditProblem.severity}`"
                  >{{ treeEditProblem.message }}</span>
                  <span
                    v-if="entry.symlinkTarget"
                    class="tree-symlink"
                    :title="`${entry.symlinkTarget} · ${t('workbench.symlinkSuffix')}`"
                  >
                    <ArrowUpRight :size="12" />
                  </span>
                  <span
                    v-if="treeGitStatus(workspace.id, entry)"
                    class="tree-git-badge"
                    :class="{ 'is-folder': entry.isDirectory }"
                    :title="treeGitBadgeTitle(workspace.id, entry)"
                    >{{ entry.isDirectory ? "" : treeGitStatus(workspace.id, entry) }}</span
                  >
                </div>
                </template>
                <template #edit="{ entry }">
                <div
                  v-if="
                    treeEdit?.workspaceId === workspace.id &&
                    !treeEdit.path &&
                    treeEdit.parentPath === entry.path
                  "
                  class="tree-entry tree-edit"
                  :style="{
                    paddingLeft: `${treeRowPad(entry.depth + 1)}px`,
                    '--tree-depth': entry.depth + 1,
                  }"
                >
                  <span
                    v-for="level in entry.depth + 1"
                    :key="`edit-guide:${level}`"
                    class="tree-indent-guide"
                    :class="{ innermost: level === entry.depth + 1 }"
                    :style="{ left: `${treeGuideLeft(level)}px` }"
                  />
                  <span class="tree-chevron" />
                  <FileTypeIcon
                    :name="treeEdit.name || 'untitled'"
                    :directory="treeEdit.mode === 'folder'"
                  />
                  <input
                    :ref="setTreeEditInput"
                    v-model="treeEdit.name"
                    class="tree-inline-input"
                    spellcheck="false"
                    @pointerdown.stop
                    @keydown.stop="onTreeEditKeydown"
                    @blur="commitTreeEdit"
                  />
                  <span
                    v-if="treeEditProblem"
                    class="tree-edit-error"
                    :class="`is-${treeEditProblem.severity}`"
                  >{{ treeEditProblem.message }}</span>
                </div>
                </template>
              </ExplorerTreeList>
            </template>
          </section>
        </div>
        </div>
        </UiPane>
        <UiPane
          v-if="showOutlineView"
          :title="t('workbench.outline')"
          storage-key="multara.explorer.outline"
          size-key="multara.explorer.outlineSize"
          :default-expanded="false"
          @contextmenu="openExplorerViewMenu"
        >
          <template #actions>
            <UiButton
              variant="ghost"
              size="icon"
              :title="outlineFollowCursor ? t('workbench.outlineFollowOn') : t('workbench.outlineFollowOff')"
              :class="{ 'is-active-action': outlineFollowCursor }"
              @click="toggleOutlineFollow"
            >
              <Codicon name="target" />
            </UiButton>
            <UiButton variant="ghost" size="icon" :title="t('workbench.collapseAll')" @click="outlinePane?.collapseAll()">
              <Codicon name="collapse-all" />
            </UiButton>
          </template>
          <EditorOutline
            ref="outlinePane"
            :items="activeOutline"
            :active-line="activeSymbol?.line ?? editorStatus.line"
            :follow-cursor="outlineFollowCursor"
            @reveal="(line) => editorWithDiff?.reveal(line)"
          />
        </UiPane>
        <UiPane
          v-if="showTimeline"
          ref="timelineUiPane"
          :title="t('workbench.timeline')"
          storage-key="multara.explorer.timeline"
          size-key="multara.explorer.timelineSize"
          :default-expanded="false"
          @contextmenu="openExplorerViewMenu"
        >
          <template #actions>
            <UiButton
              variant="ghost"
              size="icon"
              :title="timelinePane?.pinned ? t('workbench.timelineUnpin') : t('workbench.timelinePinFile')"
              @click="timelinePane?.togglePin()"
            >
              <Codicon :name="timelinePane?.pinned ? 'pinned' : 'pin'" />
            </UiButton>
            <UiButton variant="ghost" size="icon" :title="t('workbench.refresh')" @click="timelinePane?.refresh()">
              <Codicon name="refresh" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon"
              :title="t('workbench.filterTimeline')"
              @click="timelinePane?.openFilter($event)"
            >
              <Codicon name="filter" />
            </UiButton>
          </template>
          <ExplorerTimeline
            ref="timelinePane"
            :workspace-id="activeEditorTab()?.workspaceId ?? ''"
            :path="activeEditorTab()?.path ?? ''"
            :git="activeEditorIsGit()"
            :preview="preview"
            @open="openSourceControlChange"
            @restore="restoreLocalHistoryEntry"
          />
        </UiPane>
      </UiPaneView>
      </template>
      </div>
    </aside>
    <ContextMenu
      v-if="treeMenu"
      :x="treeMenu.x"
      :y="treeMenu.y"
      :items="treeMenuItems"
      @select="onTreeMenuSelect"
      @close="treeMenu = null"
    />
    <ContextMenu
      v-if="explorerViewMenu"
      :x="explorerViewMenu.x"
      :y="explorerViewMenu.y"
      :items="explorerViewMenuItems"
      @select="toggleExplorerView"
      @close="explorerViewMenu = null"
    />
    <ContextMenu
      v-if="tabMenu"
      :x="tabMenu.x"
      :y="tabMenu.y"
      :items="tabMenuItems"
      @select="onTabMenuSelect"
      @close="tabMenu = null"
    />
    <ContextMenu
      v-if="openEditorsMenu"
      :x="openEditorsMenu.x"
      :y="openEditorsMenu.y"
      :items="openEditorsMenuItems"
      @select="onOpenEditorsMenuSelect"
      @close="openEditorsMenu = null"
    />
    <ExplorerDialog
      v-if="explorerDialog"
      :request="explorerDialog"
      @resolve="explorer.resolveDialog"
    />
    <div
      class="panel-resize-handle"
      role="separator"
      :aria-label="t('workbench.resizeProjectSidebar')"
      aria-orientation="vertical"
      tabindex="0"
      @pointerdown="beginResize('left', $event)"
      @keydown="resizeWithKeyboard('left', $event)"
    />

    <section class="editor-area" data-workspace-drop="editor">
      <header class="editor-tabbar">
        <div ref="editorTabsEl" class="editor-tabs vs-scroll">
          <button
            v-for="tab in editorTabs"
            :key="tabKey(tab)"
            type="button"
            class="editor-tab"
            :class="{
              active: activeEditorKey === tabKey(tab),
              'is-preview': !tab.pinned,
              'is-dirty': isEditorTabDirty(tab),
            }"
            :title="editorTabTitle(tab)"
            @click="activateEditorTab(tab)"
            @dblclick="pinEditorTab(tab)"
            @click.middle.prevent="closeEditorTab(tab, $event)"
            @mousedown.middle.prevent
            @mousedown="$event.button === 2 && $event.preventDefault()"
            @contextmenu="openTabMenu(tab, $event)"
          >
            <FileTypeIcon :name="fileNameOf(tab.path)" />
            <span class="editor-tab-name">{{ fileNameOf(tab.path) }}</span>
            <span v-if="tab.gitDiff" class="editor-tab-diff">{{ tab.gitDiff.short }}</span>
            <span
              class="tab-close"
              :class="{ 'is-dirty': isEditorTabDirty(tab) }"
              :title="t('common.close')"
              @click="closeEditorTab(tab, $event)"
            >
              <X :size="12" />
            </span>
          </button>
        </div>
        <div
          v-if="markdownToggleVisible"
          class="md-editor-switch"
          role="tablist"
          :aria-label="t('workbench.markdownView')"
        >
          <button
            type="button"
            role="tab"
            :class="{ active: markdownMode === 'preview' }"
            :aria-selected="markdownMode === 'preview'"
            :title="t('workbench.markdownPreviewTitle')"
            @click="setMarkdownMode('preview')"
          >
            Preview
          </button>
          <button
            type="button"
            role="tab"
            :class="{ active: markdownMode === 'source' }"
            :aria-selected="markdownMode === 'source'"
            :title="t('workbench.markdownSourceTitle')"
            @click="setMarkdownMode('source')"
          >
            Markdown
          </button>
        </div>
      </header>
      <UiNotice
        v-if="error"
        class="editor-notice"
        tone="error"
        sticky
        :message="error"
        @close="dismissEditorError"
      />
      <div v-if="externalChangePending" class="editor-external-change">
        <span>{{ externalChangeMessage }}</span>
        <UiButton size="small" @click="externalChangePending = false"
          >{{ t('workbench.keepEditorContent') }}</UiButton
        >
        <UiButton size="small" @click="refreshActiveFile(true)"
          >{{ t('workbench.loadDiskVersion') }}</UiButton
        >
      </div>
      <EditorBreadcrumbs
        v-if="activeFile"
        :parts="activeBreadcrumbs"
        :symbol="activeSymbol"
        @reveal="(line) => editorWithDiff?.reveal(line)"
      />
      <div v-if="activeFile" class="editor-main">
        <div v-if="pendingDiffFiles.length" class="chat-editor-overlay-widget">
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('diff.prevPendingFile')"
            @click="goPendingDiffFile(-1)"
          >
            <ChevronLeft :size="13" />
          </UiButton>
          <span class="diff-file-nav-index">{{ pendingDiffLabel }}</span>
          <UiButton
            variant="ghost"
            size="icon"
            :title="t('diff.nextPendingFile')"
            @click="goPendingDiffFile(1)"
          >
            <ChevronRight :size="13" />
          </UiButton>
          <template v-if="editorDiffGroupCount">
            <UiButton
              class="diff-hunk-nav"
              variant="ghost"
              size="icon"
              :title="t('diff.prevHunk')"
              @click="stepDiffGroup(-1)"
            >
              <ChevronUp :size="13" />
            </UiButton>
            <span class="diff-file-nav-index">{{ editorDiffGroupIndex + 1 }}/{{ editorDiffGroupCount }}</span>
            <UiButton
              variant="ghost"
              size="icon"
              :title="t('diff.nextHunk')"
              @click="stepDiffGroup(1)"
            >
              <ChevronDown :size="13" />
            </UiButton>
          </template>
          <UiButton
            class="diff-file-action diff-file-action-keep"
            size="small"
            :title="activeFileBeingWritten ? t('diff.writingFile') : t('diff.keepAllTitle')"
            :disabled="acceptFileActionDisabled"
            @click="acceptAllCurrentFile"
          >
            {{ t('diff.keep') }}
          </UiButton>
          <UiButton
            class="diff-file-action"
            variant="ghost"
            size="small"
            :title="activeFileBeingWritten ? t('diff.writingFile') : t('diff.undoAllWriteBackTitle')"
            :disabled="undoFileActionDisabled"
            @click="undoAllCurrentFile"
          >
            {{ t('diff.undo') }}
          </UiButton>
        </div>
        <MediaPreview
          v-if="activeMedia"
          :kind="activeMedia"
          :src="activeFile.content"
          :name="activeFile.path"
        />
        <GitDiffEditor
          v-else-if="activeGitDiff"
          :diff-id="activeGitDiff.id"
          :original="activeGitDiff.original"
          :modified="activeGitDiff.modified"
          :language="activeFile.language"
          :theme="editorTheme"
          :binary="activeGitDiff.binary"
        />
        <template v-else>
          <MarkdownPreview
            v-if="markdownMode === 'preview'"
            :content="activeFile.content"
            :path="activeFile.path"
            @open="openMarkdownLink"
          />
          <EditorWithDiff
            v-show="markdownMode !== 'preview'"
            ref="editorWithDiff"
          :file-path="activeFile.path"
          :workspace-id="activeFile.workspaceId"
          :content="activeFile.content"
          :language="activeFile.language"
          :read-only="activeFile.readOnly"
          :theme="editorTheme"
          :tab-size="activeEditorOptions.tabSize"
          :insert-spaces="activeEditorOptions.insertSpaces"
          :sticky-scroll="stickyScroll"
          :git-epoch="gitEpoch"
          :writing="activeFileBeingWritten"
          @change="onEditorContentChange"
          @mount="onEditorMount"
          @pending-chunks="onEditorPendingChunks"
          @diff-groups="onEditorDiffGroups"
        />
        </template>
      </div>
      <div v-else class="editor-welcome">
        <ul class="editor-shortcut-hints">
          <li v-for="item in editorShortcutHints" :key="item.label">
            <span>{{ item.label }}</span>
            <span class="editor-shortcut-keys">
              <span v-for="(group, groupIndex) in item.keys" :key="groupIndex" class="editor-shortcut-chord">
                <kbd v-for="key in group" :key="key">{{ key }}</kbd>
              </span>
            </span>
          </li>
        </ul>
      </div>
      <TerminalPanel
        v-show="terminalOpen"
        :data-open="terminalOpen ? 'true' : 'false'"
        ref="terminalPanel"
        :sessions="terminalTabs"
        :active-id="activeTerminalId"
        :height="terminalHeight"
        :new-title="newTerminalTitle"
        :profiles="terminalProfiles"
        :default-profile="defaultTerminalProfile"
        :open="terminalOpen"
        @close="terminalOpen = false"
        @new="createTerminal"
        @split="splitTerminal"
        @select="selectTerminal"
        @kill="killTerminal"
        @reorder="reorderTerminal"
        @resize="beginTerminalResize"
        @patch="patchTerminal"
        @add-to-chat="addTerminalSelectionToChat"
        @open-path="openTerminalPath"
        @open-url="(url) => backend.openExternalUrl(url)"
        @default-profile="setDefaultTerminalProfile"
        @run-file="runActiveFileInTerminal"
        @run-selection="runSelectionInTerminal"
      />
      <QuickOpen
        v-if="quickOpenVisible"
        :key="quickOpenSeed"
        :files="quickOpenFiles"
        :loading="quickOpenLoading"
        :initial-query="quickOpenSeed"
        @close="closeQuickOpen"
        @open="openQuickOpenFile"
      />
      <div
        v-if="terminalPick"
        ref="terminalPickEl"
        class="quick-pick"
        role="dialog"
        :aria-label="t('workbench.terminalPickTitle')"
      >
        <input
          ref="terminalPickInput"
          v-model="terminalPickQuery"
          :placeholder="t('workbench.terminalPickPlaceholder')"
          :aria-label="t('workbench.terminalPickTitle')"
          @keydown.escape.stop="terminalPick = false"
          @keydown.enter.prevent="confirmTerminalPick"
          @keydown.down.prevent="moveTerminalPick(1)"
          @keydown.up.prevent="moveTerminalPick(-1)"
        />
        <button
          v-for="(workspace, index) in terminalPickChoices"
          :key="workspace.id"
          type="button"
          :class="{ active: index === terminalPickIndex }"
          @click="openTerminalFor(workspace)"
          @mouseenter="terminalPickIndex = index"
        >
          <Codicon :name="workspace.kind === 'git' ? 'repo' : 'folder'" :size="16" />
          <span>{{ workspace.name }}</span>
          <small :title="workspace.workPath || workspace.sourcePath">{{ workspace.workPath || workspace.sourcePath }}</small>
        </button>
        <p v-if="!terminalPickChoices.length">{{ t('workbench.noMatchingProjects') }}</p>
      </div>
      <EditorZoomHud v-if="activeFile" />
    </section>

    <div
      class="panel-resize-handle"
      role="separator"
      :aria-label="t('workbench.resizeChatSidebar')"
      aria-orientation="vertical"
      tabindex="0"
      @pointerdown="beginResize('right', $event)"
      @keydown="resizeWithKeyboard('right', $event)"
    />

    <aside class="project-chat">
      <ChatPanel
        :key="preview ? 'preview' : projectId"
        ref="chatPanel"
        :project-id="preview ? PREVIEW_PROJECT_ID : projectId"
        compact
        session-commands
        :preview="preview"
        :preview-interactive="previewInteractive"
        :preview-messages="previewMessages"
        :preview-composer-text="previewComposerText"
        :preview-session-title="previewSessionTitle"
        :preview-busy="previewBusy"
        :allow-target-change="!preview"
      />
    </aside>

    <UiModal
      v-if="filePick"
      @click.self="filePick = null"
    >
      <div
        ref="filePickDialog"
        class="modal file-pick"
        role="dialog"
        aria-modal="true"
        :aria-label="t('workbench.filePickTitle')"
        tabindex="-1"
        @keydown.escape="filePick = null"
      >
        <h2>{{ t('workbench.filePickTitle') }}</h2>
        <div class="file-pick-list vs-scroll">
          <button
            v-for="choice in filePick.choices"
            :key="`${choice.workspaceId}\n${choice.path}`"
            type="button"
            class="file-pick-row"
            @click="choosePickedFile(choice)"
          >
            {{ choice.label }}
          </button>
        </div>
      </div>
    </UiModal>
    <AddWorkspaceModal
      v-if="showAddWorkspaceModal && projectId"
      :project-id="projectId"
      :initial-paths="addWorkspaceInitialPaths"
      @close="closeAddWorkspaceModal"
      @added="closeAddWorkspaceModal"
    />
  </div>
</template>
