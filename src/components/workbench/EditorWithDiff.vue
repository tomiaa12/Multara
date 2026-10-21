<script setup lang="ts">
import { computed, onUnmounted, shallowRef, watch } from "vue";
import { loader, VueMonacoEditor } from "@guolao/vue-monaco-editor";
import type * as MonacoNS from "monaco-editor";
import { monaco, monacoLanguageId } from "../../monaco";
import { currentDocumentTheme, monacoThemeName } from "../../editor/monaco-workspace";
import { DiffOverlayManager, groupAnchorLine, groupConsecutiveChunks } from "./DiffOverlayManager";
import { MergeConflictEditor } from "./merge-conflict-editor";
import { GitBlame } from "./git-blame-editor";
import { GitGutter } from "./git-gutter";
import { gitBaselineText } from "./git-gutter-marks";
import { monacoZoomOptions } from "./monaco-zoom";
import { useDiffStore } from "../../stores/diff";
import { useEditorStatusStore } from "../../stores/editor-status";
import * as backend from "../../services/backend";

loader.config({ monaco });

const props = defineProps<{
  filePath: string;
  workspaceId: string;
  content: string;
  language?: string;
  readOnly?: boolean;
  theme?: string;
  tabSize?: number;
  insertSpaces?: boolean;
  stickyScroll?: boolean;
  gitEpoch?: number;
  writing?: boolean;
}>();

const emit = defineEmits<{
  change: [content: string, fromUser: boolean];
  mount: [editor: MonacoNS.editor.IStandaloneCodeEditor];
  "pending-chunks": [count: number];
  "diff-groups": [count: number];
}>();

const diffStore = useDiffStore();
const editorStatus = useEditorStatusStore();
const overlayRef = shallowRef<DiffOverlayManager | null>(null);
const gutterRef = shallowRef<GitGutter | null>(null);
const blameRef = shallowRef<GitBlame | null>(null);
const conflictRef = shallowRef<MergeConflictEditor | null>(null);
const editorRef = shallowRef<MonacoNS.editor.IStandaloneCodeEditor | null>(null);
const editorDisposables: Array<{ dispose: () => void }> = [];
let applyingExternal = false;

function replaceEditorValue(editor: MonacoNS.editor.IStandaloneCodeEditor, next: string) {
  if (editor.getValue() === next) return;
  applyingExternal = true;
  try {
    editor.setValue(next);
  } finally {
    applyingExternal = false;
  }
}
const isRunning = computed(() => props.writing === true);
const editorOptions = computed(() => ({
  scrollBeyondLastLine: false,
  lineNumbers: "on" as const,
  lineNumbersMinChars: 3,
  lineDecorationsWidth: 10,
  glyphMargin: false,
  readOnly: Boolean(props.readOnly || props.writing),
  minimap: { enabled: false },
  automaticLayout: true,
  tabSize: props.tabSize ?? 4,
  insertSpaces: props.insertSpaces ?? true,
  detectIndentation: false,
  dropIntoEditor: { enabled: false },
  stickyScroll: { enabled: props.stickyScroll ?? true },
  ...monacoZoomOptions,
}));

function onEditorMount(editor: MonacoNS.editor.IStandaloneCodeEditor) {
  editorDisposables.splice(0).forEach((item) => item.dispose());
  overlayRef.value?.dispose();
  gutterRef.value?.dispose();
  blameRef.value?.dispose();
  conflictRef.value?.dispose();
  editorRef.value = editor;
  overlayRef.value = new DiffOverlayManager(
    editor,
    (chunkId) => diffStore.acceptChunk(chunkId, editor.getValue()),
    (chunkId) => diffStore.undoChunk(chunkId, props.workspaceId, props.filePath, editor.getValue()),
    () => isRunning.value,
  );
  const locate = () => ({ workspaceId: props.workspaceId, filePath: props.filePath });
  gutterRef.value = new GitGutter(
    editor,
    async () => gitBaselineText(await backend.gitHeadFile(props.workspaceId, props.filePath)),
    () => `${props.workspaceId}\0${props.filePath}\0${props.gitEpoch ?? 0}`,
    locate,
  );
  blameRef.value = new GitBlame(editor, locate, (label, title) => editorStatus.setBlame(label, title));
  conflictRef.value = new MergeConflictEditor(editor, () => Boolean(props.readOnly || props.writing));
  gutterRef.value.schedule(true);

  editorDisposables.push(
    editor.onDidChangeModelContent((event) => {
      if (applyingExternal) return;
      const current = editor.getValue();
      if (current !== props.content) emit("change", current, !event.isFlush);
      refreshDiff(current);
      gutterRef.value?.schedule();
    }),
    editor.onDidChangeModel(() => {
      refreshDiff(editor.getValue());
      gutterRef.value?.schedule(true);
    }),
  );

  refreshDiff(props.content);
  emit("mount", editor);
}

watch(
  () => props.language,
  (language) => {
    const model = editorRef.value?.getModel();
    if (!model || !language) return;
    const id = monacoLanguageId(language);
    if (model.getLanguageId() !== id) {
      monaco.editor.setModelLanguage(model, id);
    }
  },
);

watch(
  () => [props.tabSize, props.insertSpaces, props.stickyScroll, props.theme] as const,
  ([tabSize, insertSpaces, stickyScroll, theme]) => {
    editorRef.value?.updateOptions({
      tabSize: tabSize ?? 4,
      insertSpaces: insertSpaces ?? true,
      stickyScroll: { enabled: stickyScroll ?? true },
    });
    if (theme) monaco.editor.setTheme(theme);
  },
);

function refreshDiff(currentContent: string) {
  const chunks = diffStore.getChunksForFile(
    props.workspaceId,
    props.filePath,
    currentContent,
  );
  overlayRef.value?.applyChunks(chunks);
  if (overlayRef.value) {
    emit("pending-chunks", chunks.length);
    emit("diff-groups", groupConsecutiveChunks(chunks).length);
  }
}

function revealDiffGroup(index: number) {
  const editor = editorRef.value;
  const groups = groupConsecutiveChunks(overlayRef.value?.pendingChunks() ?? []);
  const group = groups[index];
  if (!editor || !group) return;
  const line = groupAnchorLine(group);
  const model = editor.getModel();
  const clamped = Math.max(1, Math.min(line, model?.getLineCount() ?? line));
  editor.revealLineInCenter(clamped);
  editor.setPosition({ lineNumber: clamped, column: 1 });
}

async function acceptAll() {
  if (isRunning.value) return;
  const content = editorRef.value?.getValue() ?? props.content;
  await diffStore.acceptAllChunks(props.workspaceId, props.filePath, content);
  refreshDiff(editorRef.value?.getValue() ?? content);
}

async function undoAll() {
  if (isRunning.value) return;
  await diffStore.undoAllChunks(props.workspaceId, props.filePath);
}

watch(
  () => [diffStore.chunkStates, diffStore.baselines],
  () => {
    const editor = editorRef.value;
    if (editor) refreshDiff(editor.getValue());
  },
  { deep: true },
);

watch(
  () => [props.filePath, props.workspaceId, diffStore.diskEpoch] as const,
  () => {
    const editor = editorRef.value;
    if (editor) replaceEditorValue(editor, props.content);
    refreshDiff(editor?.getValue() ?? props.content);
  },
  { flush: "post" },
);

watch(
  () => [props.filePath, props.workspaceId, props.gitEpoch] as const,
  () => gutterRef.value?.schedule(true),
);

watch(isRunning, () => {
  const editor = editorRef.value;
  if (editor) refreshDiff(editor.getValue());
});

watch(
  () => [props.readOnly, props.writing] as const,
  ([readOnly, writing]) => {
    editorRef.value?.updateOptions({ readOnly: Boolean(readOnly || writing) });
    conflictRef.value?.refresh();
  },
);

watch(
  () => [
    props.content,
    [...diffStore.snapshots.keys()].join("|"),
    [...diffStore.turnOrder.values()].map((ids) => ids.join("|")).join(";"),
  ],
  () => {
    const editor = editorRef.value;
    refreshDiff(editor?.getValue() ?? props.content);
  },
  { flush: "post" },
);

onUnmounted(() => {
  editorDisposables.splice(0).forEach((item) => item.dispose());
  overlayRef.value?.dispose();
  gutterRef.value?.dispose();
  blameRef.value?.dispose();
  conflictRef.value?.dispose();
});

defineExpose({
  getValue: () => editorRef.value?.getValue(),
  getEditor: () => editorRef.value,
  layout: () => editorRef.value?.layout(),
  reveal: (start: number, end?: number, column?: number, endColumn?: number) => {
    const editor = editorRef.value;
    const model = editor?.getModel();
    if (!editor || !model) return;
    const startLine = Math.max(1, Math.min(start, model.getLineCount()));
    const endLine = Math.max(startLine, Math.min(end ?? start, model.getLineCount()));
    const range = column
      ? new monaco.Range(startLine, column, endLine, endColumn ?? column)
      : new monaco.Range(startLine, 1, endLine, model.getLineMaxColumn(endLine));
    editor.revealRangeInCenter(range);
    editor.setSelection(range);
    editor.focus();
  },
  ready: () => Boolean(editorRef.value),
  acceptAll,
  undoAll,
  revealDiffGroup,
});
</script>

<template>
  <div class="monaco-host">
    <VueMonacoEditor
      class="editor-with-diff"
      :value="content"
      :language="monacoLanguageId(language ?? 'plaintext')"
      :path="`/${workspaceId}/${filePath}`"
      :theme="theme ?? monacoThemeName(currentDocumentTheme())"
      :options="editorOptions"
      width="100%"
      height="100%"
      @mount="onEditorMount"
    />
  </div>
</template>
