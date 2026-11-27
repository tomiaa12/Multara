import { onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from "vue";
import { onBeforeRouteLeave, onBeforeRouteUpdate } from "vue-router";
import type * as Monaco from "monaco-editor";
import { monaco } from "../../monaco";
import type { EditorSelectionContext } from "../../domain/types";
import { isOversizedInlineSelection } from "../../components/chat/composer-content";
import {
  askUnsaved,
  registerUnsavedEditor,
  type UnsavedEditorOwner,
} from "../../components/workbench/unsaved-editor";
import { useAppStore } from "../../stores/app";
import * as backend from "../../services/backend";
import { t } from "../../i18n";

export type InboxEditableFile = {
  workspaceId: string;
  path: string;
  content: string;
  original: string;
  readOnly: boolean;
};

export function useInboxUnsavedEditors(owner: UnsavedEditorOwner) {
  let unregister: (() => void) | undefined;
  function activate() {
    unregister?.();
    unregister = registerUnsavedEditor(owner);
  }
  function deactivate() {
    unregister?.();
    unregister = undefined;
  }
  onMounted(activate);
  onActivated(activate);
  onDeactivated(deactivate);
  onBeforeUnmount(deactivate);
  onBeforeRouteLeave(() => owner.leave());
  onBeforeRouteUpdate(() => owner.leave());
}

export function useInboxEditor(options: {
  files: () => InboxEditableFile[];
  sessionId: () => string;
  active: () => boolean;
  attach: (context: EditorSelectionContext) => void;
  saved?: () => void;
  manageUnsaved?: boolean;
}) {
  const store = useAppStore();
  const error = ref("");
  const saves = new Map<InboxEditableFile, Promise<boolean>>();
  const bindings = new Map<Monaco.editor.IStandaloneCodeEditor, {
    file: () => InboxEditableFile | null;
    disposables: Array<{ dispose: () => void }>;
  }>();
  let autoSaveTimer = 0;

  function dirty(file: InboxEditableFile) {
    return !file.readOnly && Boolean(file.workspaceId) && file.content !== file.original;
  }

  function dirtyFiles() {
    return options.files().filter(dirty);
  }

  async function save(file: InboxEditableFile): Promise<boolean> {
    const pending = saves.get(file);
    if (pending) {
      if (!(await pending)) return false;
      return save(file);
    }
    if (!dirty(file)) return true;
    const content = file.content;
    const saving = (async () => {
      try {
        await backend.writeWorkspaceFile(file.workspaceId, file.path, content);
        file.original = content;
        error.value = "";
        options.saved?.();
        return true;
      } catch (cause) {
        error.value = cause instanceof Error ? cause.message : String(cause);
        return false;
      } finally {
        saves.delete(file);
      }
    })();
    saves.set(file, saving);
    return saving;
  }

  async function confirmFiles(files = options.files()): Promise<boolean> {
    const changed = files.filter(dirty);
    if (!changed.length) return true;
    const action = await askUnsaved(changed.length === 1
      ? t("workbench.unsavedSaveOne", { name: changed[0]!.path.split(/[\\/]/).pop() || "" })
      : t("workbench.unsavedSaveMany", { count: changed.length }));
    if (action === "cancel") return false;
    for (const file of changed) {
      if (action === "save") {
        if (!(await save(file))) return false;
      } else {
        const pending = saves.get(file);
        if (pending) await pending;
        file.content = file.original;
      }
    }
    return true;
  }

  function change(file: InboxEditableFile | null, content: string, fromUser: boolean) {
    if (file && !file.readOnly && fromUser) file.content = content;
  }

  function scheduleAutoSave() {
    if (!options.active() || store.data?.settings.autoSaveOnFocusLoss !== "true") return;
    window.clearTimeout(autoSaveTimer);
    autoSaveTimer = window.setTimeout(() => {
      if (!options.active()) return;
      void (async () => {
        for (const file of dirtyFiles()) {
          if (!(await save(file))) break;
        }
      })();
    }, 0);
  }

  function attach(editor: Monaco.editor.IStandaloneCodeEditor, file: InboxEditableFile) {
    const model = editor.getModel();
    const selection = editor.getSelection();
    if (!model || !selection) return;
    const range = selection.isEmpty()
      ? new monaco.Range(selection.startLineNumber, 1, selection.startLineNumber, model.getLineMaxColumn(selection.startLineNumber))
      : selection;
    const text = model.getValueInRange(range);
    if (!text.trim()) {
      store.reportComposerNotice(t("workbench.emptyLineReference"), "", options.sessionId());
      return;
    }
    options.attach({
      id: crypto.randomUUID(),
      workspaceId: file.workspaceId,
      path: file.path,
      startLine: range.startLineNumber,
      endLine: range.endLineNumber,
      text: isOversizedInlineSelection(text, range.startLineNumber, range.endLineNumber) ? "" : text,
    });
  }

  function bind(editor: Monaco.editor.IStandaloneCodeEditor, file: () => InboxEditableFile | null) {
    bindings.get(editor)?.disposables.forEach((item) => item.dispose());
    const disposables = [editor.onDidBlurEditorWidget(scheduleAutoSave)];
    for (const [id, key, run] of [
      ["save", monaco.KeyCode.KeyS, (target: InboxEditableFile) => { void save(target); }],
      ["attach", monaco.KeyCode.KeyI, (target: InboxEditableFile) => attach(editor, target)],
      ["attach-line", monaco.KeyCode.KeyL, (target: InboxEditableFile) => attach(editor, target)],
    ] as const) {
      disposables.push(editor.addAction({
        id: `multara-inbox-${id}`,
        label: t(id === "save" ? "workbench.shortcut.save" : "workbench.shortcut.attach"),
        keybindings: [monaco.KeyMod.CtrlCmd | key],
        run: () => {
          const target = file();
          if (options.active() && target) run(target);
        },
      }));
    }
    disposables.push(editor.onDidDispose(() => {
      bindings.delete(editor);
      disposables.forEach((item) => item.dispose());
    }));
    bindings.set(editor, { file, disposables });
  }

  function keydown(event: KeyboardEvent) {
    if (!options.active() || event.repeat || event.isComposing || event.altKey || event.shiftKey) return;
    if (!(event.metaKey || event.ctrlKey)) return;
    const key = event.key.toLowerCase();
    if (!["s", "i", "l"].includes(key)) return;
    for (const [editor, binding] of bindings) {
      const host = editor.getDomNode();
      if (!editor.hasTextFocus() && !(host && event.target instanceof Node && host.contains(event.target))) continue;
      const file = binding.file();
      if (!file) continue;
      event.preventDefault();
      event.stopPropagation();
      if (key === "s") void save(file);
      else attach(editor, file);
      return;
    }
  }

  if (options.manageUnsaved !== false) {
    useInboxUnsavedEditors({
      leave: () => confirmFiles(),
      dirty: () => dirtyFiles().length > 0,
      paths: () => dirtyFiles().map((file) => file.path),
    });
  }
  onMounted(() => {
    window.addEventListener("blur", scheduleAutoSave);
    document.addEventListener("visibilitychange", scheduleAutoSave);
  });
  onDeactivated(() => window.clearTimeout(autoSaveTimer));
  onBeforeUnmount(() => {
    window.clearTimeout(autoSaveTimer);
    window.removeEventListener("blur", scheduleAutoSave);
    document.removeEventListener("visibilitychange", scheduleAutoSave);
    for (const binding of bindings.values()) binding.disposables.forEach((item) => item.dispose());
    bindings.clear();
  });

  return { error, dirty, dirtyFiles, save, confirmFiles, change, bind, keydown };
}
