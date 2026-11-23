<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { X } from "@lucide/vue";
import EditorWithDiff from "../workbench/EditorWithDiff.vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import { fileName } from "../chat/timeline-display";
import { detectIndent } from "../../editor/eol";
import { currentDocumentTheme, monacoThemeName } from "../../editor/monaco-workspace";
import type { FileContent } from "../../domain/types";
import { useAppStore } from "../../stores/app";
import { t } from "../../i18n";
import * as backend from "../../services/backend";

const props = defineProps<{
  runId: string;
  path: string;
  /** 占当前侧栏剩余高度，不再在侧栏左边另开一列 */
  embedded?: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

const store = useAppStore();
const file = ref<FileContent | null>(null);
const error = ref("");

const title = computed(() => fileName(props.path) || props.path);
const indent = computed(() => detectIndent(file.value?.content ?? ""));
const stickyScroll = computed(() => store.data?.settings.stickyScroll !== "false");
const wordWrap = computed(() => store.data?.settings.wordWrap === "true");
const theme = monacoThemeName(currentDocumentTheme());

async function load() {
  error.value = "";
  file.value = null;
  try {
    file.value = await backend.readWorkflowFile(props.runId, props.path);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

watch(() => [props.runId, props.path] as const, () => {
  void load();
}, { immediate: true });
</script>

<template>
  <aside class="wf-file-preview" :class="{ embedded }">
    <header class="editor-tabbar">
      <div class="editor-tabs">
        <button type="button" class="editor-tab active">
          <FileTypeIcon :name="title" />
          <span class="editor-tab-name">{{ title }}</span>
          <span class="tab-close" role="button" :aria-label="t('common.close')" @click="emit('close')">
            <X :size="12" />
          </span>
        </button>
      </div>
    </header>
    <p v-if="error" class="preview-error">{{ error }}</p>
    <div v-else-if="file" class="editor-main">
      <EditorWithDiff
        :file-path="file.path"
        :workspace-id="file.workspaceId"
        :content="file.content"
        :language="file.language"
        :read-only="true"
        :git="false"
        :theme="theme"
        :tab-size="indent.tabSize"
        :insert-spaces="indent.insertSpaces"
        :sticky-scroll="stickyScroll"
        :word-wrap="wordWrap"
      />
    </div>
  </aside>
</template>

<style scoped>
.wf-file-preview {
  display: flex;
  flex: none;
  flex-direction: column;
  width: min(520px, 46vw);
  min-width: 280px;
  min-height: 0;
  height: 100%;
  border-left: 1px solid var(--color-border);
  background: var(--color-bg);
}
.wf-file-preview.embedded {
  width: auto;
  min-width: 0;
  flex: 1;
  border-left: 0;
  border-top: 1px solid var(--color-border);
}
.editor-main {
  flex: 1;
  min-height: 0;
}
.preview-error {
  margin: var(--spacing-3);
  color: var(--color-error);
  font-size: var(--font-size-sm);
}
.tab-close {
  display: inline-flex;
  margin-left: 4px;
  cursor: pointer;
}
</style>
