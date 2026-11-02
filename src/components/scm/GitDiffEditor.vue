<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { registerDiffBreadcrumbs } from "../../editor/diff-breadcrumbs";
import { monacoReadOnlyMessage } from "../../editor/monaco-nls";
import { monacoOverflowWidgetsRoot } from "../../editor/monaco-overflow-widgets";
import { applyMonacoTheme } from "../../editor/monaco-workspace";
import { languageFromFilePath } from "../../monaco-language";
import { monaco, monacoLanguageId } from "../../monaco";

const props = defineProps<{
  diffId: string;
  original: string;
  modified: string;
  language: string;
  theme: string;
  binary: boolean;
  /** 真实文件路径：写入 model URI，便于按扩展名加载与编辑器一致的语法高亮 */
  filePath?: string;
}>();

const host = ref<HTMLElement | null>(null);
let editor: monaco.editor.IStandaloneDiffEditor | null = null;
let originalModel: monaco.editor.ITextModel | null = null;
let modifiedModel: monaco.editor.ITextModel | null = null;

const SIDE_BY_SIDE_BREAKPOINT = 560;

function dispose() {
  editor?.dispose();
  originalModel?.dispose();
  modifiedModel?.dispose();
  editor = null;
  originalModel = null;
  modifiedModel = null;
}

function modelUri(side: "original" | "modified") {
  const file = (props.filePath || "file.txt").replace(/^\/+/, "");
  return monaco.Uri.from({
    scheme: "scm-diff",
    path: `/${side}/${props.diffId}/${file}`,
  });
}

function resolveLanguage() {
  const fromProp = props.language?.trim();
  const candidate = fromProp && fromProp !== "plaintext"
    ? fromProp
    : languageFromFilePath(props.filePath || "");
  return monacoLanguageId(candidate);
}

function mount() {
  if (!host.value || props.binary) return;
  registerDiffBreadcrumbs();
  const originalUri = modelUri("original");
  const modifiedUri = modelUri("modified");
  monaco.editor.getModel(originalUri)?.dispose();
  monaco.editor.getModel(modifiedUri)?.dispose();
  const language = resolveLanguage();
  originalModel = monaco.editor.createModel(props.original, language, originalUri);
  modifiedModel = monaco.editor.createModel(props.modified, language, modifiedUri);
  applyMonacoTheme();
  editor = monaco.editor.createDiffEditor(host.value, {
    readOnly: true,
    readOnlyMessage: monacoReadOnlyMessage(),
    originalEditable: false,
    automaticLayout: true,
    renderSideBySide: true,
    useInlineViewWhenSpaceIsLimited: true,
    renderSideBySideInlineBreakpoint: SIDE_BY_SIDE_BREAKPOINT,
    enableSplitViewResizing: true,
    renderMarginRevertIcon: false,
    renderGutterMenu: false,
    renderIndicators: true,
    renderOverviewRuler: true,
    ignoreTrimWhitespace: true,
    hideUnchangedRegions: {
      enabled: true,
      contextLineCount: 3,
      minimumLineCount: 3,
      revealLineCount: 20,
    },
    theme: props.theme,
    fontSize: 13,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    // DiffEditor 强制 fixed tip；挂到 body 避开 .vs-scroll 的 transform 包含块
    overflowWidgetsDomNode: monacoOverflowWidgetsRoot(),
  });
  editor.setModel({ original: originalModel, modified: modifiedModel });
  // 与普通编辑器对齐：开启语义高亮（语言支持时才生效）
  const semantic = { "semanticHighlighting.enabled": true as const };
  editor.getOriginalEditor().updateOptions(semantic);
  editor.getModifiedEditor().updateOptions(semantic);
  monaco.editor.setTheme(props.theme);
  if (originalModel.getLanguageId() !== language) {
    monaco.editor.setModelLanguage(originalModel, language);
  }
  if (modifiedModel.getLanguageId() !== language) {
    monaco.editor.setModelLanguage(modifiedModel, language);
  }
}

onMounted(mount);
onBeforeUnmount(dispose);

watch(
  () => [props.diffId, props.original, props.modified, props.language, props.binary, props.filePath],
  () => {
    dispose();
    mount();
  },
);

watch(
  () => props.theme,
  (theme) => monaco.editor.setTheme(theme),
);
</script>

<template>
  <p v-if="binary" class="git-diff-binary">二进制文件，无法比较内容。</p>
  <div v-else ref="host" class="monaco-host git-diff-editor" />
</template>
