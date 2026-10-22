<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ChevronDown } from "@lucide/vue";
import type { DiffLine, PairedTool } from "./timeline-display";
import { DIFF_COLLAPSE_LINES, diffStats, editDiff, fileName, fileRevealTarget, toolPath } from "./timeline-display";
import { workspaceFileLabel } from "../workbench/workspace-file-location";
import { translate, useI18n } from "../../i18n";
import { useAppStore } from "../../stores/app";
import { useDiffStore } from "../../stores/diff";
import * as backend from "../../services/backend";

const props = defineProps<{ item: PairedTool }>();
const { t } = useI18n();
const store = useAppStore();
const diffStore = useDiffStore();
const expanded = ref(false);
const liveContent = ref<string | null>(null);
const busy = ref(false);
const fullPath = computed(() => toolPath(props.item.use) || toolPath(props.item.result) || "");
const path = computed(() => fileName(fullPath.value || "file"));
const editingLabel = computed(() => {
  const language = store.data?.settings.language;
  const pending = !props.item.result;
  if (!fullPath.value) return translate(pending ? "chat.makingEdits" : "chat.edited", language);
  const key = pending ? "chat.editingFile" : "chat.editedFile";
  return translate(key, language).replace("{file}", path.value);
});

const reviewFile = computed(() => {
  void diffStore.baselines;
  const target = fullPath.value;
  if (!target) return null;
  for (const key of diffStore.baselines.keys()) {
    const sep = key.indexOf("\n");
    if (sep < 0) continue;
    if (key.slice(sep + 1) !== target) continue;
    return { workspaceId: key.slice(0, sep), filePath: target };
  }
  return null;
});

const pathTitle = computed(() => {
  const file = reviewFile.value;
  if (!file) return fullPath.value;
  const roots = store.activeSessionId ? store.sessionWorkspaces(store.activeSessionId) : [];
  const workspace = roots.find((item) => item.id === file.workspaceId)
    ?? store.data?.workspaces.find((item) => item.id === file.workspaceId);
  return workspaceFileLabel(workspace?.name || file.workspaceId, file.filePath, roots.length > 1);
});

async function readLive() {
  const file = reviewFile.value;
  if (!file) {
    liveContent.value = null;
    return;
  }
  try {
    liveContent.value = (await backend.readWorkspaceFile(file.workspaceId, file.filePath)).content;
  } catch {
    liveContent.value = "";
  }
}

watch(reviewFile, () => {
  void readLive();
}, { immediate: true });

watch(() => diffStore.diskEpoch, () => {
  void readLive();
});

const liveLines = computed<DiffLine[]>(() => {
  const file = reviewFile.value;
  if (!file || liveContent.value === null) return [];
  const chunks = diffStore.getChunksForFile(file.workspaceId, file.filePath, liveContent.value);
  const lines: DiffLine[] = [];
  for (const chunk of chunks) {
    chunk.hunk.oldLines.forEach((text, index) => {
      lines.push({ kind: "del", oldLine: chunk.hunk.oldStart + index, text });
    });
    chunk.hunk.newLines.forEach((text, index) => {
      lines.push({ kind: "add", newLine: chunk.hunk.newStart + index, text });
    });
  }
  return lines;
});

const usingLiveDiff = computed(() => liveLines.value.length > 0);
const lines = computed(() => usingLiveDiff.value ? liveLines.value : editDiff(props.item));
const stats = computed(() => diffStats(lines.value));
const folded = computed(() => !expanded.value && lines.value.length > DIFF_COLLAPSE_LINES);
const visibleLines = computed(() => folded.value ? lines.value.slice(0, DIFF_COLLAPSE_LINES) : lines.value);

function openFile() {
  const target = fileRevealTarget(props.item.use) ?? fileRevealTarget(props.item.result ?? props.item.use);
  const file = reviewFile.value;
  if (file) {
    store.revealWorkspaceFile(file.filePath, target?.startLine, target?.endLine, file.workspaceId);
    return;
  }
  if (!target) return;
  store.revealWorkspaceFile(target.path, target.startLine, target.endLine);
}

async function keepFile() {
  const file = reviewFile.value;
  if (!file || busy.value) return;
  busy.value = true;
  try {
    await diffStore.acceptAllChunks(file.workspaceId, file.filePath, liveContent.value ?? undefined);
  } finally {
    busy.value = false;
  }
}

async function undoFile() {
  const file = reviewFile.value;
  if (!file || busy.value) return;
  busy.value = true;
  try {
    await diffStore.undoAllChunks(file.workspaceId, file.filePath);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <details v-if="lines.length" class="diff-card" open>
    <summary>
      <ChevronDown :size="12" />
      <button class="diff-file" type="button" :title="pathTitle" @click.stop="openFile">{{ editingLabel }}</button>
      <span v-if="stats.added" class="diff-added">+{{ stats.added }}</span>
      <span v-if="stats.deleted" class="diff-deleted">-{{ stats.deleted }}</span>
      <template v-if="usingLiveDiff">
        <UiButton variant="ghost" size="small" :title="t('diff.keepFileTitle')" :disabled="busy" @click.stop="keepFile">{{ t('diff.keep') }}</UiButton>
        <UiButton variant="ghost" size="small" :title="t('diff.undoFileTitle')" :disabled="busy" @click.stop="undoFile">{{ t('diff.undo') }}</UiButton>
      </template>
    </summary>
    <div class="diff-body vs-scroll" :class="{ 'is-collapsed': folded, 'is-expanded': !folded }">
      <div
        v-for="(line, index) in visibleLines"
        :key="index"
        class="diff-line"
        :data-kind="line.kind"
      >
        <span class="diff-num">{{ line.oldLine ?? line.newLine ?? "" }}</span>
        <span class="diff-mark">{{ line.kind === "add" ? "+" : line.kind === "del" ? "-" : "" }}</span>
        <code>{{ line.text }}</code>
      </div>
      <button
        v-if="lines.length > DIFF_COLLAPSE_LINES"
        class="diff-expand"
        type="button"
        :title="expanded ? t('diff.collapseRest') : t('diff.expandRest')"
        @click="expanded = !expanded"
      >
        <ChevronDown :size="14" />
      </button>
    </div>
  </details>
  <div v-else class="diff-card diff-card-empty">
    <button class="diff-file" type="button" :title="pathTitle" @click="openFile">{{ editingLabel }}</button>
    <span>{{ t('diff.noVisualDiff') }}</span>
  </div>
</template>
