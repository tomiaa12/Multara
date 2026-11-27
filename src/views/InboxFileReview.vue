<script setup lang="ts">
import { computed, onActivated, onDeactivated, ref, watch } from "vue";
import { vMotion } from "../ui/motion";
import { X } from "@lucide/vue";
import EditorWithDiff from "../components/workbench/EditorWithDiff.vue";
import MarkdownPreview from "../components/workbench/MarkdownPreview.vue";
import MarkdownSourceSwitch from "../components/workbench/MarkdownSourceSwitch.vue";
import { isMarkdownFile } from "../components/workbench/markdown-source";
import FileTypeIcon from "../components/FileTypeIcon.vue";
import { fileName } from "../components/chat/timeline-display";
import { detectIndent } from "../editor/eol";
import { currentDocumentTheme, monacoThemeName } from "../editor/monaco-workspace";
import type { EditorSelectionContext, FileContent } from "../domain/types";
import { useInboxEditor } from "./inbox/use-inbox-editor";
import { useAppStore } from "../stores/app";
import { useDiffStore } from "../stores/diff";
import { useI18n } from "../i18n";
import * as backend from "../services/backend";

const props = withDefaults(defineProps<{
  workspaceId: string;
  filePath: string;
  sessionId: string;
  manageUnsaved?: boolean;
}>(), { manageUnsaved: true });

const emit = defineEmits<{
  close: [];
  addToChat: [context: EditorSelectionContext];
}>();

const store = useAppStore();
const diffStore = useDiffStore();
const { t } = useI18n();
const editor = ref<InstanceType<typeof EditorWithDiff> | null>(null);
const file = ref<(FileContent & { original: string }) | null>(null);
const active = ref(true);
const fileEditor = useInboxEditor({
  files: () => file.value ? [file.value] : [],
  sessionId: () => props.sessionId,
  active: () => active.value,
  attach: (context) => emit("addToChat", context),
  manageUnsaved: props.manageUnsaved,
});
const editorError = fileEditor.error;
onActivated(() => { active.value = true; });
onDeactivated(() => { active.value = false; });
const pendingChunks = ref(0);
const busy = ref(false);

const title = computed(() => fileName(props.filePath) || props.filePath);
const indent = computed(() => detectIndent(file.value?.content ?? ""));
const stickyScroll = computed(() => store.data?.settings.stickyScroll !== "false");
const wordWrap = computed(() => store.data?.settings.wordWrap === "true");
const theme = monacoThemeName(currentDocumentTheme());
const markdownMode = ref<"preview" | "source">("source");

const markdownFile = computed(() => Boolean(file.value && isMarkdownFile(file.value)));

watch(
  () => [props.workspaceId, props.filePath] as const,
  () => {
    markdownMode.value = "source";
  },
);

let loadVersion = 0;
async function load() {
  const version = ++loadVersion;
  try {
    const loaded = await backend.readWorkspaceFile(props.workspaceId, props.filePath);
    if (version !== loadVersion || (file.value && fileEditor.dirty(file.value))) return;
    file.value = { ...loaded, original: loaded.content };
  } catch {
    if (version !== loadVersion || (file.value && fileEditor.dirty(file.value))) return;
    file.value = {
      workspaceId: props.workspaceId,
      path: props.filePath,
      content: "",
      original: "",
      language: "plaintext",
      size: 0,
      readOnly: true,
    };
  }
}

watch(
  () => [props.workspaceId, props.filePath, diffStore.diskEpoch] as const,
  () => {
    void load();
  },
  { immediate: true },
);

async function keep() {
  if (busy.value || !pendingChunks.value) return;
  busy.value = true;
  try {
    await editor.value?.acceptAll();
  } finally {
    busy.value = false;
  }
}

async function undo() {
  if (busy.value || !pendingChunks.value || !(await fileEditor.confirmFiles())) return;
  busy.value = true;
  try {
    await editor.value?.undoAll();
    await load();
  } finally {
    busy.value = false;
  }
}

function layout() {
  editor.value?.layout();
}

async function close() {
  if (await fileEditor.confirmFiles()) emit("close");
}

defineExpose({ layout, confirmUnsaved: fileEditor.confirmFiles, dirtyFiles: fileEditor.dirtyFiles });
</script>

<template>
  <aside class="inbox-file-review" @keydown.capture="fileEditor.keydown">
    <section v-if="file" class="editor-area">
      <header class="editor-tabbar">
        <div class="editor-tabs">
          <UiTooltip :content="filePath"><button type="button" class="editor-tab active" :class="{ 'is-dirty': fileEditor.dirty(file) }">
            <FileTypeIcon :name="title" />
            <span class="editor-tab-name">{{ title }}</span>
            <UiTooltip :content="t('common.close')">
              <span class="tab-close" :class="{ 'is-dirty': fileEditor.dirty(file) }" @click="close">
                <X :size="12" />
              </span>
            </UiTooltip>
          </button></UiTooltip>
        </div>
      </header>
      <div class="editor-main">
        <MarkdownSourceSwitch v-if="markdownFile" v-model="markdownMode" />
        <div v-motion="'dropdown'" v-if="pendingChunks && markdownMode !== 'preview'" class="chat-editor-overlay-widget">
          <UiButton
            class="diff-file-action diff-file-action-keep"
            size="small"
            :tip="t('diff.keepAllTitle')"
            :disabled="busy"
            @click="keep"
          >{{ t('diff.keep') }}</UiButton>
          <UiButton
            class="diff-file-action"
            variant="ghost"
            size="small"
            :tip="t('diff.revertFileTitle')"
            :disabled="busy"
            @click="undo"
          >{{ t('diff.revert') }}</UiButton>
        </div>
        <MarkdownPreview
          v-if="markdownFile && markdownMode === 'preview' && file"
          :content="file.content"
          :path="file.path"
        />
        <EditorWithDiff
          v-show="!markdownFile || markdownMode !== 'preview'"
          ref="editor"
          :file-path="file.path"
          :workspace-id="file.workspaceId"
          :content="file.content"
          :language="file.language"
          :read-only="file.readOnly"
          :theme="theme"
          :tab-size="indent.tabSize"
          :insert-spaces="indent.insertSpaces"
          :sticky-scroll="stickyScroll"
          :word-wrap="wordWrap"
          @change="(content, fromUser) => fileEditor.change(file, content, fromUser)"
          @mount="fileEditor.bind($event, () => file)"
          @pending-chunks="pendingChunks = $event"
        />
      </div>
      <p v-if="editorError" class="form-error" role="alert">{{ editorError }}</p>
    </section>
  </aside>
</template>
