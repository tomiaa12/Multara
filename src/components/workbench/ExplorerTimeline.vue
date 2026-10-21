<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { GitHistoryItem } from "../../domain/types";
import type { ContextMenuItem } from "../ui/ContextMenu.vue";
import ContextMenu from "../ui/ContextMenu.vue";
import type { ScmOpenTarget } from "./scm-groups";
import * as backend from "../../services/backend";
import {
  absoluteTimelineTime,
  deleteLocalHistory,
  listLocalHistory,
  mergeTimeline,
  onLocalHistoryChange,
  registerLocalHistory,
  renameLocalHistorySource,
  timelineTime,
  type LocalHistoryEntry,
  type TimelineEntry,
} from "./local-history";

const PAGE = 20;

const props = defineProps<{
  workspaceId: string;
  path: string;
  git: boolean;
  preview: boolean;
}>();

const emit = defineEmits<{
  open: [target: ScmOpenTarget];
  restore: [entry: LocalHistoryEntry];
}>();

const now = Date.now();
const previewGit: GitHistoryItem[] = [
  { hash: "h1", parents: ["h2"], subject: "修改图片资源", author: "Ada", committedAt: new Date(now - 3 * 60 * 60_000).toISOString(), refs: [] },
  { hash: "h2", parents: ["h3"], subject: "Merge branch 'hotfix/user'", author: "Ada", committedAt: new Date(now - 26 * 60 * 60_000).toISOString(), refs: [] },
  { hash: "h4", parents: ["h5"], subject: "修正标题文字颜色", author: "Bo", committedAt: new Date(now - 3 * 24 * 60 * 60_000).toISOString(), refs: [] },
];

const previewLocal: LocalHistoryEntry[] = [
  {
    id: "preview-local-1",
    workspaceId: "preview-workspace",
    path: "src/App.vue",
    savedAt: now - 4 * 60_000,
    source: "保存",
    content: "const title = \"旧标题\"\n",
  },
  {
    id: "preview-local-2",
    workspaceId: "preview-workspace",
    path: "src/App.vue",
    savedAt: now - 5 * 60_000,
    source: "撤销",
    content: "const title = \"更早的标题\"\n",
  },
];
registerLocalHistory(previewLocal);

const gitItems = ref<GitHistoryItem[]>([]);
const loading = ref(false);
const hasMore = ref(false);
const localSource = ref(localStorage.getItem("multara.explorer.timelineLocal") !== "0");
const gitSource = ref(localStorage.getItem("multara.explorer.timelineGit") !== "0");
const filterMenu = ref<{ x: number; y: number } | null>(null);
const rowMenu = ref<{ x: number; y: number; id: string; source: "local" | "git" } | null>(null);
const pinned = ref(localStorage.getItem("multara.explorer.timelinePin") === "1");
const pinnedTarget = ref<{ workspaceId: string; path: string } | null>(null);
const historyTick = ref(0);
const stopHistory = onLocalHistoryChange(() => {
  historyTick.value += 1;
});
onBeforeUnmount(stopHistory);

const activeWorkspaceId = computed(() =>
  pinned.value && pinnedTarget.value ? pinnedTarget.value.workspaceId : props.workspaceId,
);
const activePath = computed(() =>
  pinned.value && pinnedTarget.value ? pinnedTarget.value.path : props.path,
);

const rows = computed(() => {
  void historyTick.value;
  const entries: TimelineEntry[] = [];
  if (localSource.value && activePath.value) {
    for (const item of listLocalHistory(activeWorkspaceId.value, activePath.value)) {
      if (!props.preview && item.id.startsWith("preview-local-")) continue;
      entries.push({
        id: item.id,
        source: "local",
        label: item.source || "保存",
        detail: "",
        at: item.savedAt,
      });
    }
  }
  if (gitSource.value) {
    for (const item of gitItems.value) {
      entries.push({
        id: item.hash,
        source: "git",
        label: item.subject || item.hash.slice(0, 7),
        detail: item.author,
        at: timelineTime(item.committedAt),
      });
    }
  }
  return mergeTimeline(entries);
});

function emptyMessage() {
  if (!activePath.value) return "活动编辑器无法提供时间线信息。";
  if (!localSource.value && !gitSource.value) return "所有时间线源都已筛除。";
  if (!loading.value && !rows.value.length) return "未提供时间线信息。";
  return "";
}

async function load(reset = true) {
  if (reset) {
    gitItems.value = [];
    hasMore.value = false;
  }
  if (!activePath.value || !gitSource.value) return;
  if (props.preview) {
    gitItems.value = activePath.value === "src/App.vue" ? previewGit : [];
    hasMore.value = false;
    return;
  }
  if (!props.git || !activeWorkspaceId.value) return;
  loading.value = true;
  try {
    const page = await backend.gitLog(activeWorkspaceId.value, reset ? 0 : gitItems.value.length, PAGE, activePath.value);
    gitItems.value = reset ? page.items : [...gitItems.value, ...page.items];
    hasMore.value = page.hasMore;
  } catch {
    if (reset) gitItems.value = [];
    hasMore.value = false;
  } finally {
    loading.value = false;
  }
}

watch(
  () => [activeWorkspaceId.value, activePath.value, props.git, props.preview, gitSource.value],
  () => void load(true),
  { immediate: true },
);

function toggleSource(id: string) {
  if (id === "local") {
    localSource.value = !localSource.value;
    localStorage.setItem("multara.explorer.timelineLocal", localSource.value ? "1" : "0");
  }
  if (id === "git") {
    gitSource.value = !gitSource.value;
    localStorage.setItem("multara.explorer.timelineGit", gitSource.value ? "1" : "0");
  }
  filterMenu.value = null;
}

function openFilter(event: MouseEvent) {
  const target = event.currentTarget as HTMLElement | null;
  const box = target?.getBoundingClientRect();
  filterMenu.value = box ? { x: box.left, y: box.bottom } : { x: event.clientX, y: event.clientY };
}

function togglePin() {
  pinned.value = !pinned.value;
  localStorage.setItem("multara.explorer.timelinePin", pinned.value ? "1" : "0");
  if (pinned.value && props.path) {
    pinnedTarget.value = { workspaceId: props.workspaceId, path: props.path };
  }
}

watch(
  () => [props.workspaceId, props.path],
  () => {
    if (!pinned.value && props.path) pinnedTarget.value = { workspaceId: props.workspaceId, path: props.path };
  },
  { immediate: true },
);

function openRow(id: string, source: "local" | "git") {
  if (!activeWorkspaceId.value || !activePath.value) return;
  if (source === "local") {
    emit("open", {
      workspaceId: activeWorkspaceId.value,
      path: activePath.value,
      group: "local",
      localId: id,
    });
    return;
  }
  const item = gitItems.value.find((entry) => entry.hash === id);
  if (!item) return;
  emit("open", {
    workspaceId: activeWorkspaceId.value,
    path: activePath.value,
    group: "commit",
    commit: item.hash,
    parent: item.parents[0] ?? null,
  });
}

const rowMenuItems = computed<ContextMenuItem[]>(() => {
  const menu = rowMenu.value;
  if (!menu) return [];
  if (menu.source === "local") {
    return [
      { id: "compare", label: "与当前文件比较" },
      { id: "restore", label: "还原内容" },
      { id: "rename", label: "重命名…" },
      { id: "delete", label: "删除" },
    ];
  }
  const item = gitItems.value.find((entry) => entry.hash === menu.id);
  return [
    { id: "compare", label: "与当前文件比较" },
    { id: "compare-prev", label: "与上一版本比较", disabled: !item?.parents[0] },
    { type: "sep" },
    { id: "copy-id", label: "复制提交 ID" },
    { id: "copy-message", label: "复制提交消息" },
  ];
});

function openRowMenu(id: string, source: "local" | "git", event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  rowMenu.value = { x: event.clientX, y: event.clientY, id, source };
}

async function copyText(value: string) {
  try {
    await navigator.clipboard.writeText(value);
  } catch {
    /* ignore */
  }
}

function onRowMenuSelect(id: string) {
  const menu = rowMenu.value;
  rowMenu.value = null;
  if (!menu || !activeWorkspaceId.value || !activePath.value) return;
  if (id === "compare") {
    openRow(menu.id, menu.source);
    return;
  }
  if (id === "compare-prev") {
    const item = gitItems.value.find((entry) => entry.hash === menu.id);
    if (!item?.parents[0]) return;
    emit("open", {
      workspaceId: activeWorkspaceId.value,
      path: activePath.value,
      group: "commit",
      commit: item.hash,
      parent: item.parents[0],
    });
    return;
  }
  if (id === "copy-id") {
    void copyText(menu.id);
    return;
  }
  if (id === "copy-message") {
    const item = gitItems.value.find((entry) => entry.hash === menu.id);
    if (item) void copyText(item.subject);
    return;
  }
  if (id === "restore") {
    const entry = listLocalHistory(activeWorkspaceId.value, activePath.value).find((item) => item.id === menu.id);
    if (entry) emit("restore", entry);
    return;
  }
  if (id === "rename") {
    const entry = listLocalHistory(activeWorkspaceId.value, activePath.value).find((item) => item.id === menu.id);
    const next = window.prompt("条目名称", entry?.source ?? "");
    if (next != null) renameLocalHistorySource(menu.id, next);
    return;
  }
  if (id === "delete") deleteLocalHistory(menu.id);
}

defineExpose({ openFilter, refresh: () => load(true), togglePin, pinned });
</script>

<template>
  <div class="explorer-timeline">
    <p v-if="emptyMessage()" class="ui-pane-empty">{{ emptyMessage() }}</p>
    <button
      v-for="row in rows"
      :key="`${row.source}:${row.id}`"
      type="button"
      class="explorer-timeline-row"
      :title="`${row.detail ? `${row.label} · ${row.detail}` : row.label}\n${absoluteTimelineTime(row.at)}`"
      @click="openRow(row.id, row.source)"
      @contextmenu="openRowMenu(row.id, row.source, $event)"
    >
      <Codicon :name="row.source === 'local' ? 'history' : 'git-commit'" />
      <span class="explorer-timeline-copy">
        <span class="explorer-timeline-label">{{ row.label }}</span>
        <span v-if="row.detail" class="explorer-timeline-detail">{{ row.detail }}</span>
      </span>
      <span class="explorer-timeline-time" :class="{ 'is-duplicate': row.hideTimestamp }">{{ row.timestamp }}</span>
    </button>
    <button
      v-if="hasMore && gitSource"
      type="button"
      class="explorer-timeline-more"
      :disabled="loading"
      @click="load(false)"
    >
      {{ loading ? "正在加载…" : "加载更多" }}
    </button>
    <Teleport to="body">
      <ContextMenu
        v-if="filterMenu"
        :x="filterMenu.x"
        :y="filterMenu.y"
        :items="[
          { id: 'local', label: '本地历史', checked: localSource },
          { id: 'git', label: 'Git 历史', checked: gitSource },
        ]"
        @select="toggleSource"
        @close="filterMenu = null"
      />
      <ContextMenu
        v-if="rowMenu"
        :x="rowMenu.x"
        :y="rowMenu.y"
        :items="rowMenuItems"
        @select="onRowMenuSelect"
        @close="rowMenu = null"
      />
    </Teleport>
  </div>
</template>
