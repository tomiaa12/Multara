<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { monaco, monacoLanguageId } from "../../monaco";

const props = defineProps<{
  diffId: string;
  original: string;
  modified: string;
  language: string;
  theme: string;
  binary: boolean;
}>();

const host = ref<HTMLElement | null>(null);
let editor: monaco.editor.IStandaloneDiffEditor | null = null;
let originalModel: monaco.editor.ITextModel | null = null;
let modifiedModel: monaco.editor.ITextModel | null = null;

function dispose() {
  editor?.dispose();
  originalModel?.dispose();
  modifiedModel?.dispose();
  editor = null;
  originalModel = null;
  modifiedModel = null;
}

function mount() {
  if (!host.value || props.binary) return;
  const originalUri = monaco.Uri.from({ scheme: "scm-diff", path: `/original/${props.diffId}` });
  const modifiedUri = monaco.Uri.from({ scheme: "scm-diff", path: `/modified/${props.diffId}` });
  monaco.editor.getModel(originalUri)?.dispose();
  monaco.editor.getModel(modifiedUri)?.dispose();
  const language = monacoLanguageId(props.language);
  originalModel = monaco.editor.createModel(props.original, language, originalUri);
  modifiedModel = monaco.editor.createModel(props.modified, language, modifiedUri);
  editor = monaco.editor.createDiffEditor(host.value, {
    readOnly: true,
    originalEditable: false,
    automaticLayout: true,
    renderSideBySide: host.value.clientWidth >= 560,
    theme: props.theme,
    fontSize: 13,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
  });
  editor.setModel({ original: originalModel, modified: modifiedModel });
}

onMounted(mount);
onBeforeUnmount(dispose);

watch(
  () => [props.diffId, props.original, props.modified, props.language, props.binary],
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
