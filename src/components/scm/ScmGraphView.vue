<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { GitCommitFile, GitHistoryItem, Workspace } from "../../domain/types";
import Codicon from "../ui/Codicon.vue";
import ContextMenu from "../ui/ContextMenu.vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import UiButton from "../ui/UiButton.vue";
import UiPane from "../ui/UiPane.vue";
import * as backend from "../../services/backend";
import { fileNameOf } from "../workbench/workspace-mode";
import {
  historyCircleIndex,
  historyGraph,
  historyGraphRails,
  historyGraphWidth,
  historyRefBadges,
  HISTORY_REF_ICON,
  isHistoryHead,
  layoutHistoryGraph,
  SCM_LANE_HEIGHT,
  SCM_LANE_WIDTH,
  type HistoryGraphRow,
} from "../workbench/scm-history";
import type { ScmOpenTarget } from "../workbench/scm-groups";
import { gitStatusClass } from "../workbench/tree-git-status";
import { t } from "../../i18n";

const props = defineProps<{
  workspaces: Workspace[];
  workspaceId: string;
  epoch: number;
  live?: boolean;
}>();
const emit = defineEmits<{
  open: [target: ScmOpenTarget];
  error: [message: string];
  updated: [];
}>();

const menu = ref<{
  x: number;
  y: number;
  commit: GitHistoryItem;
  row: HistoryGraphRow;
  file?: GitCommitFile;
} | null>(null);

const menuItems = computed(() => menu.value?.file
  ? [{ id: "open-file", label: t("scm.graph.openFile") }]
  : [
      { id: "changes", label: t("scm.graph.openChanges") },
      { type: "sep" as const },
      { id: "detach", label: t("scm.graph.detachCheckout") },
      { id: "branch", label: t("scm.graph.createBranch") },
      { id: "tag", label: t("scm.graph.createTag") },
      { id: "cherry-pick", label: t("scm.graph.cherryPick") },
      { type: "sep" as const },
      { id: "copy-hash", label: t("scm.graph.copyHash") },
      { id: "copy-message", label: t("scm.graph.copyMessage") },
    ]);

const previewHistory: GitHistoryItem[] = [
  { hash: "h1", parents: ["h2"], subject: "修改图片资源", author: "Ada", committedAt: "", refs: ["HEAD -> feat/demo", "origin/feat/demo"] },
  { hash: "h2", parents: ["h3", "h4"], subject: "Merge branch 'hotfix/user'", author: "Ada", committedAt: "", refs: [] },
  { hash: "h4", parents: ["h5"], subject: "修正标题文字颜色", author: "Bo", committedAt: "", refs: ["hotfix/user"] },
  { hash: "h3", parents: ["h5"], subject: "feat: 登录页视觉验收", author: "Cy", committedAt: "", refs: ["origin/main"] },
  { hash: "h5", parents: [], subject: "补充公募交易文件", author: "Ada", committedAt: "", refs: ["main"] },
];

const PAGE = 40;
const collapsed = ref(localStorage.getItem("multara.scm.graphCollapsed") === "1");
const graphExpanded = computed({
  get: () => !collapsed.value,
  set: (value: boolean) => {
    collapsed.value = !value;
    if (!collapsed.value && !items.value.length) void load(true);
  },
});
const items = ref<GitHistoryItem[]>([]);
const hasMore = ref(false);
const loading = ref(false);
const openHash = ref("");
const filesByHash = ref<Record<string, GitCommitFile[]>>({});
const filesLoading = ref("");

const rows = computed(() => layoutHistoryGraph(items.value));
const graphWidth = computed(() => historyGraphWidth(rows.value));
const workspaceName = computed(
  () => props.workspaces.find((item) => item.id === props.workspaceId)?.name ?? "",
);

watch([() => props.workspaceId, () => props.epoch, () => props.live], () => {
  items.value = [];
  hasMore.value = false;
  openHash.value = "";
  filesByHash.value = {};
  if (props.live === false) {
    items.value = previewHistory;
    return;
  }
  if (props.workspaceId && !collapsed.value) void load(true);
}, { immediate: true });

let historyTicket = 0;

async function load(reset: boolean) {
  const workspaceId = props.workspaceId;
  if (!workspaceId || props.live === false) return;
  const ticket = ++historyTicket;
  loading.value = true;
  try {
    const page = await backend.gitLog(workspaceId, reset ? 0 : items.value.length, PAGE);
    if (ticket !== historyTicket) return;
    items.value = reset ? page.items : [...items.value, ...page.items];
    hasMore.value = page.hasMore;
  } catch (cause) {
    if (ticket !== historyTicket) return;
    emit("error", cause instanceof Error ? cause.message : String(cause));
  } finally {
    if (ticket === historyTicket) loading.value = false;
  }
}

function openMenu(event: MouseEvent, row: HistoryGraphRow, file?: GitCommitFile) {
  event.preventDefault();
  event.stopPropagation();
  menu.value = { x: event.clientX, y: event.clientY, commit: row.commit, row, file };
}

async function onMenu(id: string) {
  const current = menu.value;
  menu.value = null;
  if (!current) return;
  if (id === "open-file" && current.file) {
    openFile(current.row, current.file);
    return;
  }
  if (id === "changes") {
    if (openHash.value !== current.commit.hash) toggleCommit(current.commit.hash);
    return;
  }
  if (id === "copy-hash" || id === "copy-message") {
    const text = id === "copy-hash" ? current.commit.hash : current.commit.subject;
    await navigator.clipboard.writeText(text);
    return;
  }
  const promptLabel = id === "branch" ? t("scm.graph.branchNamePrompt") : t("scm.graph.tagNamePrompt");
  const name = id === "branch" || id === "tag" ? window.prompt(promptLabel)?.trim() ?? "" : "";
  if ((id === "branch" || id === "tag") && !name) return;
  try {
    await backend.gitGraphCommitAction(props.workspaceId, id, current.commit.hash, name);
    emit("updated");
  } catch (cause) {
    emit("error", cause instanceof Error ? cause.message : String(cause));
  }
}

async function toggleCommit(hash: string) {
  if (openHash.value === hash) {
    openHash.value = "";
    return;
  }
  openHash.value = hash;
  if (filesByHash.value[hash] || !props.workspaceId) return;
  const workspaceId = props.workspaceId;
  filesLoading.value = hash;
  try {
    filesByHash.value = {
      ...filesByHash.value,
      [hash]: await backend.gitCommitFiles(workspaceId, hash),
    };
  } catch (cause) {
    emit("error", cause instanceof Error ? cause.message : String(cause));
  } finally {
    filesLoading.value = "";
  }
}

function openFile(row: HistoryGraphRow, file: GitCommitFile) {
  emit("open", {
    workspaceId: props.workspaceId,
    path: file.path,
    group: "commit",
    commit: row.commit.hash,
    parent: row.commit.parents[0] ?? null,
    status: file.status,
  });
}

function dotX(index: number) {
  return SCM_LANE_WIDTH * (index + 1);
}

function dotClass(hole: boolean, color: number) {
  return hole ? `scm-graph-dot is-hole scm-lane-${color}` : `scm-graph-dot scm-lane-${color}`;
}

function letter(status: string) {
  return status === "?" ? "U" : status || "M";
}
</script>

<template>
  <UiPane
    v-model:expanded="graphExpanded"
    :title="t('scm.graph.title')"
    storage-key="multara.scm.graphCollapsed"
    size-key="multara.scm.graphSize"
    :default-size="200"
    :min-body="120"
  >
    <template #title-extra>
      <span v-if="workspaceName" class="scm-graph-project" :title="workspaceName">{{ workspaceName }}</span>
    </template>
    <template #actions>
      <UiButton
        v-if="live !== false"
        variant="ghost"
        size="icon"
        :title="t('scm.graph.refresh')"
        :disabled="loading"
        @click="load(true)"
      >
        <Codicon v-if="loading" name="loading" class="codicon-modifier-spin" />
        <Codicon v-else name="refresh" />
      </UiButton>
    </template>
    <div class="scm-graph-body">
      <p v-if="loading && !rows.length" class="tree-empty">{{ t('scm.graph.loadingCommits') }}</p>
      <p v-else-if="!rows.length" class="tree-empty">{{ t('scm.graph.noCommits') }}</p>
      <div
        v-for="row in rows"
        :key="row.commit.hash"
        class="scm-history"
        :class="{ 'is-head': isHistoryHead(row.commit), 'is-open': openHash === row.commit.hash }"
      >
        <button
          type="button"
          class="scm-history-row"
          :title="`${row.commit.subject}\n${row.commit.author}`"
          @click="toggleCommit(row.commit.hash)"
          @contextmenu="openMenu($event, row)"
        >
          <svg class="scm-history-lane" :width="graphWidth" :height="SCM_LANE_HEIGHT" aria-hidden="true">
            <path
              v-for="(path, index) in historyGraph(row, openHash === row.commit.hash).paths"
              :key="`p-${index}`"
              :class="`scm-lane scm-lane-${path.color}`"
              :d="path.d"
              :stroke-width="path.width"
            />
            <circle
              v-for="(dot, index) in historyGraph(row, openHash === row.commit.hash).dots"
              :key="`d-${index}`"
              :class="dotClass(dot.hole, dot.color)"
              :cx="dotX(dot.index)"
              cy="11"
              :r="dot.r"
            />
          </svg>
          <span class="scm-history-subject">{{ row.commit.subject || row.commit.hash.slice(0, 7) }}</span>
          <span class="scm-history-refs">
            <em
              v-for="badge in historyRefBadges(row.commit)"
              :key="badge.key"
              :class="`scm-ref scm-ref-${badge.kind}`"
              :title="badge.title"
            >
              <span v-if="badge.count" class="scm-ref-count">{{ badge.count }}</span>
              <Codicon :name="HISTORY_REF_ICON[badge.kind]" :size="12" />
              <span v-if="badge.name" class="scm-ref-name">{{ badge.name }}</span>
            </em>
          </span>
        </button>
        <div v-if="openHash === row.commit.hash" class="scm-history-files">
          <p v-if="filesLoading === row.commit.hash" class="tree-empty">{{ t('scm.graph.loadingFiles') }}</p>
          <div
            v-for="file in filesByHash[row.commit.hash] ?? []"
            :key="file.path"
            class="scm-history-file"
          >
            <svg class="scm-history-lane" :width="graphWidth" :height="SCM_LANE_HEIGHT" aria-hidden="true">
              <path
                v-for="(path, index) in historyGraphRails(row.output, historyCircleIndex(row))"
                :key="`r-${index}`"
                :class="`scm-lane scm-lane-${path.color}`"
                :d="path.d"
                :stroke-width="path.width"
              />
            </svg>
            <button
              type="button"
              class="scm-file"
              :class="gitStatusClass(letter(file.status))"
              @click="openFile(row, file)"
              @contextmenu="openMenu($event, row, file)"
            >
              <FileTypeIcon :name="fileNameOf(file.path)" />
              <span class="scm-file-name">{{ fileNameOf(file.path) }}</span>
              <span class="scm-file-path" :title="file.path">{{ file.path }}</span>
              <b>{{ letter(file.status) }}</b>
            </button>
          </div>
        </div>
      </div>
      <UiButton
        v-if="hasMore"
        size="small"
        :disabled="loading"
        @click="load(false)"
      >
        {{ t('scm.graph.loadMore') }}
      </UiButton>
    </div>
    <ContextMenu
      v-if="menu"
      :x="menu.x"
      :y="menu.y"
      :items="menuItems"
      @close="menu = null"
      @select="onMenu"
    />
  </UiPane>
</template>
