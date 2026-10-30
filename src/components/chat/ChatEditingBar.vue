<script setup lang="ts">
import { computed, ref, watch } from "vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import { useDiffStore } from "../../stores/diff";
import { useAppStore } from "../../stores/app";
import { fileName } from "./timeline-display";
import { workspaceFileLabel } from "../workbench/workspace-file-location";
import { t } from "../../i18n";

const props = defineProps<{
  workspaceIds: string[];
  /** 在当前页打开文件，而不是跳到工作台编辑器。 */
  reviewInPlace?: boolean;
}>();

const emit = defineEmits<{
  open: [file: { workspaceId: string; filePath: string }];
}>();

const diffStore = useDiffStore();
const store = useAppStore();
const contents = ref(new Map<string, string>());
const busy = ref(false);

function fileKey(workspaceId: string, filePath: string) {
  return `${workspaceId}\n${filePath}`;
}

function fileLabel(workspaceId: string, filePath: string) {
  const workspace = store.data?.workspaces.find((item) => item.id === workspaceId);
  return workspaceFileLabel(
    workspace?.name || workspaceId,
    filePath,
    props.workspaceIds.length > 1,
  );
}

const files = computed(() => {
  void diffStore.baselines;
  void diffStore.diskEpoch;
  void contents.value;
  return diffStore.listPendingDiffFiles(props.workspaceIds, contents.value);
});

const rows = computed(() =>
  files.value.map((file) => {
    const content = contents.value.get(fileKey(file.workspaceId, file.filePath));
    const chunks = content === undefined
      ? []
      : diffStore.getChunksForFile(file.workspaceId, file.filePath, content);
    let added = 0;
    let removed = 0;
    for (const chunk of chunks) {
      added += chunk.hunk.newLines.length;
      removed += chunk.hunk.oldLines.length;
    }
    return { ...file, added, removed };
  }),
);

const totals = computed(() =>
  rows.value.reduce(
    (sum, row) => ({ added: sum.added + row.added, removed: sum.removed + row.removed }),
    { added: 0, removed: 0 },
  ),
);

let readEpoch = -1;

// diskEpoch 不变时只补读新出现的文件；变了才全部重读，并走 diff store 的共享缓存，和编辑卡共用同一次读盘。
async function refreshContents() {
  const listed = diffStore.listSnapshotFiles(props.workspaceIds);
  const epoch = diffStore.diskEpoch;
  const previous = epoch === readEpoch ? contents.value : new Map<string, string>();
  const next = new Map<string, string>();
  await Promise.all(listed.map(async (file) => {
    const key = fileKey(file.workspaceId, file.filePath);
    next.set(key, previous.get(key) ?? await diffStore.readLiveContent(file.workspaceId, file.filePath));
  }));
  if (epoch !== diffStore.diskEpoch) return;
  readEpoch = epoch;
  const unchanged = next.size === contents.value.size
    && [...next].every(([key, value]) => contents.value.get(key) === value);
  if (!unchanged) contents.value = next;
}

watch(
  () => [props.workspaceIds.join("\n"), diffStore.diskEpoch, diffStore.baselines] as const,
  () => {
    void refreshContents();
  },
  { immediate: true },
);

async function keep(workspaceId: string, filePath: string) {
  if (busy.value) return;
  busy.value = true;
  try {
    const content = contents.value.get(fileKey(workspaceId, filePath));
    await diffStore.acceptAllChunks(workspaceId, filePath, content);
  } finally {
    busy.value = false;
  }
}

async function undo(workspaceId: string, filePath: string) {
  if (busy.value) return;
  busy.value = true;
  try {
    await diffStore.undoAllChunks(workspaceId, filePath);
    await refreshContents();
  } finally {
    busy.value = false;
  }
}

function open(workspaceId: string, filePath: string) {
  if (props.reviewInPlace) {
    emit("open", { workspaceId, filePath });
    return;
  }
  const chunks = diffStore.getChunksForFile(
    workspaceId,
    filePath,
    contents.value.get(fileKey(workspaceId, filePath)) ?? "",
  );
  const line = chunks[0]?.hunk.newStart || undefined;
  store.revealWorkspaceFile(filePath, line, line, workspaceId);
}

function openFirst() {
  const row = rows.value[0];
  if (!row) return;
  open(row.workspaceId, row.filePath);
}
</script>

<template>
  <section v-if="rows.length" class="chat-editing-bar">
    <header class="chat-editing-overview">
      <button
        v-if="reviewInPlace"
        type="button"
        class="working-set-title chat-editing-overview-open"
        @click="openFirst"
      >{{ t('chat.editingFilesCount', { count: rows.length }) }}</button>
      <span v-else class="working-set-title">{{ t('chat.editingFilesCount', { count: rows.length }) }}</span>
      <span class="working-set-line-counts">
        <span v-if="totals.added" class="working-set-lines-added">+{{ totals.added }}</span>
        <span v-if="totals.removed" class="working-set-lines-removed">−{{ totals.removed }}</span>
      </span>
    </header>
    <div class="chat-editing-files vs-scroll">
      <div v-for="row in rows" :key="fileKey(row.workspaceId, row.filePath)" class="chat-editing-file">
        <button type="button" class="chat-editing-file-name" @click="open(row.workspaceId, row.filePath)">
          <FileTypeIcon :name="fileName(row.filePath)" />
          <UiTooltip :content="fileLabel(row.workspaceId, row.filePath)">
            <span>{{ fileName(row.filePath) }}</span>
          </UiTooltip>
        </button>
        <span class="working-set-line-counts">
          <span v-if="row.added" class="working-set-lines-added">+{{ row.added }}</span>
          <span v-if="row.removed" class="working-set-lines-removed">−{{ row.removed }}</span>
        </span>
        <span class="chat-editing-file-actions">
          <UiButton variant="ghost" size="small" :tip="t('chat.editingKeepTitle')" :disabled="busy" @click="keep(row.workspaceId, row.filePath)">{{ t('chat.editingKeep') }}</UiButton>
          <UiButton variant="ghost" size="small" :tip="t('chat.editingUndoTitle')" :disabled="busy" @click="undo(row.workspaceId, row.filePath)">{{ t('chat.editingUndo') }}</UiButton>
        </span>
      </div>
    </div>
  </section>
</template>
