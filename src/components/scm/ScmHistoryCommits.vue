<script setup lang="ts">
/**
 * 提交图里的 commit 行：泳道 + 标题/作者/引用，点击展开文件。
 */
import { computed, ref } from "vue";
import type { GitCommitFile, GitHistoryItem } from "../../domain/types";
import Codicon from "../ui/Codicon.vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import UiButton from "../ui/UiButton.vue";
import UiTooltip from "../ui/UiTooltip.vue";
import * as backend from "../../services/backend";
import { fileNameOf } from "../workbench/workspace-mode";
import {
  historyCircleColor,
  historyCircleIndex,
  historyColorMap,
  historyGraph,
  historyGraphRails,
  historyRefBadges,
  historyRowGraphWidth,
  HISTORY_REF_ICON,
  layoutHistoryGraph,
  SCM_LANE_HEIGHT,
  SCM_LANE_WIDTH,
  type HistoryGraphRow,
} from "../workbench/scm-history";
import type { ScmOpenChangesPayload, ScmOpenTarget } from "../workbench/scm-groups";
import GitStatusBadge from "../workbench/GitStatusBadge.vue";
import { gitStatusClass } from "../workbench/tree-git-status";
import { t } from "../../i18n";

const props = defineProps<{
  workspaceId: string;
  commits: GitHistoryItem[];
  branch?: string;
  upstream?: string | null;
}>();

const emit = defineEmits<{
  open: [target: ScmOpenTarget];
  openChanges: [payload: ScmOpenChangesPayload];
}>();

const openHash = ref("");
const filesByHash = ref<Record<string, GitCommitFile[]>>({});
const filesLoading = ref("");

const colorMap = computed(() => historyColorMap(props.branch, props.upstream));
const rows = computed(() => layoutHistoryGraph(props.commits, colorMap.value));

function rowBadges(row: HistoryGraphRow) {
  return historyRefBadges(row.commit, historyCircleColor(row), colorMap.value);
}

function dotX(index: number) {
  return SCM_LANE_WIDTH * (index + 1);
}

function rowGraphWidth(row: HistoryGraphRow) {
  return historyRowGraphWidth(row);
}

function dotClass(hole: boolean, color: number) {
  return hole ? `scm-graph-dot is-hole scm-lane-${color}` : `scm-graph-dot scm-lane-${color}`;
}

function letter(status: string) {
  return status === "?" ? "U" : status || "M";
}

async function ensureCommitFiles(hash: string) {
  if (filesByHash.value[hash] || !props.workspaceId) return filesByHash.value[hash] ?? [];
  filesLoading.value = hash;
  try {
    const files = await backend.gitCommitFiles(props.workspaceId, hash);
    filesByHash.value = { ...filesByHash.value, [hash]: files };
    return files;
  } catch {
    return [];
  } finally {
    filesLoading.value = "";
  }
}

async function toggleCommit(hash: string) {
  if (openHash.value === hash) {
    openHash.value = "";
    return;
  }
  openHash.value = hash;
  await ensureCommitFiles(hash);
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

async function openChanges(row: HistoryGraphRow) {
  const hash = row.commit.hash;
  const files = await ensureCommitFiles(hash);
  if (!files.length) return;
  const parent = row.commit.parents[0] ?? null;
  const shortHash = hash.slice(0, 7);
  const subject = row.commit.subject || shortHash;
  emit("openChanges", {
    workspaceId: props.workspaceId,
    title: `${shortHash} - ${subject}`,
    commit: hash,
    parent,
    targets: files.map((file) => ({
      workspaceId: props.workspaceId,
      path: file.path,
      group: "commit" as const,
      commit: hash,
      parent,
      status: file.status,
    })),
  });
}
</script>

<template>
  <div class="scm-history-commits">
    <div
      v-for="row in rows"
      :key="row.commit.hash"
      class="scm-history"
      :data-commit-hash="row.commit.hash"
      :class="{ 'is-open': openHash === row.commit.hash }"
    >
      <div
        class="scm-history-row"
        :class="{ 'is-selected': openHash === row.commit.hash }"
        role="button"
        tabindex="0"
        :title="`${row.commit.subject}\n${row.commit.author}`"
        @click="toggleCommit(row.commit.hash)"
        @keydown.enter.prevent="toggleCommit(row.commit.hash)"
        @keydown.space.prevent="toggleCommit(row.commit.hash)"
      >
        <span class="scm-history-graph">
          <svg class="scm-history-lane" :width="rowGraphWidth(row)" :height="SCM_LANE_HEIGHT" aria-hidden="true">
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
              :cy="SCM_LANE_WIDTH"
              :r="dot.r"
            />
          </svg>
        </span>
        <span class="scm-history-copy">
          <span class="scm-history-subject">{{ row.commit.subject || row.commit.hash.slice(0, 7) }}</span>
          <span v-if="row.commit.author" class="scm-history-author">{{ row.commit.author }}</span>
        </span>
        <span class="scm-history-refs">
          <template v-for="badge in rowBadges(row)" :key="badge.key">
            <UiTooltip :content="badge.title">
              <em :class="`scm-ref scm-lane-${badge.color}`">
                <span v-if="badge.count" class="scm-ref-count">{{ badge.count }}</span>
                <Codicon :name="HISTORY_REF_ICON[badge.kind]" :size="12" />
                <span v-if="badge.name" class="scm-ref-name">{{ badge.name }}</span>
              </em>
            </UiTooltip>
          </template>
        </span>
        <span class="scm-history-actions" @click.stop @mousedown.stop>
          <UiButton variant="ghost" size="icon" :tip="t('scm.graph.openChanges')" @click.stop="openChanges(row)">
            <Codicon name="diff-multiple" :size="12" />
          </UiButton>
        </span>
      </div>
      <div v-if="openHash === row.commit.hash" class="scm-history-files">
        <p v-if="filesLoading === row.commit.hash" class="tree-empty">{{ t("scm.graph.loadingFiles") }}</p>
        <div v-for="file in filesByHash[row.commit.hash] ?? []" :key="file.path" class="scm-history-file">
          <span class="scm-history-graph">
            <svg class="scm-history-lane" :width="rowGraphWidth(row)" :height="SCM_LANE_HEIGHT" aria-hidden="true">
              <path
                v-for="(path, index) in historyGraphRails(row.output, historyCircleIndex(row))"
                :key="`r-${index}`"
                :class="`scm-lane scm-lane-${path.color}`"
                :d="path.d"
                :stroke-width="path.width"
              />
            </svg>
          </span>
          <button
            type="button"
            class="scm-file"
            :class="gitStatusClass(letter(file.status))"
            @click="openFile(row, file)"
          >
            <FileTypeIcon :name="fileNameOf(file.path)" />
            <span class="scm-file-label">
              <span class="scm-file-name">{{ fileNameOf(file.path) }}</span>
              <UiTooltip :content="file.path">
                <span class="scm-file-path">{{ file.path }}</span>
              </UiTooltip>
            </span>
            <GitStatusBadge :letter="letter(file.status)" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
