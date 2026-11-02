<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { registerDiffBreadcrumbs } from "../../editor/diff-breadcrumbs";
import { monacoReadOnlyMessage } from "../../editor/monaco-nls";
import { monacoOverflowWidgetsRoot } from "../../editor/monaco-overflow-widgets";
import { applyMonacoTheme } from "../../editor/monaco-workspace";
import { languageFromFilePath } from "../../monaco-language";
import { monaco, monacoLanguageId } from "../../monaco";

const props = withDefaults(
  defineProps<{
    diffId: string;
    original: string;
    modified: string;
    language: string;
    theme: string;
    binary: boolean;
    /** 真实文件路径：写入 model URI，便于按扩展名加载与编辑器一致的语法高亮 */
    filePath?: string;
    /** MultiDiff：按内容高度撑开，随外层纵向滚动 */
    autoHeight?: boolean;
  }>(),
  { autoHeight: false },
);

const host = ref<HTMLElement | null>(null);
/** 等 hideUnchangedRegions 算完再露出，避免先全文再折叠 */
const ready = ref(!props.autoHeight);
let editor: monaco.editor.IStandaloneDiffEditor | null = null;
let originalModel: monaco.editor.ITextModel | null = null;
let modifiedModel: monaco.editor.ITextModel | null = null;
let sizeDisposable: { dispose: () => void } | null = null;
let mountGeneration = 0;

const SIDE_BY_SIDE_BREAKPOINT = 560;
const AUTO_HEIGHT_MIN = 80;

function nextFrame() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

function dispose() {
  sizeDisposable?.dispose();
  sizeDisposable = null;
  editor?.dispose();
  originalModel?.dispose();
  modifiedModel?.dispose();
  editor = null;
  originalModel = null;
  modifiedModel = null;
}

function syncAutoHeight() {
  if (!props.autoHeight || !editor || !host.value) return;
  const height = Math.max(
    editor.getOriginalEditor().getContentHeight(),
    editor.getModifiedEditor().getContentHeight(),
    AUTO_HEIGHT_MIN,
  );
  host.value.style.height = `${height}px`;
  editor.layout();
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

async function mount() {
  if (!host.value || props.binary) return;
  const generation = ++mountGeneration;
  ready.value = !props.autoHeight;
  registerDiffBreadcrumbs();
  const originalUri = modelUri("original");
  const modifiedUri = modelUri("modified");
  monaco.editor.getModel(originalUri)?.dispose();
  monaco.editor.getModel(modifiedUri)?.dispose();
  const language = resolveLanguage();
  originalModel = monaco.editor.createModel(props.original, language, originalUri);
  modifiedModel = monaco.editor.createModel(props.modified, language, modifiedUri);
  applyMonacoTheme();
  if (props.autoHeight) host.value.style.visibility = "hidden";
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
    // MultiDiff 外层滚动：隐藏编辑器滚动条并放行滚轮
    renderOverviewRuler: !props.autoHeight,
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
    ...(props.autoHeight
      ? {
          scrollbar: {
            vertical: "hidden" as const,
            horizontal: "hidden" as const,
            handleMouseWheel: false,
            alwaysConsumeMouseWheel: false,
            useShadows: false,
          },
        }
      : {}),
    // DiffEditor 强制 fixed tip；挂到 body 避开 .vs-scroll 的 transform 包含块
    overflowWidgetsDomNode: monacoOverflowWidgetsRoot(),
  });
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
  if (props.autoHeight) {
    // 先算 diff/折叠区，再 setModel，避免先全文再折叠
    const viewModel = editor.createViewModel({
      original: originalModel,
      modified: modifiedModel,
    });
    try {
      await viewModel.waitForDiff();
    } catch {
      viewModel.dispose();
      return;
    }
    if (generation !== mountGeneration) {
      viewModel.dispose();
      return;
    }
    editor.setModel(viewModel);
    await nextFrame();
    if (generation !== mountGeneration) return;
    const original = editor.getOriginalEditor();
    const modified = editor.getModifiedEditor();
    const originalDisposable = original.onDidContentSizeChange(syncAutoHeight);
    const modifiedDisposable = modified.onDidContentSizeChange(syncAutoHeight);
    sizeDisposable = {
      dispose() {
        originalDisposable.dispose();
        modifiedDisposable.dispose();
      },
    };
    await nextTick();
    syncAutoHeight();
    ready.value = true;
    if (host.value) host.value.style.visibility = "";
    await nextTick();
    syncAutoHeight();
  } else {
    editor.setModel({ original: originalModel, modified: modifiedModel });
  }
}

onMounted(() => {
  void mount();
});
onBeforeUnmount(() => {
  mountGeneration += 1;
  dispose();
});

watch(
  () => [props.diffId, props.original, props.modified, props.language, props.binary, props.filePath],
  () => {
    dispose();
    void mount();
  },
);

watch(
  () => props.theme,
  (theme) => monaco.editor.setTheme(theme),
);
</script>

<template>
  <p v-if="binary" class="git-diff-binary">二进制文件，无法比较内容。</p>
  <div
    v-else
    ref="host"
    class="monaco-host git-diff-editor"
    :class="{ 'is-diff-pending': autoHeight && !ready }"
  />
</template>
