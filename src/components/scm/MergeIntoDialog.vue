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
  canMergePlan,
  mergeBranchGroups,
  mergePlanBadge,
  mergeQueueSummary,
  mergeResultLines,
  mergeRunOrder,
  rememberMergeTargets,
} from "./merge-targets";

const props = defineProps<{ workspace: Workspace; repository: GitRepositoryStatus }>();
const emit = defineEmits<{ close: []; merged: [] }>();

const store = useAppStore();
const LAST_TARGET_KEY = `multara.merge.target.${props.workspace.id}`;
const RECENT_KEY = `multara.merge.recent.${props.workspace.id}`;

interface PickedTarget {
  name: string;
  preview: NonNullable<MergePreview["target"]> | null;
  loading: boolean;
  error: string;
}

interface MergeOutcome {
  name: string;
  status: "merged" | "up_to_date" | "conflict" | "error" | "pending";
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
const finishedCount = computed(() => outcomes.value.filter((item) => item.status === "merged" || item.status === "up_to_date").length);
const resultTitle = computed(() => {
  const conflicts = outcomes.value.filter((item) => item.status === "conflict").length;
  if (conflicts) return t("scm.merge.doneConflicts", { count: conflicts });
  if (outcomes.value.some((item) => item.status === "error")) return t("scm.merge.incomplete");
  return t("scm.merge.done");
});
const blocked = computed(() => running.value || Boolean(conflictOutcome.value));

function readRecent() {
  try {
    const saved = localStorage.getItem(RECENT_KEY);
    if (saved) {
      const parsed = JSON.parse(saved);
      if (Array.isArray(parsed)) return parsed.filter((item) => typeof item === "string");
    }
    const last = localStorage.getItem(LAST_TARGET_KEY);
    return last ? [last] : [];
  } catch {
    return [];
  }
}

function knownTarget(name: string) {
  return locals.value.includes(name) || remotes.value.includes(name);
}

function defaultTarget(baseRef: string | undefined) {
  const remembered = recent.value.find((name) => knownTarget(name));
  if (remembered) return remembered;
  if (!baseRef) return null;
  if (knownTarget(`origin/${baseRef}`)) return `origin/${baseRef}`;
  return knownTarget(baseRef) ? baseRef : null;
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
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
    if (next[0]) localStorage.setItem(LAST_TARGET_KEY, next[0]);
  } catch {
    // 存不下时只在这次弹窗里有效
  }
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
  const done: MergeOutcome[] = [];
  let hitConflict = false;
  for (const item of queue) {
    if (hitConflict) {
      done.push({ name: item.name, status: "pending", result: null, error: "", pushRequested });
      continue;
    }
    try {
      const outcome = await store.mergeIntoBranch(props.workspace.id, item.name, pushRequested && Boolean(item.preview?.remote));
      const status = outcome.status === "conflict" ? "conflict" : outcome.status;
      done.push({ name: item.name, status, result: outcome, error: "", pushRequested });
      if (outcome.status === "conflict") hitConflict = true;
    } catch (cause) {
      done.push({
        name: item.name,
        status: "error",
        result: null,
        error: cause instanceof Error ? cause.message : String(cause),
        pushRequested,
      });
      break;
    }
  }
  outcomes.value = done;
  running.value = false;
  if (done.some((item) => item.status === "merged" || item.status === "up_to_date")) emit("merged");
}

const mergeId = computed(() => conflictOutcome.value?.result?.mergeSession?.id ?? "");

async function resolveWithAi() {
  if (!mergeId.value) return;
  await store.resolveMergeWithAi(mergeId.value);
  emit("close");
}

async function resolveManually() {
  if (!mergeId.value) return;
  await store.openMergeSession(mergeId.value);
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

function outcomeLines(item: MergeOutcome) {
  return mergeResultLines({
    status: item.status,
    result: item.result,
    error: item.error,
    pushRequested: item.pushRequested,
  });
}

onMounted(async () => {
  recent.value = readRecent();
  await loadSource();
  const next = defaultTarget(origin.value?.baseRef);
  if (next) toggle(next, true);
});
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop merge-backdrop" @click.self="!blocked && emit('close')">
      <section class="modal merge-dialog" role="dialog" :aria-label="t('scm.merge.title')">
        <div class="modal-heading">
          <h2>{{ outcomes.length ? resultTitle : t('scm.merge.title') }}</h2>
          <UiButton
            v-if="!conflictOutcome"
            class="icon-button"
            size="icon"
            :title="t('common.close')"
            :disabled="running"
            @click="emit('close')"
          >
            <X :size="15" />
          </UiButton>
        </div>

        <template v-if="!outcomes.length">
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
              <UiInput v-model="query" type="search" :placeholder="t('workspace.searchBranch')" :aria-label="t('workspace.searchBranch')" />
              <SegmentedTabs v-model="filter" :items="filterTabs" :label="t('scm.merge.branchFilter')" />
              <div class="merge-branch-list vs-scroll">
                <p v-if="!groups.length" class="merge-empty">{{ t('scm.merge.noMatch') }}</p>
                <section v-for="group in groups" :key="group.label || 'selected'">
                  <h3 v-if="group.label">{{ group.label }}</h3>
                  <label v-for="row in group.rows" :key="row.name" class="check-row merge-branch">
                    <UiInput
                      type="checkbox"
                      :model-value="isPicked(row.name)"
                      @update:model-value="toggle(row.name, Boolean($event))"
                    />
                    <span>{{ row.name }}</span>
                  </label>
                </section>
              </div>
              <p class="merge-picker-foot">{{ t('scm.merge.excludedHint') }}</p>
            </div>

            <div class="merge-queue">
              <div class="merge-queue-head">
                <strong>{{ picked.length ? t('scm.merge.pickCount', { count: picked.length }) : t('scm.merge.pickPrompt') }}</strong>
                <span v-if="picked.length > 1">{{ t('scm.merge.dragOrder') }}</span>
              </div>
              <p v-if="!picked.length" class="merge-empty">{{ t('scm.merge.pickHint') }}</p>
              <ol v-else>
                <li
                  v-for="(item, index) in picked"
                  :key="item.name"
                  :class="item.preview?.plan ? `is-${item.preview.plan}` : ''"
                  draggable="true"
                  @dragstart="onDragStart($event, index)"
                  @dragend="dragIndex = -1"
                  @dragover.prevent
                  @drop="onDrop(index)"
                >
                  <span class="merge-order">{{ index + 1 }}</span>
                  <div class="merge-queue-copy">
                    <b>{{ item.preview?.display || item.name }}</b>
                    <small>{{ mergeQueueSummary(item.preview, item.loading, item.error) }}</small>
                  </div>
                  <em v-if="item.preview">{{ mergePlanBadge(item.preview.plan) }}</em>
                  <Codicon name="gripper" :size="14" :title="t('scm.merge.dragOrder')" />
                </li>
              </ol>
              <p v-if="picked.length" class="merge-note">
                {{ t('scm.merge.queueNote') }}
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
            <label class="check-row merge-push" :class="{ 'is-disabled': !canPush }">
              <UiInput v-model="push" type="checkbox" :disabled="!canPush" />
              {{ pushLabel }}
            </label>
            <div class="modal-actions">
              <UiButton :disabled="running" @click="emit('close')">{{ t('common.cancel') }}</UiButton>
              <UiButton variant="primary" :disabled="!canStart" @click="start">{{ startLabel }}</UiButton>
            </div>
          </div>
        </template>

        <template v-else>
          <p class="merge-note">
            {{ t('scm.merge.progressSummary', { source: sourceLabel, count: outcomes.length, finished: finishedCount, total: outcomes.length }) }}
          </p>
          <div class="merge-results">
            <article
              v-for="item in outcomes"
              :key="item.name"
              class="merge-result"
              :class="`is-${item.status}`"
            >
              <header>
                <span class="merge-result-mark" aria-hidden="true">
                  <Codicon v-if="item.status === 'conflict' || item.status === 'error'" name="warning" :size="14" />
                  <Codicon v-else-if="item.status === 'pending'" name="clock" :size="14" />
                  <Codicon v-else name="check" :size="14" />
                </span>
                <b>{{ item.result?.targetDisplay || item.name }}</b>
                <span v-if="item.status === 'conflict'">{{ t('scm.merge.fileConflicts', { count: item.result?.conflicts.length || 0 }) }}</span>
              </header>
              <ul v-if="item.status === 'conflict'" class="merge-conflicts">
                <li v-for="file in item.result?.conflicts || []" :key="file">
                  <span :title="file">{{ file }}</span>
                </li>
              </ul>
              <p
                v-for="(line, index) in outcomeLines(item)"
                :key="`${item.name}-${index}`"
                class="merge-note"
                :class="{ 'is-warn': line.warn }"
              >
                {{ line.text }}
              </p>
              <p v-if="item.status === 'conflict'" class="merge-note">{{ t('scm.merge.conflictStarted') }}</p>
            </article>
          </div>
          <p v-if="conflictOutcome" class="merge-note">
            {{ t('scm.merge.conflictTodo', { pushSuffix: conflictOutcome.result?.mergeSession?.push ? t('scm.merge.conflictPushSuffix') : '' }) }}
          </p>
          <p v-if="error" class="merge-note is-error">{{ error }}</p>
          <div class="modal-actions" :class="{ 'merge-conflict-actions': conflictOutcome }">
            <template v-if="conflictOutcome">
              <UiButton variant="danger" :disabled="running" @click="abort">{{ t('scm.merge.abortConflict') }}</UiButton>
              <UiButton :disabled="running" @click="resolveManually">{{ t('scm.merge.resolveManual') }}</UiButton>
              <UiButton variant="primary" :disabled="running" @click="resolveWithAi">{{ t('scm.aiResolve') }}</UiButton>
            </template>
            <UiButton v-else variant="primary" @click="emit('close')">{{ t('common.ok') }}</UiButton>
          </div>
        </template>
      </section>
    </div>
  </Teleport>
</template>
