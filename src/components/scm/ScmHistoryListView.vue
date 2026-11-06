<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import Codicon from "../ui/Codicon.vue";
import UiButton from "../ui/UiButton.vue";
import UiPane from "../ui/UiPane.vue";
import * as backend from "../../services/backend";
import { formatRelativeTime } from "../ui/relative-time";
import { t } from "../../i18n";
import {
  contributorsFromHistory,
  historyToInspectRows,
  loadLineHistory,
  type HistoryListMode,
  type InspectHistoryRow,
} from "./scm-inspect";
import type { ScmOpenTarget } from "../workbench/scm-groups";

const props = defineProps<{
  kind: "file" | "line";
  title: string;
  storageKey: string;
  sizeKey: string;
  workspaceId: string;
  path: string;
  line: number;
  epoch: number;
  live?: boolean;
}>();

const emit = defineEmits<{
  open: [target: ScmOpenTarget];
  openFile: [path: string];
  /** 跳到另一侧历史抽屉（文件 ↔ 行）。 */
  switchKind: [kind: "file" | "line"];
}>();

const PAGE = 30;
/** 对齐 GitLens lineTracker / activeEditor：行变更防抖 250ms。 */
const LINE_DEBOUNCE_MS = 250;
const rows = ref<InspectHistoryRow[]>([]);
const loading = ref(false);
const hasMore = ref(false);
const pinned = ref(false);
const pinnedPath = ref("");
const pinnedLine = ref(0);
/** 文件历史：提交列表 | 贡献者。 */
const mode = ref<HistoryListMode>("commits");
let ticket = 0;
let lineTimer = 0;
/** 上次已请求的键；相同选区/文件不重复拉（对齐 GitLens URI / selection 相等则跳过）。 */
let loadedKey = "";

const activePath = computed(() => (pinned.value ? pinnedPath.value : props.path));
const activeLine = computed(() => (pinned.value ? pinnedLine.value : props.line));
const contributors = computed(() => contributorsFromHistory(rows.value));

const emptyText = computed(() => {
  if (!activePath.value) return t("scm.inspect.needFile");
  if (props.kind === "line" && activeLine.value < 1) return t("scm.inspect.needLine");
  if (!loading.value && !rows.value.length) return t("scm.inspect.emptyHistory");
  return "";
});

const paneTitle = computed(() => {
  if (!pinned.value) return props.title;
  return t("scm.inspect.pinnedTitle", { title: props.title });
});

function requestKey() {
  // 文件历史只跟文件，不跟光标行（GitLens FileHistoryTracker 只听 active editor URI）。
  if (props.kind === "file") {
    return `file|${props.workspaceId}|${activePath.value}|${props.epoch}`;
  }
  return `line|${props.workspaceId}|${activePath.value}|${activeLine.value}|${props.epoch}`;
}

async function load(reset = true) {
  const key = requestKey();
  if (reset && key === loadedKey) return;
  const id = ++ticket;
  if (reset) {
    rows.value = [];
    hasMore.value = false;
  }
  if (!props.workspaceId || !activePath.value || (props.kind === "line" && activeLine.value < 1)) {
    loading.value = false;
    loadedKey = key;
    return;
  }
  loading.value = true;
  try {
    if (props.kind === "line") {
      const next = await loadLineHistory(props.workspaceId, activePath.value, activeLine.value);
      if (id !== ticket) return;
      rows.value = next;
      hasMore.value = false;
    } else {
      const page = await backend.gitLog(
        props.workspaceId,
        reset ? 0 : rows.value.length,
        PAGE,
        activePath.value,
      );
      if (id !== ticket) return;
      const mapped = historyToInspectRows(page.items);
      rows.value = reset ? mapped : [...rows.value, ...mapped];
      hasMore.value = page.hasMore;
    }
    loadedKey = key;
  } catch {
    if (id !== ticket) return;
    if (reset) rows.value = [];
    hasMore.value = false;
    loadedKey = "";
  } finally {
    if (id === ticket) loading.value = false;
  }
}

function scheduleLoad() {
  window.clearTimeout(lineTimer);
  // 文件历史立即跟文件切换；行历史防抖，避免点选编辑器时连打 blame。
  if (props.kind === "file") {
    void load(true);
    return;
  }
  lineTimer = window.setTimeout(() => void load(true), LINE_DEBOUNCE_MS);
}

// 文件：workspace + path + epoch；行：再加 line。不把 line 绑到文件历史上。
watch(
  () => (
    props.kind === "file"
      ? ([props.kind, props.workspaceId, activePath.value, props.epoch] as const)
      : ([props.kind, props.workspaceId, activePath.value, activeLine.value, props.epoch] as const)
  ),
  () => scheduleLoad(),
  { immediate: true },
);

watch(
  () => props.kind,
  () => {
    mode.value = "commits";
    loadedKey = "";
  },
);

onBeforeUnmount(() => window.clearTimeout(lineTimer));

function togglePin() {
  if (pinned.value) {
    pinned.value = false;
    return;
  }
  if (!props.path) return;
  pinned.value = true;
  pinnedPath.value = props.path;
  pinnedLine.value = props.line;
}

function toggleMode() {
  mode.value = mode.value === "commits" ? "contributors" : "commits";
}

function openRow(row: InspectHistoryRow) {
  if (!props.workspaceId || !activePath.value) return;
  emit("open", {
    workspaceId: props.workspaceId,
    path: activePath.value,
    group: "commit",
    commit: row.hash,
    parent: row.parents[0] ?? null,
  });
}

function when(iso: string) {
  if (!iso) return "";
  const ms = Date.parse(iso);
  if (!Number.isFinite(ms)) return iso;
  return formatRelativeTime(ms, Date.now());
}

const pane = ref<{ setExpanded: (value: boolean) => void } | null>(null);
defineExpose({
  setExpanded(value: boolean) {
    pane.value?.setExpanded(value);
  },
});
</script>

<template>
  <UiPane
    ref="pane"
    :title="paneTitle"
    :storage-key="storageKey"
    :size-key="sizeKey"
    :default-size="200"
    :default-expanded="false"
    :min-body="60"
    :actions-visible="pinned || mode === 'contributors'"
  >
    <template #actions>
      <UiButton
        v-if="activePath"
        variant="ghost"
        size="icon"
        :tip="t('scm.inspect.revealFile')"
        @click="emit('openFile', activePath)"
      >
        <Codicon name="go-to-file" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :tip="pinned ? t('scm.inspect.unpin') : t('scm.inspect.pin')"
        :disabled="!path"
        @click="togglePin"
      >
        <Codicon :name="pinned ? 'pinned' : 'pin'" />
      </UiButton>
      <UiButton
        v-if="kind === 'file'"
        variant="ghost"
        size="icon"
        :tip="t('scm.inspect.viewLineHistory')"
        @click="emit('switchKind', 'line')"
      >
        <Codicon name="list-selection" />
      </UiButton>
      <UiButton
        v-if="kind === 'line'"
        variant="ghost"
        size="icon"
        :tip="t('scm.inspect.viewFileHistory')"
        @click="emit('switchKind', 'file')"
      >
        <Codicon name="history" />
      </UiButton>
      <UiButton
        v-if="kind === 'file'"
        variant="ghost"
        size="icon"
        :tip="mode === 'commits' ? t('scm.inspect.viewContributors') : t('scm.inspect.viewCommits')"
        @click="toggleMode"
      >
        <Codicon :name="mode === 'commits' ? 'organization' : 'git-commit'" />
      </UiButton>
      <UiButton variant="ghost" size="icon" :tip="t('scm.inspect.refresh')" :disabled="loading" @click="load(true)">
        <Codicon name="refresh" />
      </UiButton>
    </template>
    <div class="scm-inspect-list">
      <p v-if="emptyText" class="tree-empty">{{ emptyText }}</p>
      <p v-else-if="loading && !rows.length" class="tree-empty">{{ t("scm.inspect.loading") }}</p>
      <template v-else-if="kind === 'file' && mode === 'contributors'">
        <div
          v-for="row in contributors"
          :key="row.author"
          class="scm-inspect-row scm-inspect-row--static"
        >
          <span class="scm-inspect-row-main">
            <span class="scm-inspect-subject">{{ row.author }}</span>
            <span class="scm-inspect-meta">{{ when(row.lastAt) }}</span>
          </span>
          <span class="scm-inspect-sha">{{ t("scm.inspect.contributorCommits", { n: row.commits }) }}</span>
        </div>
      </template>
      <template v-else>
        <button
          v-for="row in rows"
          :key="row.hash"
          type="button"
          class="scm-inspect-row"
          @click="openRow(row)"
        >
          <span class="scm-inspect-row-main">
            <span class="scm-inspect-subject">{{ row.subject }}</span>
            <span class="scm-inspect-meta">{{ row.author }} · {{ when(row.committedAt) }}</span>
          </span>
          <span class="scm-inspect-sha">{{ row.hash.slice(0, 7) }}</span>
        </button>
        <UiButton
          v-if="hasMore"
          size="small"
          class="scm-inspect-more"
          :disabled="loading"
          @click="load(false)"
        >
          {{ t("scm.inspect.loadMore") }}
        </UiButton>
      </template>
    </div>
  </UiPane>
</template>
