<script setup lang="ts">
/*
 * 合并到其他分支：把当前工作区已提交的内容按顺序合到选中的目标分支，不切换当前分支。
 * 无冲突的在后台直接完成；有冲突的排到最后，一次只开一个临时 Worktree。
 */
import { computed, onMounted, ref } from "vue";
import { X } from "@lucide/vue";
import type { GitRepositoryStatus, MergeIntoResult, MergePreview, Workspace } from "../../domain/types";
import * as backend from "../../services/backend";
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
  { id: MERGE_FILTER_ALL, label: "全部" },
  { id: MERGE_FILTER_RECENT, label: "常用" },
  { id: MERGE_FILTER_SELECTED, label: "已选", count: picked.value.length, pinCount: true },
]);
const actionable = computed(() => picked.value.filter((item) => item.preview && canMergePlan(item.preview.plan)));
const remoteCount = computed(() => picked.value.filter((item) => item.preview?.remote).length);
const canPush = computed(() => remoteCount.value > 0);
const pushLabel = computed(() => {
  if (!picked.value.length) return "合并后推送到各目标的远程";
  return `合并后推送到各目标的远程（${remoteCount.value}/${picked.value.length} 有远程）`;
});
const loadingPreview = computed(() => picked.value.some((item) => item.loading));
const canStart = computed(() => actionable.value.length > 0 && !loadingPreview.value && !running.value);
const startLabel = computed(() => {
  if (running.value) return "正在合并…";
  if (actionable.value.length) return `合并到 ${actionable.value.length} 个分支`;
  return "开始合并";
});
const checkoutNotes = computed(() => picked.value.flatMap((item) => {
  const path = item.preview?.checkedOutAt;
  if (!path) return [];
  return [`${item.preview?.branch || item.name} 正在 ${path} 检出：那里干净时一起快进，否则只推送远程。`];
}));
const conflictOutcome = computed(() => outcomes.value.find((item) => item.status === "conflict" && item.result?.mergeSession) ?? null);
const finishedCount = computed(() => outcomes.value.filter((item) => item.status === "merged" || item.status === "up_to_date").length);
const resultTitle = computed(() => {
  const conflicts = outcomes.value.filter((item) => item.status === "conflict").length;
  if (conflicts) return `合并完成 · ${conflicts} 个分支需要处理冲突`;
  if (outcomes.value.some((item) => item.status === "error")) return "合并未完成";
  return "合并完成";
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
    item.error = preview.target ? "" : "找不到这个分支";
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
      <section class="modal merge-dialog" role="dialog" aria-label="合并到其他分支">
        <div class="modal-heading">
          <h2>{{ outcomes.length ? resultTitle : "合并到其他分支" }}</h2>
          <UiButton
            v-if="!conflictOutcome"
            class="icon-button"
            size="icon"
            title="关闭"
            :disabled="running"
            @click="emit('close')"
          >
            <X :size="15" />
          </UiButton>
        </div>

        <template v-if="!outcomes.length">
          <div class="merge-source">
            <div class="merge-source-main">
              <span class="merge-chip">当前</span>
              <strong>{{ sourceLabel }}</strong>
              <code v-if="sourceCommit">{{ sourceCommit }}</code>
            </div>
            <small v-if="origin">
              从 {{ origin.baseRef }} @ {{ origin.baseCommit }} 迁出 · 新增 {{ origin.commits }} 个提交 · {{ origin.files }} 个文件
            </small>
          </div>

          <div class="merge-body">
            <div class="merge-picker">
              <UiInput v-model="query" type="search" placeholder="搜索分支" aria-label="搜索分支" />
              <SegmentedTabs v-model="filter" :items="filterTabs" label="分支筛选" />
              <div class="merge-branch-list vs-scroll">
                <p v-if="!groups.length" class="merge-empty">没有匹配的分支</p>
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
              <p class="merge-picker-foot">已排除当前分支及其同名远程</p>
            </div>

            <div class="merge-queue">
              <div class="merge-queue-head">
                <strong>{{ picked.length ? `将依次合并到 ${picked.length} 个分支` : "选择要合并到的分支" }}</strong>
                <span v-if="picked.length > 1">拖动调整顺序</span>
              </div>
              <p v-if="!picked.length" class="merge-empty">从左侧勾选目标分支。无冲突的会直接完成，冲突留到最后处理。</p>
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
                  <Codicon name="gripper" :size="14" title="拖动调整顺序" />
                </li>
              </ol>
              <p v-if="picked.length" class="merge-note">
                无冲突的分支在后台直接完成；有冲突的分支会各开一个临时工作区，排在队列最后逐个处理。当前工作区不会切换。
              </p>
              <div v-if="dirty || checkoutNotes.length" class="merge-warn-box">
                <p v-if="dirty">当前有未提交的改动，不会被合并。</p>
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
              <UiButton :disabled="running" @click="emit('close')">取消</UiButton>
              <UiButton variant="primary" :disabled="!canStart" @click="start">{{ startLabel }}</UiButton>
            </div>
          </div>
        </template>

        <template v-else>
          <p class="merge-note">
            {{ sourceLabel }} → {{ outcomes.length }} 个分支 · 已完成 {{ finishedCount }}/{{ outcomes.length }}
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
                <span v-if="item.status === 'conflict'">· {{ item.result?.conflicts.length || 0 }} 个文件冲突</span>
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
              <p v-if="item.status === 'conflict'" class="merge-note">已在临时工作区开始合并，当前工作区没有变化</p>
            </article>
          </div>
          <p v-if="conflictOutcome" class="merge-note">
            冲突分支会保留为待办：解决后在源码管理点「完成合并」，会提交合并{{ conflictOutcome.result?.mergeSession?.push ? "并推送" : "" }}，再自动删除临时工作区。
          </p>
          <p v-if="error" class="merge-note is-error">{{ error }}</p>
          <div class="modal-actions" :class="{ 'merge-conflict-actions': conflictOutcome }">
            <template v-if="conflictOutcome">
              <UiButton variant="danger" :disabled="running" @click="abort">放弃冲突分支</UiButton>
              <UiButton :disabled="running" @click="resolveManually">手动解决</UiButton>
              <UiButton variant="primary" :disabled="running" @click="resolveWithAi">交给 AI 解决</UiButton>
            </template>
            <UiButton v-else variant="primary" @click="emit('close')">好</UiButton>
          </div>
        </template>
      </section>
    </div>
  </Teleport>
</template>
