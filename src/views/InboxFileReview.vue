<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { X } from "@lucide/vue";
import EditorWithDiff from "../components/workbench/EditorWithDiff.vue";
import FileTypeIcon from "../components/FileTypeIcon.vue";
import { fileName } from "../components/chat/timeline-display";
import { detectIndent } from "../editor/eol";
import { currentDocumentTheme, monacoThemeName } from "../editor/monaco-workspace";
import type { FileContent } from "../domain/types";
import { useAppStore } from "../stores/app";
import { useDiffStore } from "../stores/diff";
import { useI18n } from "../i18n";
import * as backend from "../services/backend";

const props = defineProps<{
  workspaceId: string;
  filePath: string;
}>();

const emit = defineEmits<{
  close: [];
}>();

const store = useAppStore();
const diffStore = useDiffStore();
const { t } = useI18n();
const editor = ref<InstanceType<typeof EditorWithDiff> | null>(null);
const file = ref<FileContent | null>(null);
const pendingChunks = ref(0);
const busy = ref(false);

const title = computed(() => fileName(props.filePath) || props.filePath);
const indent = computed(() => detectIndent(file.value?.content ?? ""));
const stickyScroll = computed(() => store.data?.settings.stickyScroll !== "false");
const theme = monacoThemeName(currentDocumentTheme());

async function load() {
  try {
    file.value = await backend.readWorkspaceFile(props.workspaceId, props.filePath);
  } catch {
    file.value = {
      workspaceId: props.workspaceId,
      path: props.filePath,
      content: "",
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
  if (busy.value || !pendingChunks.value) return;
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

defineExpose({ layout });
</script>

<template>
  <aside class="inbox-file-review">
    <section v-if="file" class="editor-area">
      <header class="editor-tabbar">
        <div class="editor-tabs">
          <UiTooltip :content="filePath"><button type="button" class="editor-tab active">
            <FileTypeIcon :name="title" />
            <span class="editor-tab-name">{{ title }}</span>
            <UiTooltip :content="t('common.close')">
              <span class="tab-close" @click="emit('close')">
                <X :size="12" />
              </span>
            </UiTooltip>
          </button></UiTooltip>
        </div>
      </header>
      <div class="editor-main">
        <div v-if="pendingChunks" class="chat-editor-overlay-widget">
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
        <EditorWithDiff
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
          @pending-chunks="pendingChunks = $event"
        />
      </div>
    </section>
  </aside>
</template>
