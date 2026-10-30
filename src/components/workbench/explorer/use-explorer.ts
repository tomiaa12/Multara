import { computed, onBeforeUnmount, onMounted, ref, shallowRef, triggerRef, watch } from "vue";
import type { FileEntry, Workspace } from "../../../domain/types";
import { monaco } from "../../../monaco";
import * as backend from "../../../services/backend";
import * as explorerBackend from "../../../services/explorer-backend";
import { t } from "../../../i18n";
import { useAppStore } from "../../../stores/app";
import { canDropInExplorer, fileNamesMessage, moveConfirmMessage } from "./explorer-dnd";
import { readDroppedTree, type DroppedItem } from "./explorer-import";
import { findAvailableName } from "./explorer-names";
import {
  EMPTY_SELECTION,
  distinctParents,
  moveFocus,
  pruneSelection,
  selectAll,
  selectOnly,
  selectRange,
  toggleSelection,
  typeNavigate,
  type FocusMove,
  type SelectionState,
} from "./explorer-selection";
import {
  applySeparator,
  parseExplorerSettings,
  serializeExplorerSettings,
  type ExplorerSettings,
} from "./explorer-settings";
import { ExplorerUndoStack, type UndoableFileOperation } from "./explorer-undo";

export interface ExplorerItem {
  key: string;
  workspaceId: string;
  /** 工作区根为空串。 */
  path: string;
  name: string;
  isDirectory: boolean;
  readonly?: boolean;
}

export interface ExplorerDialogRequest {
  message: string;
  detail?: string;
  primary: string;
  cancel?: string;
  type?: "warning" | "info";
  danger?: boolean;
  /** 有值时显示“不再询问”之类的复选框。 */
  checkbox?: string;
}

export interface ExplorerDialogResult {
  confirmed: boolean;
  checked: boolean;
}

export interface ExplorerHost {
  preview: () => boolean;
  workspaces: () => Workspace[];
  /** 当前可见的行（含工作区根），顺序即键盘导航与 Shift 选择的顺序。 */
  visibleItems: () => ExplorerItem[];
  /** 已加载的目录内容；未加载返回 undefined。 */
  levelEntries: (workspaceId: string, dirPath: string) => FileEntry[] | undefined;
  reloadLevel: (workspaceId: string, dirPath: string) => Promise<void>;
  reloadWorkspace: (workspaceId: string) => Promise<void>;
  loadedPaths: (workspaceId: string) => string[];
  isGitWorkspace: (workspaceId: string) => boolean;
  afterMutation: () => void;
  gitChanged: () => void;
  renameEditors: (workspaceId: string, from: string, to: string) => void;
  /** 关闭路径本身及其下的编辑器，不询问保存。 */
  closeEditors: (workspaceId: string, path: string) => void;
  dirtyEditors: (workspaceId: string, path: string) => number;
  openFile: (workspaceId: string, path: string) => Promise<void>;
  revealEntry: (workspaceId: string, path: string, isDirectory: boolean) => Promise<void>;
  expandFolder: (workspaceId: string, path: string) => Promise<void>;
  absolutePath: (workspaceId: string, path: string) => string;
  previewDelete?: (item: ExplorerItem) => void;
  reportError: (message: string) => void;
}

interface TransferSource {
  /** 为空时 `path` 是本机绝对路径（从系统剪贴板粘贴）。 */
  workspaceId: string | null;
  path: string;
  name: string;
  isDirectory: boolean;
}

interface TransferTarget {
  workspaceId: string;
  path: string;
  name: string;
}

export function explorerItemKey(workspaceId: string, path: string) {
  return `${workspaceId}:${path}`;
}

function parentOf(path: string) {
  const slash = path.lastIndexOf("/");
  return slash === -1 ? "" : path.slice(0, slash);
}

function baseName(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function joinPath(dir: string, name: string) {
  return dir ? `${dir}/${name}` : name;
}

function messageOf(cause: unknown) {
  return cause instanceof Error ? cause.message : String(cause);
}

function trashLabel(isWindows: boolean) {
  return t(isWindows ? "explorer.trash.windows" : "explorer.trash.mac");
}

function entryKind(isDirectory: boolean) {
  return t(isDirectory ? "explorer.kind.folder" : "explorer.kind.file");
}

function transferOpLabel(copy: boolean, count: number, singleName: string) {
  if (count === 1) return t(copy ? "explorer.op.copy" : "explorer.op.move", { name: singleName });
  return t(copy ? "explorer.op.copyMany" : "explorer.op.moveMany", { count });
}

function isUnder(path: string, folder: string) {
  return folder === "" || path === folder || path.startsWith(`${folder}/`);
}

export function useExplorer(host: ExplorerHost) {
  const store = useAppStore();
  const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
  const isWindows = /Win/.test(navigator.platform);

  // ---- 设置 ----
  const previewSettings = ref<string | null>(null);
  const settings = computed<ExplorerSettings>(() =>
    parseExplorerSettings(previewSettings.value ?? store.data?.settings.explorerSettings),
  );

  async function updateSettings(patch: Partial<ExplorerSettings>) {
    const raw = serializeExplorerSettings({ ...settings.value, ...patch });
    if (host.preview() || !store.data) {
      previewSettings.value = raw;
      return;
    }
    try {
      await store.saveSettings({ explorerSettings: raw });
    } catch (cause) {
      host.reportError(messageOf(cause));
    }
  }

  // ---- 确认对话框 ----
  const dialog = shallowRef<(ExplorerDialogRequest & { resolve: (result: ExplorerDialogResult) => void }) | null>(null);

  function confirm(request: ExplorerDialogRequest): Promise<ExplorerDialogResult> {
    dialog.value?.resolve({ confirmed: false, checked: false });
    return new Promise((resolve) => {
      dialog.value = {
        ...request,
        resolve: (result) => {
          dialog.value = null;
          resolve(result);
        },
      };
    });
  }

  function resolveDialog(result: ExplorerDialogResult) {
    dialog.value?.resolve(result);
  }

  // ---- 多选 ----
  const selection = ref<SelectionState>(EMPTY_SELECTION);
  const known = new Map<string, ExplorerItem>();
  const visibleItems = computed(() => host.visibleItems());
  const order = computed(() => visibleItems.value.map((item) => item.key));
  const itemMap = computed(() => new Map(visibleItems.value.map((item) => [item.key, item])));
  const selectedKeys = computed(() => new Set(selection.value.selected));

  function itemOf(key: string | null) {
    if (!key) return null;
    return itemMap.value.get(key) ?? known.get(key) ?? null;
  }

  function remember(item: ExplorerItem) {
    known.set(item.key, item);
    if (known.size > 2000) known.delete(known.keys().next().value as string);
  }

  const selectedItems = computed(() =>
    selection.value.selected.map(itemOf).filter((item): item is ExplorerItem => Boolean(item)),
  );
  const focusedItem = computed(() => itemOf(selection.value.focus));

  function isSelected(key: string) {
    return selectedKeys.value.has(key);
  }

  function isFocused(key: string) {
    return selection.value.focus === key;
  }

  function select(item: ExplorerItem) {
    remember(item);
    selection.value = selectOnly(item.key);
  }

  function clearSelection() {
    selection.value = EMPTY_SELECTION;
  }

  /** 返回 true 表示普通单击（调用方据此打开文件）。 */
  function clickSelect(item: ExplorerItem, event: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }) {
    remember(item);
    const toggle = isMac ? event.metaKey : event.ctrlKey;
    if (event.shiftKey) {
      selection.value = selectRange(selection.value, order.value, item.key, toggle);
      return false;
    }
    if (toggle) {
      selection.value = toggleSelection(selection.value, item.key);
      return false;
    }
    selection.value = selectOnly(item.key);
    return true;
  }

  /** 右键：点在已选项上保留多选，否则改为只选它（VS Code 行为）。 */
  function contextSelect(item: ExplorerItem) {
    remember(item);
    if (isSelected(item.key)) selection.value = { ...selection.value, focus: item.key };
    else selection.value = selectOnly(item.key);
  }

  function moveSelection(move: FocusMove, extend: boolean, pageSize = 10) {
    const next = moveFocus(order.value, selection.value.focus, move, pageSize);
    if (!next) return null;
    selection.value = extend ? selectRange(selection.value, order.value, next) : selectOnly(next);
    return itemOf(next);
  }

  /** Ctrl+方向键：只移动焦点，不改变选择。 */
  function moveFocusOnly(move: "next" | "previous") {
    const next = moveFocus(order.value, selection.value.focus, move, 1);
    if (!next) return null;
    selection.value = { ...selection.value, focus: next };
    return itemOf(next);
  }

  /** Ctrl+Space：切换焦点项的选中状态。 */
  function toggleFocused() {
    const key = selection.value.focus;
    if (!key) return null;
    selection.value = toggleSelection(selection.value, key);
    return itemOf(key);
  }

  function focusKey(key: string) {
    const item = itemOf(key);
    if (!item) return null;
    selection.value = selectOnly(key);
    return item;
  }

  function selectAllVisible() {
    selection.value = selectAll(order.value, selection.value.focus);
  }

  // 宿主的可见行通常依赖本组合函数的返回值，挂载后再订阅，避免初始化期间循环访问。
  onMounted(() => {
    watch(order, (next, previous) => {
      if (!selection.value.selected.length && !selection.value.focus) return;
      const alive = new Set(next);
      if (selection.value.selected.every((key) => alive.has(key)) && (!selection.value.focus || alive.has(selection.value.focus))) return;
      selection.value = pruneSelection(selection.value, next, previous ?? []);
    });
  });

  /** 操作对象：右键/快捷键作用在整个多选上，前提是焦点项在多选里。 */
  function actionItems(anchor?: ExplorerItem | null) {
    const focus = anchor ?? focusedItem.value;
    if (!focus) return [];
    if (isSelected(focus.key) && selectedItems.value.length > 1) return selectedItems.value;
    return [focus];
  }

  // ---- 键入即定位 ----
  let typeBuffer = "";
  let typeTimer = 0;

  function typeAhead(char: string) {
    window.clearTimeout(typeTimer);
    typeBuffer += char;
    typeTimer = window.setTimeout(() => {
      typeBuffer = "";
    }, 800);
    const names = new Map(visibleItems.value.map((item) => [item.key, item.name]));
    const key = typeNavigate(order.value, names, selection.value.focus, typeBuffer);
    return key ? focusKey(key) : null;
  }

  // ---- 查找 ----
  const findOpen = ref(false);
  const findQuery = ref("");
  const findMode = ref<"highlight" | "filter">("highlight");

  function closeFind() {
    findOpen.value = false;
    findQuery.value = "";
  }

  // ---- 比较 ----
  const compareSource = shallowRef<ExplorerItem | null>(null);

  // ---- 剪贴板 ----
  const clipboard = shallowRef<{ items: ExplorerItem[]; cut: boolean } | null>(null);
  let nativeSignature = "";
  const cutKeys = computed(() =>
    clipboard.value?.cut ? new Set(clipboard.value.items.map((item) => item.key)) : new Set<string>(),
  );

  async function setClipboard(items: ExplorerItem[], cut: boolean) {
    const files = distinctParents(items.filter((item) => item.path !== "" || !cut));
    if (!files.length) return;
    clipboard.value = { items: files, cut };
    nativeSignature = "";
    if (host.preview()) return;
    try {
      await explorerBackend.writeClipboardFiles(files.map((item) => ({ workspaceId: item.workspaceId, path: item.path })));
      nativeSignature = (await explorerBackend.readClipboardFiles()).map((file) => file.path).join("\n");
    } catch {
      /* 写系统剪贴板失败时仍可在应用内粘贴 */
    }
  }

  function cancelCut() {
    if (!clipboard.value?.cut) return false;
    clipboard.value = null;
    return true;
  }

  // ---- 撤销/重做 ----
  const undoStack = new ExplorerUndoStack();
  const undoVersion = ref(0);
  const canUndo = computed(() => (void undoVersion.value, undoStack.canUndo));
  const canRedo = computed(() => (void undoVersion.value, undoStack.canRedo));

  function pushUndo(operation: UndoableFileOperation) {
    if (!settings.value.enableUndo) return;
    undoStack.push(operation);
    undoVersion.value += 1;
  }

  async function undo() {
    const operation = undoStack.peekUndo();
    if (!operation) return;
    const mode = settings.value.confirmUndo;
    if (mode === "verbose" || (mode === "default" && operation.destructiveUndo)) {
      const answer = await confirm({
        message: t("explorer.undo.confirm", { label: operation.label }),
        primary: t("explorer.confirm.undo"),
        type: "warning",
      });
      if (!answer.confirmed) return;
    }
    try {
      await undoStack.undo();
    } catch (cause) {
      host.reportError(t("explorer.undo.failed", { message: messageOf(cause) }));
    } finally {
      undoVersion.value += 1;
      host.afterMutation();
    }
  }

  async function redo() {
    const operation = undoStack.peekRedo();
    if (!operation) return;
    if (settings.value.confirmUndo === "verbose") {
      const answer = await confirm({
        message: t("explorer.redo.confirm", { label: operation.label }),
        primary: t("explorer.confirm.redo"),
      });
      if (!answer.confirmed) return;
    }
    try {
      await undoStack.redo();
    } catch (cause) {
      host.reportError(t("explorer.redo.failed", { message: messageOf(cause) }));
    } finally {
      undoVersion.value += 1;
      host.afterMutation();
    }
  }

  async function reloadDirs(pairs: Array<[string, string]>) {
    const unique = new Map(pairs.map(([workspaceId, dir]) => [`${workspaceId}\n${dir}`, [workspaceId, dir] as const]));
    await Promise.all(
      [...unique.values()].map(([workspaceId, dir]) => host.reloadLevel(workspaceId, dir).catch(() => undefined)),
    );
  }

  /** 删掉并保留备份，返回可恢复的记录。 */
  async function deleteWithBackup(workspaceId: string, paths: string[]) {
    const results = await explorerBackend.deleteEntries(workspaceId, paths, false);
    const failed = results.find((result) => result.error);
    if (failed) throw new Error(failed.error ?? t("explorer.delete.failed"));
    return results.map((result) => ({ path: result.path, backupId: result.backupId }));
  }

  async function restoreBackups(workspaceId: string, records: Array<{ path: string; backupId: string | null }>) {
    for (const record of records) {
      if (!record.backupId) throw new Error(t("explorer.undo.tooLarge", { name: baseName(record.path) }));
      await explorerBackend.restoreEntry(workspaceId, record.path, record.backupId);
    }
  }

  /** 新建/复制产生的文件：撤消=删除（留备份），重做=从备份恢复。 */
  function createdUndo(label: string, workspaceId: string, paths: string[]): UndoableFileOperation {
    let records: Array<{ path: string; backupId: string | null }> = [];
    return {
      label,
      destructiveUndo: true,
      undo: async () => {
        for (const path of paths) host.closeEditors(workspaceId, path);
        records = await deleteWithBackup(workspaceId, paths);
        await reloadDirs(paths.map((path) => [workspaceId, parentOf(path)]));
      },
      redo: async () => {
        await restoreBackups(workspaceId, records);
        await reloadDirs(paths.map((path) => [workspaceId, parentOf(path)]));
      },
    };
  }

  function recordCreate(workspaceId: string, path: string, isDirectory: boolean) {
    pushUndo(
      createdUndo(
        t(isDirectory ? "explorer.op.createFolder" : "explorer.op.createFile", { name: baseName(path) }),
        workspaceId,
        [path],
      ),
    );
  }

  function recordRename(workspaceId: string, from: string, to: string) {
    pushUndo({
      label: t("explorer.op.rename", { name: baseName(from) }),
      destructiveUndo: false,
      undo: async () => {
        await backend.renameWorkspaceEntry(workspaceId, to, from);
        host.renameEditors(workspaceId, to, from);
        await reloadDirs([[workspaceId, parentOf(to)], [workspaceId, parentOf(from)]]);
      },
      redo: async () => {
        await backend.renameWorkspaceEntry(workspaceId, from, to);
        host.renameEditors(workspaceId, from, to);
        await reloadDirs([[workspaceId, parentOf(to)], [workspaceId, parentOf(from)]]);
      },
    });
  }

  // ---- 删除 ----
  async function deleteItems(items: ExplorerItem[], permanent: boolean) {
    const targets = distinctParents(items).filter((item) => item.path !== "");
    if (!targets.length) return;
    const names = targets.map((item) => item.name);
    const dirty = targets.reduce((sum, item) => sum + host.dirtyEditors(item.workspaceId, item.path), 0);
    if (dirty > 0) {
      let unsavedMessage: string;
      if (targets.length === 1) {
        unsavedMessage = t("explorer.delete.unsavedOne", {
          kind: entryKind(targets[0].isDirectory),
          name: names[0],
        });
      } else {
        unsavedMessage = t("explorer.delete.unsavedMany", { count: dirty });
      }
      const answer = await confirm({
        type: "warning",
        message: unsavedMessage,
        detail: t("explorer.delete.unsavedDetail"),
        primary: t("common.delete"),
        danger: true,
      });
      if (!answer.confirmed) return;
    }

    if (host.preview()) {
      for (const item of targets) host.previewDelete?.(item);
      return;
    }

    const useTrash = !permanent;
    let what: string;
    if (targets.length === 1) {
      const suffix = targets[0].isDirectory ? t("explorer.delete.andContents") : "";
      what = t("explorer.delete.singleTarget", { name: names[0], suffix });
    } else {
      const hasDirectory = targets.some((item) => item.isDirectory);
      const typeLabel = hasDirectory ? t("explorer.delete.manyTypeMixed") : t("explorer.delete.manyTypeFiles");
      what = t("explorer.delete.manyTarget", { count: targets.length, type: typeLabel });
    }
    if (!useTrash || settings.value.confirmDelete) {
      const listing = targets.length > 1 ? fileNamesMessage(names) : "";
      const trash = trashLabel(isWindows);
      const detailParts = [listing];
      if (useTrash) detailParts.push(t("explorer.delete.restoreFromTrash", { trash }));
      else detailParts.push(t("explorer.delete.noTrash"));
      const answer = await confirm({
        type: "warning",
        message: useTrash ? t("explorer.delete.confirmTrash", { what }) : t("explorer.delete.confirmPermanent", { what }),
        detail: detailParts.filter(Boolean).join("\n\n"),
        primary: useTrash
          ? t("explorer.confirm.moveToTrash", { trash })
          : t("explorer.confirm.permanentDelete"),
        danger: true,
        checkbox: useTrash ? t("explorer.confirm.dontAskAgain") : undefined,
      });
      if (!answer.confirmed) return;
      if (answer.checked) void updateSettings({ confirmDelete: false });
    }

    const byWorkspace = new Map<string, ExplorerItem[]>();
    for (const item of targets) byWorkspace.set(item.workspaceId, [...(byWorkspace.get(item.workspaceId) ?? []), item]);
    const done: Array<{ workspaceId: string; path: string; backupId: string | null }> = [];
    const errors: string[] = [];
    for (const [workspaceId, list] of byWorkspace) {
      let results: explorerBackend.ExplorerDeleteResult[];
      try {
        results = await explorerBackend.deleteEntries(workspaceId, list.map((item) => item.path), useTrash);
      } catch (cause) {
        errors.push(messageOf(cause));
        continue;
      }
      const trashFailed = results.filter((result) => result.error?.startsWith(explorerBackend.TRASH_FAILED_PREFIX));
      if (trashFailed.length) {
        const trash = trashLabel(isWindows);
        const trashMessage =
          trashFailed.length === 1
            ? t("explorer.delete.trashFailOne", { name: baseName(trashFailed[0].path), trash })
            : t("explorer.delete.trashFailMany", { count: trashFailed.length, trash });
        const answer = await confirm({
          type: "warning",
          message: trashMessage,
          detail: trashFailed[0].error?.slice(explorerBackend.TRASH_FAILED_PREFIX.length) ?? "",
          primary: t("explorer.confirm.permanentDelete"),
          danger: true,
        });
        if (answer.confirmed) {
          try {
            const retried = await explorerBackend.deleteEntries(workspaceId, trashFailed.map((result) => result.path), false);
            results = results.map((result) => retried.find((item) => item.path === result.path) ?? result);
          } catch (cause) {
            errors.push(messageOf(cause));
          }
        }
      }
      for (const result of results) {
        if (result.error) {
          if (!result.error.startsWith(explorerBackend.TRASH_FAILED_PREFIX)) errors.push(result.error);
          continue;
        }
        host.closeEditors(workspaceId, result.path);
        done.push({ workspaceId, path: result.path, backupId: result.backupId });
      }
    }
    if (errors.length) host.reportError(errors.join("\n"));
    if (!done.length) return;
    let records = done;
    pushUndo({
      label:
        done.length === 1
          ? t("explorer.op.delete", { name: baseName(done[0].path) })
          : t("explorer.op.deleteMany", { count: done.length }),
      destructiveUndo: false,
      undo: async () => {
        for (const record of records) await restoreBackups(record.workspaceId, [record]);
        await reloadDirs(records.map((record) => [record.workspaceId, parentOf(record.path)]));
      },
      redo: async () => {
        const next: typeof records = [];
        for (const record of records) {
          host.closeEditors(record.workspaceId, record.path);
          const [again] = await deleteWithBackup(record.workspaceId, [record.path]);
          next.push({ workspaceId: record.workspaceId, ...again });
        }
        records = next;
        await reloadDirs(records.map((record) => [record.workspaceId, parentOf(record.path)]));
      },
    });
    await reloadDirs(done.map((record) => [record.workspaceId, parentOf(record.path)]));
    host.afterMutation();
  }

  // ---- 复制/移动（粘贴与拖放共用） ----
  async function transferInto(
    sources: TransferSource[],
    target: TransferTarget,
    copy: boolean,
    options: { incremental: boolean },
  ) {
    await host.reloadLevel(target.workspaceId, target.path).catch(() => undefined);
    const taken = new Set((host.levelEntries(target.workspaceId, target.path) ?? []).map((entry) => entry.name));
    const naming = settings.value.incrementalNaming;
    const plain: explorerBackend.ExplorerTransferRequest[] = [];
    const replace: explorerBackend.ExplorerTransferRequest[] = [];
    const meta = new Map<string, TransferSource>();
    for (const source of sources) {
      if (!copy && source.workspaceId === target.workspaceId && parentOf(source.path) === target.path) continue;
      if (source.workspaceId === target.workspaceId && source.isDirectory && isUnder(target.path, source.path)) {
        host.reportError(t("explorer.transfer.cycleError", { name: source.name }));
        continue;
      }
      let name = source.name;
      let overwrite = false;
      if (taken.has(name)) {
        if (copy && options.incremental && naming !== "disabled") {
          name = findAvailableName(name, source.isDirectory, naming, (candidate) => taken.has(candidate));
        } else {
          const answer = await confirm({
            type: "warning",
            message: t("explorer.transfer.replaceConfirm", { name }),
            detail: t("explorer.transfer.irreversible"),
            primary: t("explorer.confirm.replace"),
            danger: true,
          });
          if (!answer.confirmed) continue;
          overwrite = true;
        }
      }
      taken.add(name);
      const request = {
        sourceWorkspaceId: source.workspaceId,
        source: source.path,
        targetWorkspaceId: target.workspaceId,
        target: joinPath(target.path, name),
      };
      meta.set(request.target, source);
      (overwrite ? replace : plain).push(request);
    }
    const succeeded: explorerBackend.ExplorerTransferRequest[] = [];
    const errors: string[] = [];
    for (const [batch, overwrite] of [[plain, false], [replace, true]] as const) {
      if (!batch.length) continue;
      try {
        const results = await explorerBackend.transferEntries(batch, copy, overwrite);
        results.forEach((result, index) => {
          if (result.error) errors.push(result.error.replace(explorerBackend.TARGET_EXISTS_PREFIX, t("explorer.error.existsPrefix")));
          else succeeded.push(batch[index]);
        });
      } catch (cause) {
        errors.push(messageOf(cause));
      }
    }
    if (errors.length) host.reportError(errors.join("\n"));
    if (!succeeded.length) return [];

    for (const request of succeeded) {
      if (copy || !request.sourceWorkspaceId) continue;
      if (request.sourceWorkspaceId === request.targetWorkspaceId) {
        host.renameEditors(request.targetWorkspaceId, request.source, request.target);
      } else {
        host.closeEditors(request.sourceWorkspaceId, request.source);
      }
    }

    const label = transferOpLabel(
      copy,
      succeeded.length,
      copy ? baseName(succeeded[0].target) : baseName(succeeded[0].source),
    );
    if (!replace.some((request) => succeeded.includes(request))) {
      if (copy) {
        pushUndo(createdUndo(label, target.workspaceId, succeeded.map((request) => request.target)));
      } else if (succeeded.every((request) => request.sourceWorkspaceId)) {
        const back = succeeded.map((request) => ({
          sourceWorkspaceId: request.targetWorkspaceId,
          source: request.target,
          targetWorkspaceId: request.sourceWorkspaceId!,
          target: request.source,
        }));
        const dirs = (): Array<[string, string]> =>
          succeeded.flatMap((request) => [
            [request.targetWorkspaceId, parentOf(request.target)] as [string, string],
            [request.sourceWorkspaceId!, parentOf(request.source)] as [string, string],
          ]);
        pushUndo({
          label,
          destructiveUndo: false,
          undo: async () => {
            const results = await explorerBackend.transferEntries(back, false, false);
            const failed = results.find((result) => result.error);
            if (failed) throw new Error(failed.error!);
            for (const request of back) {
              if (request.sourceWorkspaceId === request.targetWorkspaceId) host.renameEditors(request.targetWorkspaceId, request.source, request.target);
            }
            await reloadDirs(dirs());
          },
          redo: async () => {
            const results = await explorerBackend.transferEntries(succeeded, false, false);
            const failed = results.find((result) => result.error);
            if (failed) throw new Error(failed.error!);
            for (const request of succeeded) {
              if (request.sourceWorkspaceId === request.targetWorkspaceId) host.renameEditors(request.targetWorkspaceId, request.source, request.target);
            }
            await reloadDirs(dirs());
          },
        });
      }
    }

    const dirs: Array<[string, string]> = [[target.workspaceId, target.path]];
    for (const request of succeeded) {
      if (!copy && request.sourceWorkspaceId) dirs.push([request.sourceWorkspaceId, parentOf(request.source)]);
    }
    await reloadDirs(dirs);
    host.afterMutation();
    await host.expandFolder(target.workspaceId, target.path).catch(() => undefined);
    const created = succeeded.map((request) => {
      const source = meta.get(request.target);
      const item: ExplorerItem = {
        key: explorerItemKey(target.workspaceId, request.target),
        workspaceId: target.workspaceId,
        path: request.target,
        name: baseName(request.target),
        isDirectory: Boolean(source?.isDirectory),
      };
      remember(item);
      return item;
    });
    selection.value = {
      selected: created.map((item) => item.key),
      focus: created[0]?.key ?? null,
      anchor: created[0]?.key ?? null,
    };
    return created;
  }

  function folderTarget(item: ExplorerItem | null): TransferTarget | null {
    if (!item) {
      const workspace = host.workspaces()[0];
      return workspace ? { workspaceId: workspace.id, path: "", name: workspace.name } : null;
    }
    if (item.isDirectory) {
      const root = item.path ? null : host.workspaces().find((entry) => entry.id === item.workspaceId);
      return { workspaceId: item.workspaceId, path: item.path, name: root?.name ?? item.name };
    }
    const dir = parentOf(item.path);
    const workspace = host.workspaces().find((entry) => entry.id === item.workspaceId);
    return { workspaceId: item.workspaceId, path: dir, name: dir ? baseName(dir) : (workspace?.name ?? "") };
  }

  async function paste(anchor?: ExplorerItem | null) {
    const target = folderTarget(anchor ?? focusedItem.value);
    if (!target) return;
    if (host.preview()) {
      host.reportError(t("explorer.paste.previewBlocked"));
      return;
    }
    let osFiles: explorerBackend.ExplorerClipboardFile[] = [];
    try {
      osFiles = await explorerBackend.readClipboardFiles();
    } catch {
      osFiles = [];
    }
    const signature = osFiles.map((file) => file.path).join("\n");
    if (osFiles.length && signature !== nativeSignature) {
      if (settings.value.confirmPasteNative) {
        const answer = await confirm({
          message:
            osFiles.length === 1
              ? t("explorer.paste.one", { name: osFiles[0].name, target: target.name })
              : t("explorer.paste.many", { count: osFiles.length, target: target.name }),
          detail: osFiles.length > 1 ? fileNamesMessage(osFiles.map((file) => file.name)) : "",
          primary: t("workbench.paste"),
          checkbox: t("explorer.confirm.dontAskAgain"),
        });
        if (!answer.confirmed) return;
        if (answer.checked) void updateSettings({ confirmPasteNative: false });
      }
      const created = await transferInto(
        osFiles.map((file) => ({ workspaceId: null, path: file.path, name: file.name, isDirectory: file.isDirectory })),
        target,
        true,
        { incremental: true },
      );
      await openSinglePasted(created);
      return;
    }
    const current = clipboard.value;
    if (!current?.items.length) return;
    const created = await transferInto(
      current.items.map((item) => ({
        workspaceId: item.workspaceId,
        path: item.path,
        name: item.path ? item.name : (host.workspaces().find((entry) => entry.id === item.workspaceId)?.name ?? item.name),
        isDirectory: item.isDirectory,
      })),
      target,
      !current.cut,
      { incremental: true },
    );
    if (current.cut && created.length) clipboard.value = null;
    await openSinglePasted(created);
  }

  async function openSinglePasted(created: ExplorerItem[]) {
    if (created.length === 1 && !created[0].isDirectory) {
      await host.openFile(created[0].workspaceId, created[0].path).catch(() => undefined);
    }
  }

  // ---- 文件树内拖放 ----
  const dragSources = shallowRef<ExplorerItem[]>([]);
  const dragOver = ref<{ key: string; workspaceId: string; path: string; copy: boolean; valid: boolean } | null>(null);
  let expandTimer = 0;
  let expandKey = "";

  function startDrag(item: ExplorerItem) {
    dragSources.value = isSelected(item.key) ? actionItems(item) : [item];
    if (!isSelected(item.key)) select(item);
  }

  function updateDragOver(target: ExplorerItem | null, copy: boolean) {
    if (!target) {
      dragOver.value = null;
      window.clearTimeout(expandTimer);
      expandKey = "";
      return;
    }
    const folder = folderTarget(target)!;
    const valid =
      settings.value.enableDragAndDrop &&
      canDropInExplorer(
        dragSources.value.map((item) => ({ workspaceId: item.workspaceId, path: item.path, isDirectory: item.isDirectory, readonly: item.readonly })),
        { workspaceId: folder.workspaceId, path: folder.path },
        copy,
      );
    dragOver.value = { key: explorerItemKey(folder.workspaceId, folder.path), workspaceId: folder.workspaceId, path: folder.path, copy, valid };
    if (target.isDirectory && target.path && target.key !== expandKey) {
      window.clearTimeout(expandTimer);
      expandKey = target.key;
      expandTimer = window.setTimeout(() => {
        void host.expandFolder(target.workspaceId, target.path);
      }, 500);
    } else if (!target.isDirectory) {
      window.clearTimeout(expandTimer);
      expandKey = "";
    }
  }

  function endDrag() {
    window.clearTimeout(expandTimer);
    expandKey = "";
    dragOver.value = null;
    dragSources.value = [];
  }

  async function dropOnTarget(target: ExplorerItem, copy: boolean) {
    const sources = dragSources.value;
    endDrag();
    if (!settings.value.enableDragAndDrop) return;
    const folder = folderTarget(target);
    if (!folder) return;
    const plain = sources.map((item) => ({ workspaceId: item.workspaceId, path: item.path, isDirectory: item.isDirectory, readonly: item.readonly }));
    if (!canDropInExplorer(plain, { workspaceId: folder.workspaceId, path: folder.path }, copy)) return;
    if (host.preview()) {
      host.reportError(t("explorer.dnd.previewMoveBlocked"));
      return;
    }
    const distinct = distinctParents(sources);
    if (!copy && settings.value.confirmDragAndDrop) {
      const text = moveConfirmMessage(distinct, folder.name);
      const answer = await confirm({
        message: text.message,
        detail: text.detail,
        primary: t("explorer.confirm.move"),
        checkbox: t("explorer.confirm.dontAskAgain"),
      });
      if (!answer.confirmed) return;
      if (answer.checked) void updateSettings({ confirmDragAndDrop: false });
    }
    await transferInto(
      distinct.map((item) => ({ workspaceId: item.workspaceId, path: item.path, name: item.name, isDirectory: item.isDirectory })),
      folder,
      copy,
      { incremental: true },
    );
  }

  // ---- 从系统拖入 ----
  async function importDropped(items: DroppedItem[], anchor: ExplorerItem | null) {
    const target = folderTarget(anchor);
    if (!target) {
      host.reportError(t("explorer.import.needWorkspace"));
      return;
    }
    if (host.preview()) {
      host.reportError(t("explorer.import.previewBlocked"));
      return;
    }
    const native = items.filter((item) => item.nativePath);
    const read = items.filter((item) => !item.nativePath);
    const created: ExplorerItem[] = [];
    if (native.length) {
      created.push(
        ...(await transferInto(
          native.map((item) => ({ workspaceId: null, path: item.nativePath, name: item.name, isDirectory: item.isDirectory })),
          target,
          true,
          { incremental: false },
        )),
      );
    }
    if (read.length) {
      await host.reloadLevel(target.workspaceId, target.path).catch(() => undefined);
      const taken = new Set((host.levelEntries(target.workspaceId, target.path) ?? []).map((entry) => entry.name));
      const written: string[] = [];
      let replaced = false;
      const errors: string[] = [];
      for (const item of read) {
        let overwrite = false;
        if (taken.has(item.name)) {
          const answer = await confirm({
            type: "warning",
            message: t("explorer.transfer.replaceConfirm", { name: item.name }),
            detail: t("explorer.transfer.irreversible"),
            primary: t("explorer.confirm.replace"),
            danger: true,
          });
          if (!answer.confirmed) continue;
          overwrite = true;
          replaced = true;
        }
        try {
          const files = await readDroppedTree(item);
          for (const file of files) {
            const path = joinPath(target.path, file.relativePath);
            if (file.isDirectory) {
              await explorerBackend.createEntry(target.workspaceId, path, true).catch((cause) => {
                if (!messageOf(cause).startsWith(explorerBackend.TARGET_EXISTS_PREFIX)) throw cause;
              });
            } else if (file.file) {
              await explorerBackend.writeFileBytes(target.workspaceId, path, new Uint8Array(await file.file.arrayBuffer()), overwrite);
            }
          }
          const path = joinPath(target.path, item.name);
          written.push(path);
          const entry: ExplorerItem = {
            key: explorerItemKey(target.workspaceId, path),
            workspaceId: target.workspaceId,
            path,
            name: item.name,
            isDirectory: item.isDirectory,
          };
          remember(entry);
          created.push(entry);
        } catch (cause) {
          errors.push(
            t("explorer.error.itemWithCause", {
              name: item.name,
              message: messageOf(cause).replace(explorerBackend.TARGET_EXISTS_PREFIX, t("explorer.error.existsInline")),
            }),
          );
        }
      }
      if (errors.length) host.reportError(errors.join("\n"));
      if (written.length) {
        if (!replaced) {
          const importLabel =
            written.length === 1
              ? t("explorer.op.import", { name: baseName(written[0]) })
              : t("explorer.op.importMany", { count: written.length });
          pushUndo(createdUndo(importLabel, target.workspaceId, written));
        }
        await host.reloadLevel(target.workspaceId, target.path).catch(() => undefined);
        host.afterMutation();
        await host.expandFolder(target.workspaceId, target.path).catch(() => undefined);
      }
    }
    if (!created.length) return;
    selection.value = { selected: created.map((item) => item.key), focus: created[0].key, anchor: created[0].key };
    if (settings.value.autoOpenDroppedFile && created.length === 1 && !created[0].isDirectory) {
      await host.openFile(created[0].workspaceId, created[0].path).catch(() => undefined);
    }
  }

  // ---- 复制路径 ----
  async function copyPaths(items: ExplorerItem[], relative: boolean) {
    if (!items.length) return;
    const separator = relative ? settings.value.copyRelativePathSeparator : settings.value.copyPathSeparator;
    const lines = items.map((item) => {
      if (relative) {
        const value = item.path || (host.workspaces().find((entry) => entry.id === item.workspaceId)?.name ?? "");
        return applySeparator(value, separator, isWindows);
      }
      return applySeparator(host.absolutePath(item.workspaceId, item.path), separator, isWindows);
    });
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
    } catch {
      host.reportError(t("explorer.clipboard.failed"));
    }
  }

  // ---- .gitignore ----
  // 原地修改后 triggerRef，不再每次 flush 克隆整个集合并替换深响应式对象。
  const ignored = shallowRef(new Map<string, Set<string>>());
  /** isIgnored 的结果缓存（含祖先回溯），集合变化时按工作区整体失效。 */
  const ignoredLookup = new Map<string, Map<string, boolean>>();
  const checked = new Map<string, Set<string>>();
  const pendingIgnore = new Map<string, Set<string>>();
  let ignoreTimer = 0;

  function noteEntries(workspaceId: string, entries: Array<{ path: string }>) {
    if (host.preview() || !host.isGitWorkspace(workspaceId)) return;
    const done = checked.get(workspaceId) ?? new Set<string>();
    checked.set(workspaceId, done);
    const queue = pendingIgnore.get(workspaceId) ?? new Set<string>();
    for (const entry of entries) {
      if (done.has(entry.path)) continue;
      done.add(entry.path);
      queue.add(entry.path);
    }
    if (!queue.size) return;
    pendingIgnore.set(workspaceId, queue);
    window.clearTimeout(ignoreTimer);
    ignoreTimer = window.setTimeout(flushIgnore, 60);
  }

  async function flushIgnore() {
    const batches = [...pendingIgnore.entries()];
    pendingIgnore.clear();
    for (const [workspaceId, paths] of batches) {
      try {
        const hits = await explorerBackend.gitIgnoredPaths(workspaceId, [...paths]);
        const current = ignored.value.get(workspaceId) ?? new Set<string>();
        for (const path of paths) current.delete(path);
        for (const hit of hits) current.add(hit);
        ignored.value.set(workspaceId, current);
        ignoredLookup.delete(workspaceId);
        triggerRef(ignored);
      } catch {
        /* 不是 Git 仓库或 git 不可用 */
      }
    }
  }

  function resetIgnored(workspaceId: string) {
    checked.delete(workspaceId);
    noteEntries(workspaceId, host.loadedPaths(workspaceId).map((path) => ({ path })));
  }

  function isIgnored(workspaceId: string, path: string): boolean {
    const set = ignored.value.get(workspaceId);
    if (!set?.size || !path) return false;
    const cache = ignoredLookup.get(workspaceId) ?? new Map<string, boolean>();
    ignoredLookup.set(workspaceId, cache);
    const known = cache.get(path);
    if (known !== undefined) return known;
    const result = set.has(path) || isIgnored(workspaceId, parentOf(path));
    cache.set(path, result);
    return result;
  }

  // ---- 问题标记（Monaco markers） ----
  const problems = shallowRef(new Map<string, { errors: number; warnings: number }>());
  let problemTimer = 0;

  function recomputeProblems() {
    const next = new Map<string, { errors: number; warnings: number }>();
    const add = (key: string, error: boolean) => {
      const entry = next.get(key) ?? { errors: 0, warnings: 0 };
      if (error) entry.errors += 1;
      else entry.warnings += 1;
      next.set(key, entry);
    };
    for (const marker of monaco.editor.getModelMarkers({})) {
      const error = marker.severity === monaco.MarkerSeverity.Error;
      if (!error && marker.severity !== monaco.MarkerSeverity.Warning) continue;
      const full = decodeURIComponent(marker.resource.path).replace(/^\/+/, "");
      const slash = full.indexOf("/");
      if (slash <= 0) continue;
      const workspaceId = full.slice(0, slash);
      let path = full.slice(slash + 1);
      add(explorerItemKey(workspaceId, path), error);
      // 文件夹只标颜色，计数用于悬停提示。
      while (path) {
        path = parentOf(path);
        add(`${explorerItemKey(workspaceId, path)}\u0000dir`, error);
      }
    }
    problems.value = next;
  }

  const markerListener = monaco.editor.onDidChangeMarkers(() => {
    window.clearTimeout(problemTimer);
    problemTimer = window.setTimeout(recomputeProblems, 150);
  });
  recomputeProblems();

  function problemOf(workspaceId: string, path: string, isDirectory: boolean) {
    const key = explorerItemKey(workspaceId, path);
    return problems.value.get(isDirectory ? `${key}\u0000dir` : key) ?? null;
  }

  // ---- 文件监听 ----
  const owner = `explorer-${Math.random().toString(36).slice(2)}`;
  const pendingChanges = new Map<string, { dirs: Set<string>; overflow: boolean; git: boolean; gitignore: boolean }>();
  const watcherExcludes = computed(() =>
    Object.entries(settings.value.watcherExclude)
      .filter(([, enabled]) => enabled === true)
      .map(([pattern]) => pattern)
      .sort(),
  );
  let changeTimer = 0;
  let gitTimer = 0;
  let flushingChanges = false;
  let unlisten: (() => void) | null = null;
  let disposed = false;

  function scheduleChangeFlush() {
    window.clearTimeout(changeTimer);
    changeTimer = window.setTimeout(() => {
      changeTimer = 0;
      void flushChanges();
    }, 500);
  }

  void explorerBackend
    .onFsChanged((change) => {
      const entry = pendingChanges.get(change.workspaceId) ?? { dirs: new Set<string>(), overflow: false, git: false, gitignore: false };
      entry.overflow ||= change.overflow;
      entry.git ||= change.git;
      for (const path of change.paths) {
        entry.dirs.add(parentOf(path));
        if (baseName(path) === ".gitignore") entry.gitignore = true;
      }
      pendingChanges.set(change.workspaceId, entry);
      scheduleChangeFlush();
    })
    .then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });

  async function flushChanges() {
    if (flushingChanges) return;
    flushingChanges = true;
    const batches = [...pendingChanges.entries()];
    pendingChanges.clear();
    let gitDirty = false;
    try {
      for (const [workspaceId, change] of batches) {
        if (!host.workspaces().some((workspace) => workspace.id === workspaceId)) continue;
        if (change.overflow) {
          await host.reloadWorkspace(workspaceId).catch(() => undefined);
        } else {
          for (const dir of change.dirs) {
            if (host.levelEntries(workspaceId, dir)) await host.reloadLevel(workspaceId, dir).catch(() => undefined);
          }
        }
        if (change.gitignore || change.git || change.overflow) resetIgnored(workspaceId);
        gitDirty ||= change.git || change.dirs.size > 0 || change.overflow;
      }
      if (gitDirty) {
        window.clearTimeout(gitTimer);
        gitTimer = window.setTimeout(() => host.gitChanged(), 1_000);
      }
    } finally {
      flushingChanges = false;
      if (pendingChanges.size) scheduleChangeFlush();
    }
  }

  watch(
    [
      () => (host.preview() ? "" : host.workspaces().map((workspace) => workspace.id).join("\0")),
      () => watcherExcludes.value.join("\0"),
    ],
    ([ids, excludes]) => {
      void explorerBackend.watchWorkspaces(
        owner,
        ids ? ids.split("\0") : [],
        excludes ? excludes.split("\0") : [],
      ).catch(() => undefined);
    },
    { immediate: true },
  );

  onBeforeUnmount(() => {
    disposed = true;
    unlisten?.();
    markerListener.dispose();
    window.clearTimeout(changeTimer);
    window.clearTimeout(gitTimer);
    window.clearTimeout(problemTimer);
    window.clearTimeout(ignoreTimer);
    window.clearTimeout(expandTimer);
    window.clearTimeout(typeTimer);
    void explorerBackend.watchWorkspaces(owner, [], watcherExcludes.value).catch(() => undefined);
    dialog.value?.resolve({ confirmed: false, checked: false });
  });

  return {
    isMac,
    isWindows,
    settings,
    updateSettings,
    dialog,
    confirm,
    resolveDialog,
    selection,
    selectedItems,
    focusedItem,
    isSelected,
    isFocused,
    select,
    clearSelection,
    clickSelect,
    contextSelect,
    moveSelection,
    moveFocusOnly,
    toggleFocused,
    focusKey,
    selectAllVisible,
    actionItems,
    remember,
    typeAhead,
    findOpen,
    findQuery,
    findMode,
    closeFind,
    compareSource,
    clipboard,
    cutKeys,
    setClipboard,
    cancelCut,
    canUndo,
    canRedo,
    undo,
    redo,
    recordCreate,
    recordRename,
    deleteItems,
    paste,
    folderTarget,
    dragSources,
    dragOver,
    startDrag,
    updateDragOver,
    endDrag,
    dropOnTarget,
    importDropped,
    copyPaths,
    revealEntry: host.revealEntry,
    noteEntries,
    isIgnored,
    problemOf,
  };
}

export type ExplorerController = ReturnType<typeof useExplorer>;
