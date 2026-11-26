<script setup lang="ts">
/*
 * 合并到其他分支：把当前工作区已提交的内容按顺序合到选中的目标分支，不切换当前分支。
 * 无冲突的在后台直接完成；有冲突的排到最后，一次只开一个临时 Worktree。
 */
import { computed, onMounted, ref } from "vue";
import { X } from "@lucide/vue";
import type { GitRepositoryStatus, MergeIntoResult, MergePreview, Workspace } from "../../domain/types";
import * as backend from "../../services/backend";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import UiButton from "../ui/UiButton.vue";
import UiInput from "../ui/UiInput.vue";
import SegmentedTabs from "../ui/SegmentedTabs.vue";
import {
  MERGE_FILTER_ALL,
  MERGE_FILTER_RECENT,
  MERGE_FILTER_SELECTED,
  MERGE_PLAN_UP_TO_DATE,
  MERGE_RECENT_KEY,
  canMergePlan,
  mergeBranchGroups,
  mergePlanBadge,
  mergeQueueSummary,
  mergeResultLines,
  mergeRunBadge,
  mergeRunOrder,
  rememberMergeTargets,
} from "./merge-targets";

const props = defineProps<{ workspace: Workspace; repository: GitRepositoryStatus }>();
const emit = defineEmits<{
  close: [];
  merged: [];
  refresh: [];
  /** 先去解决冲突，弹窗关掉；剩下的冲突分支等这次解决完再继续 */
  defer: [payload: { workspaceId: string; pending: string[]; push: boolean }];
  /** 收件箱应切到合并会话聊天，并让右侧 Git 绑临时 Worktree */
  mergeFocus: [payload: { inboxItemId: string | null; workspaceId: string; sessionId: string }];
}>();

const store = useAppStore();

interface PickedTarget {
  name: string;
  preview: NonNullable<MergePreview["target"]> | null;
  loading: boolean;
  error: string;
}

interface MergeOutcome {
  name: string;
  status: "queued" | "running" | "stopped" | "merged" | "up_to_date" | "conflict" | "error" | "pending";
  result: MergeIntoResult | null;
  error: string;
  pushRequested: boolean;
}

const filter = ref(MERGE_FILTER_ALL);
const query = ref("");
const push = ref(true);
const picked = ref<PickedTarget[]>([]);
const sourceLabel = ref(props.repository.branch || "HEAD");
const sourceCommit = ref("");
const origin = ref<MergePreview["origin"]>(null);
const dirty = ref(false);
const previewError = ref("");
const running = ref(false);
const refreshing = ref(false);
const error = ref("");
const outcomes = ref<MergeOutcome[]>([]);
const dragIndex = ref(-1);

const current = computed(() => props.repository.branch);
const locals = computed(() => props.repository.branches.filter((name) => name !== current.value));
const remotes = computed(() => props.repository.remoteBranches.filter((name) => !name.endsWith(`/${current.value}`)));
const recent = ref<string[]>([]);

const groups = computed(() => mergeBranchGroups({
  locals: locals.value,
  remotes: remotes.value,
  recent: recent.value,
  filter: filter.value,
  query: query.value,
  selected: picked.value.map((item) => item.name),
}));
const filterTabs = computed(() => [
  { id: MERGE_FILTER_ALL, label: t("scm.merge.filter.all") },
  { id: MERGE_FILTER_RECENT, label: t("scm.merge.filter.recent") },
  { id: MERGE_FILTER_SELECTED, label: t("scm.merge.filter.selected"), count: picked.value.length, pinCount: true },
]);
const actionable = computed(() => picked.value.filter((item) => item.preview && canMergePlan(item.preview.plan)));
const remoteCount = computed(() => picked.value.filter((item) => item.preview?.remote).length);
const canPush = computed(() => remoteCount.value > 0);
const pushLabel = computed(() => {
  if (!picked.value.length) return t("scm.merge.pushAfter");
  return t("scm.merge.pushAfterCount", { remote: remoteCount.value, total: picked.value.length });
});
const loadingPreview = computed(() => picked.value.some((item) => item.loading));
const canStart = computed(() => actionable.value.length > 0 && !loadingPreview.value && !running.value);
const startLabel = computed(() => {
  if (running.value) return t("scm.merge.running");
  if (actionable.value.length) return t("scm.merge.startCount", { count: actionable.value.length });
  return t("scm.merge.start");
});
const checkoutNotes = computed(() => picked.value.flatMap((item) => {
  const path = item.preview?.checkedOutAt;
  if (!path) return [];
  return [t("scm.merge.checkoutNote", { branch: item.preview?.branch || item.name, path })];
}));
const conflictOutcome = computed(() => outcomes.value.find((item) => item.status === "conflict" && item.result?.mergeSession) ?? null);
const reporting = computed(() => outcomes.value.length > 0);
const settled = computed(() => reporting.value && !running.value);
const finishedCount = computed(() => outcomes.value.filter((item) => item.status === "merged" || item.status === "up_to_date").length);
const queueRows = computed(() => picked.value.map((item) => ({
  item,
  outcome: outcomes.value.find((row) => row.name === item.name) ?? null,
})));
const resultTitle = computed(() => {
  const conflicts = outcomes.value.filter((item) => item.status === "conflict").length;
  if (conflicts) return t("scm.merge.doneConflicts", { count: conflicts });
  if (outcomes.value.some((item) => item.status === "error")) return t("scm.merge.incomplete");
  return t("scm.merge.done");
});
const queueTitle = computed(() => {
  if (!reporting.value) {
    return picked.value.length
      ? t("scm.merge.pickCount", { count: picked.value.length })
      : t("scm.merge.pickPrompt");
  }
  if (running.value) {
    return t("scm.merge.progressSummary", {
      source: sourceLabel.value,
      count: outcomes.value.length,
      finished: finishedCount.value,
      total: outcomes.value.length,
    });
  }
  return resultTitle.value;
});
const blocked = computed(() => running.value || Boolean(conflictOutcome.value));

/** 常用列表全局共用：别的项目选过的分支名也留着，这个项目里存在才会显示。 */
function readRecent() {
  try {
    const parsed = JSON.parse(localStorage.getItem(MERGE_RECENT_KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

function applySource(preview: MergePreview) {
  sourceLabel.value = preview.sourceLabel || sourceLabel.value;
  sourceCommit.value = preview.sourceCommit;
  origin.value = preview.origin ?? null;
  dirty.value = preview.dirty;
}

function isPicked(name: string) {
  return picked.value.some((item) => item.name === name);
}

async function loadSource() {
  previewError.value = "";
  try {
    applySource(await backend.gitMergePreview(props.workspace.id, null));
  } catch (cause) {
    previewError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function loadOne(name: string) {
  try {
    const preview = await backend.gitMergePreview(props.workspace.id, name);
    const item = picked.value.find((row) => row.name === name);
    if (!item) return;
    item.preview = preview.target ?? null;
    item.loading = false;
    item.error = preview.target ? "" : t("scm.merge.branchMissing");
    applySource(preview);
  } catch (cause) {
    const item = picked.value.find((row) => row.name === name);
    if (!item) return;
    item.loading = false;
    item.error = cause instanceof Error ? cause.message : String(cause);
  }
}

function toggle(name: string, on: boolean) {
  if (reporting.value) return;
  if (!on) {
    picked.value = picked.value.filter((item) => item.name !== name);
    return;
  }
  if (isPicked(name)) return;
  picked.value = [...picked.value, { name, preview: null, loading: true, error: "" }];
  void loadOne(name);
}

function onDragStart(event: DragEvent, index: number) {
  dragIndex.value = index;
  event.dataTransfer?.setData("text/plain", String(index));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}

function onDrop(index: number) {
  if (reporting.value) return;
  const from = dragIndex.value;
  dragIndex.value = -1;
  if (from < 0 || from === index) return;
  const next = picked.value.slice();
  const [item] = next.splice(from, 1);
  if (!item) return;
  next.splice(index, 0, item);
  picked.value = next;
}

function saveRecent() {
  const next = rememberMergeTargets(recent.value, picked.value.map((item) => item.name));
  recent.value = next;
  try {
    localStorage.setItem(MERGE_RECENT_KEY, JSON.stringify(next));
  } catch {
    // 存不下时只在这次弹窗里有效
  }
}

function patchOutcome(name: string, patch: Partial<MergeOutcome>) {
  outcomes.value = outcomes.value.map((item) => (item.name === name ? { ...item, ...patch } : item));
}

function rowClass(row: { item: PickedTarget; outcome: MergeOutcome | null }) {
  const status = row.outcome?.status || row.item.preview?.plan;
  return [status ? `is-${status}` : "", row.outcome ? "is-reporting" : ""];
}

async function start() {
  if (!canStart.value) return;
  running.value = true;
  error.value = "";
  saveRecent();
  const pushRequested = push.value && canPush.value;
  const queue = mergeRunOrder(picked.value.flatMap((item) => {
    const plan = item.preview?.plan ?? "";
    if (!canMergePlan(plan) && plan !== MERGE_PLAN_UP_TO_DATE) return [];
    return [{ ...item, plan }];
  }));
  const runnable = new Set(queue.map((item) => item.name));
  const ordered = queue.flatMap((item) => {
    const pickedItem = picked.value.find((row) => row.name === item.name);
    return pickedItem ? [pickedItem] : [];
  });
  picked.value = [...ordered, ...picked.value.filter((item) => !runnable.has(item.name))];
  outcomes.value = queue.map((item) => ({
    name: item.name,
    status: "queued",
    result: null,
    error: "",
    pushRequested,
  }));
  let mergedAny = false;
  for (const [index, item] of queue.entries()) {
    patchOutcome(item.name, { status: "running" });
    try {
      const outcome = await store.mergeIntoBranch(props.workspace.id, item.name, pushRequested && Boolean(item.preview?.remote));
      const status = outcome.status === "conflict" ? "conflict" : outcome.status;
      patchOutcome(item.name, { status, result: outcome, error: "" });
      if (status === "merged" || status === "up_to_date") mergedAny = true;
      if (outcome.status === "conflict" && outcome.mergeSession) {
        for (const row of queue.slice(index + 1)) patchOutcome(row.name, { status: "pending" });
        running.value = false;
        if (mergedAny) emit("merged");
        emit("defer", {
          workspaceId: props.workspace.id,
          pending: queue.slice(index + 1).map((row) => row.name),
          push: pushRequested,
        });
        emit("mergeFocus", {
          inboxItemId: store.inboxItemIdForSession(outcome.mergeSession.sessionId),
          workspaceId: outcome.mergeSession.workspaceId,
          sessionId: outcome.mergeSession.sessionId,
        });
        emit("close");
        return;
      }
    } catch (cause) {
      patchOutcome(item.name, {
        status: "error",
        result: null,
        error: cause instanceof Error ? cause.message : String(cause),
      });
      for (const row of queue.slice(index + 1)) patchOutcome(row.name, { status: "stopped" });
      break;
    }
  }
  running.value = false;
  if (mergedAny) emit("merged");
}

const mergeId = computed(() => conflictOutcome.value?.result?.mergeSession?.id ?? "");

async function resolveWithAi() {
  if (!mergeId.value) return;
  const merge = store.data?.mergeSessions?.find((item) => item.id === mergeId.value);
  const inboxItemId = await store.resolveMergeWithAi(mergeId.value);
  if (merge) {
    emit("mergeFocus", {
      inboxItemId,
      workspaceId: merge.workspaceId,
      sessionId: merge.sessionId,
    });
  }
  emit("close");
}

async function resolveManually() {
  if (!mergeId.value) return;
  const merge = store.data?.mergeSessions?.find((item) => item.id === mergeId.value);
  await store.openMergeSession(mergeId.value);
  if (merge) {
    emit("mergeFocus", {
      inboxItemId: store.inboxItemIdForSession(merge.sessionId),
      workspaceId: merge.workspaceId,
      sessionId: merge.sessionId,
    });
  }
  emit("close");
}

async function abort() {
  if (!mergeId.value) return;
  running.value = true;
  try {
    await store.abortMergeSession(mergeId.value);
    emit("close");
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    running.value = false;
  }
}

async function refreshBranches() {
  if (refreshing.value || running.value) return;
  refreshing.value = true;
  previewError.value = "";
  try {
    await backend.gitFetchWorkspace(props.workspace.id);
    emit("refresh");
  } catch (cause) {
    previewError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    refreshing.value = false;
  }
}

function outcomeLines(item: MergeOutcome) {
  return mergeResultLines({
    status: item.status,
    result: item.result,
    error: item.error,
    pushRequested: item.pushRequested,
  });
}

onMounted(() => {
  recent.value = readRecent();
  void refreshBranches();
  void loadSource();
});
</script>

<template>
  <UiModal class="merge-backdrop" @click.self="!blocked && emit('close')">
    <section class="modal merge-dialog" role="dialog" :aria-label="t('scm.merge.title')">
        <div class="modal-heading">
          <h2>{{ t('scm.merge.title') }}</h2>
          <UiButton
            v-if="!conflictOutcome"
            class="icon-button"
            size="icon"
            :tip="t('common.close')"
            :disabled="running"
            @click="emit('close')"
          >
            <X :size="15" />
          </UiButton>
        </div>

          <div class="merge-source">
            <div class="merge-source-main">
              <span class="merge-chip">{{ t('scm.merge.current') }}</span>
              <strong>{{ sourceLabel }}</strong>
              <code v-if="sourceCommit">{{ sourceCommit }}</code>
            </div>
            <small v-if="origin">
              {{ t('scm.merge.originSummary', { baseRef: origin.baseRef, baseCommit: origin.baseCommit, commits: origin.commits, files: origin.files }) }}
            </small>
          </div>

          <div class="merge-body">
            <div class="merge-picker">
              <div class="merge-search">
                <UiInput v-model="query" type="search" spellcheck="false" :placeholder="t('workspace.searchBranch')" :aria-label="t('workspace.searchBranch')" />
                <UiButton size="small" :disabled="refreshing || running || reporting" @click="refreshBranches">
                  {{ refreshing ? t("scm.merge.refreshing") : t("scm.merge.refresh") }}
                </UiButton>
              </div>
              <SegmentedTabs v-model="filter" :items="filterTabs" :label="t('scm.merge.branchFilter')" />
              <div class="merge-branch-list vs-scroll">
                <p v-if="!groups.length" class="merge-empty">{{ t('scm.merge.noMatch') }}</p>
                <section v-for="group in groups" :key="group.label || 'selected'">
                  <h3 v-if="group.label">{{ group.label }}</h3>
                  <label v-for="row in group.rows" :key="row.name" class="check-row merge-branch">
                    <UiInput
                      type="checkbox"
                      :disabled="reporting"
                      :model-value="isPicked(row.name)"
                      @update:model-value="toggle(row.name, Boolean($event))"
                    />
                    <span class="merge-branch-name">{{ row.name }}</span>
                    <em v-if="row.localCopy" class="merge-branch-mark">{{ t('scm.merge.alsoLocal') }}</em>
                  </label>
                </section>
              </div>
              <p class="merge-picker-foot">{{ t('scm.merge.excludedHint') }}</p>
            </div>

            <div class="merge-queue">
              <div class="merge-queue-head">
                <strong>{{ queueTitle }}</strong>
                <span v-if="picked.length > 1 && !reporting">{{ t('scm.merge.dragOrder') }}</span>
              </div>
              <p v-if="!picked.length" class="merge-empty">{{ t('scm.merge.pickHint') }}</p>
              <ol v-else class="vs-scroll">
                <li
                  v-for="(row, index) in queueRows"
                  :key="row.item.name"
                  :class="rowClass(row)"
                  :draggable="!reporting"
                  @dragstart="onDragStart($event, index)"
                  @dragend="dragIndex = -1"
                  @dragover.prevent
                  @drop="onDrop(index)"
                >
                  <div class="merge-queue-row">
                    <span class="merge-order">
                      <Codicon v-if="row.outcome?.status === 'running'" name="loading" spin :size="14" />
                      <Codicon v-else-if="row.outcome && (row.outcome.status === 'merged' || row.outcome.status === 'up_to_date')" name="check" :size="14" />
                      <Codicon v-else-if="row.outcome && (row.outcome.status === 'conflict' || row.outcome.status === 'error')" name="warning" :size="14" />
                      <Codicon v-else-if="row.outcome" name="clock" :size="14" />
                      <template v-else>{{ index + 1 }}</template>
                    </span>
                    <div class="merge-queue-copy">
                      <b>{{ row.outcome?.result?.targetDisplay || row.item.preview?.display || row.item.name }}</b>
                      <template v-if="row.outcome">
                        <small
                          v-for="(line, lineIndex) in outcomeLines(row.outcome)"
                          :key="`${row.item.name}-${lineIndex}`"
                          :class="{ 'is-warn': line.warn }"
                        >{{ line.text }}</small>
                      </template>
                      <small v-else>{{ mergeQueueSummary(row.item.preview, row.item.loading, row.item.error) }}</small>
                    </div>
                    <em v-if="row.outcome">{{ mergeRunBadge(row.outcome.status) }}</em>
                    <em v-else-if="row.item.preview">{{ mergePlanBadge(row.item.preview.plan) }}</em>
                    <UiTooltip v-if="!row.outcome" :content="t('scm.merge.dragOrder')">
                      <Codicon name="gripper" :size="14" />
                    </UiTooltip>
                  </div>
                  <ul v-if="row.outcome?.result?.conflicts.length" class="merge-conflicts">
                    <li v-for="file in row.outcome?.result?.conflicts || []" :key="file">
                      <UiTooltip :content="file">
                        <span>{{ file }}</span>
                      </UiTooltip>
                    </li>
                  </ul>
                </li>
              </ol>
              <p v-if="picked.length && !reporting" class="merge-note">
                {{ t('scm.merge.queueNote') }}
              </p>
              <p v-if="conflictOutcome" class="merge-note">
                {{ t('scm.merge.conflictTodo', { pushSuffix: conflictOutcome.result?.mergeSession?.push ? t('scm.merge.conflictPushSuffix') : '' }) }}
              </p>
              <div v-if="dirty || checkoutNotes.length" class="merge-warn-box">
                <p v-if="dirty">{{ t('scm.merge.dirtyWarning') }}</p>
                <p v-for="note in checkoutNotes" :key="note">{{ note }}</p>
              </div>
            </div>
          </div>

          <p v-if="previewError" class="merge-note is-error">{{ previewError }}</p>
          <p v-if="error" class="merge-note is-error">{{ error }}</p>
          <div class="merge-foot">
            <label class="check-row merge-push" :class="{ 'is-disabled': !canPush || reporting }">
              <UiInput v-model="push" type="checkbox" :disabled="!canPush || reporting" />
              {{ pushLabel }}
            </label>
            <div class="modal-actions" :class="{ 'merge-conflict-actions': conflictOutcome && settled }">
              <template v-if="conflictOutcome && settled">
                <UiButton variant="danger" :disabled="running" @click="abort">{{ t('scm.merge.abortConflict') }}</UiButton>
                <UiButton :disabled="running" @click="resolveManually">{{ t('scm.merge.resolveManual') }}</UiButton>
                <UiButton variant="primary" :disabled="running" @click="resolveWithAi">{{ t('scm.aiResolve') }}</UiButton>
              </template>
              <UiButton v-else-if="settled" variant="primary" @click="emit('close')">{{ t('common.ok') }}</UiButton>
              <template v-else>
                <UiButton :disabled="running" @click="emit('close')">{{ t('common.cancel') }}</UiButton>
                <UiButton variant="primary" :disabled="!canStart" @click="start">{{ startLabel }}</UiButton>
              </template>
            </div>
          </div>
    </section>
  </UiModal>
</template>
