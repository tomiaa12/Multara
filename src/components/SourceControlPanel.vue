<script setup lang="ts">
import { computed, reactive, ref, shallowRef, watch } from "vue";
import { useRouter } from "vue-router";
import type { GitRepositoryStatus, Workspace } from "../domain/types";
import UiButton from "./ui/UiButton.vue";
import UiPane from "./ui/UiPane.vue";
import UiPaneView from "./ui/UiPaneView.vue";
import ScmChangesView from "./scm/ScmChangesView.vue";
import ScmGraphView from "./scm/ScmGraphView.vue";
import ScmHistoryListView from "./scm/ScmHistoryListView.vue";
import ScmSearchCompareView from "./scm/ScmSearchCompareView.vue";
import ScmVisualHistoryView from "./scm/ScmVisualHistoryView.vue";
import ScmTreemapView from "./scm/ScmTreemapView.vue";
import MergeIntoDialog from "./scm/MergeIntoDialog.vue";
import { useAppStore } from "../stores/app";
import { parseGitHookFailure, type GitHookFailure } from "./workbench/git-hook-error";
import { confirmDialog } from "./ui/confirm-dialog";
import { t } from "../i18n";
import * as backend from "../services/backend";
import {
  PREVIEW_PROJECT_ID,
  previewGitRepository,
  previewGitRepositoryB,
} from "./workbench/preview-workspace";
import { workspaceMode } from "./workbench/workspace-mode";
import {
  canPublishBranch,
  discardConfirmMessage,
  publishSkipsStagedChanges,
  publishWithoutCommitConfirm,
  publishStagedLeftNotice,
  rememberCommitMessage,
} from "./workbench/scm-commit-action";
import { renameBranchNotice } from "./workbench/scm-branch";
import type { ScmGroupId, ScmOpenChangesPayload, ScmOpenTarget } from "./workbench/scm-groups";

const props = withDefaults(defineProps<{
  projectId: string;
  workspaces: Workspace[];
  readonly?: boolean;
  /** 列表 / 树状视图的本地存储键。只读时不读也不写。 */
  viewModeStorageKey?: string;
  /** 当前编辑器文件，供检查器历史 / 可视化抽屉使用。 */
  activePath?: string;
  activeLine?: number;
  activeWorkspaceId?: string;
}>(), {
  readonly: false,
  viewModeStorageKey: "multara.scm.viewMode",
  activePath: "",
  activeLine: 0,
  activeWorkspaceId: "",
});
const emit = defineEmits<{
  openChange: [target: ScmOpenTarget];
  openChanges: [payload: ScmOpenChangesPayload];
  openFile: [target: { workspaceId: string; path: string }];
  updated: [repositories: GitRepositoryStatus[]];
  worktreeChanged: [workspaceId: string, paths: string[]];
}>();

type HookPlanKind = "commit" | "commit-push" | "commit-sync" | "push" | "pull" | "sync";

const store = useAppStore();
const router = useRouter();
const discardAsk = ref<{
  workspaceId: string;
  files: Array<{ path: string; worktreeStatus: string }>;
  message: string;
} | null>(null);
// 只整体替换、从不原地修改，变更列表可能有几千条，不做深响应式。
const repositories = shallowRef<GitRepositoryStatus[]>([]);
/** 正在打开「合并到其他分支」弹窗的工作区 */
const mergeDialogWorkspaceId = ref("");
const mergeDialogItem = computed(() =>
  items.value.find((item) => item.workspace.id === mergeDialogWorkspaceId.value && item.repository) ?? null,
);
const loading = ref(false);
const busy = ref("");
const error = ref("");
const notice = ref("");
const hookFailure = ref<GitHookFailure | null>(null);
const hookPlan = ref<{ workspaceId: string; kind: HookPlanKind } | null>(null);
const hookLines = computed(() =>
  (hookFailure.value?.message ?? "")
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean),
);
const hookTitle = computed(() => hookLines.value[0] ?? "");
const hookDetails = computed(() => hookLines.value.slice(1));
const messages = reactive<Record<string, string>>({});
const collapsedRepos = ref(new Set<string>());
const selectedWorkspaceId = ref("");
const historyEpoch = ref(0);
const lineHistoryView = ref<{ setExpanded: (value: boolean) => void } | null>(null);
const fileHistoryView = ref<{ setExpanded: (value: boolean) => void } | null>(null);

function switchHistoryKind(kind: "file" | "line") {
  if (kind === "line") lineHistoryView.value?.setExpanded(true);
  else fileHistoryView.value?.setExpanded(true);
}
function storedViewMode(): "list" | "tree" {
  if (props.readonly) return "list";
  if (localStorage.getItem(props.viewModeStorageKey) === "tree") return "tree";
  return "list";
}

const viewMode = ref<"list" | "tree">(storedViewMode());

const items = computed(() =>
  props.workspaces.map((workspace) => ({
    workspace,
    repository: repositories.value.find((item) => item.workspaceId === workspace.id) ?? null,
  })),
);

/** 检查器工作区：默认同编辑器；固定或下拉切换后停在所选工作区（只换查看对象，不改 checkout）。 */
const inspectPinned = ref(false);
const pinnedInspectWorkspaceId = ref("");
const followInspectWorkspaceId = computed(() => props.activeWorkspaceId || selectedWorkspaceId.value);
const inspectWorkspaceId = computed(() => {
  if (inspectPinned.value && pinnedInspectWorkspaceId.value) return pinnedInspectWorkspaceId.value;
  return followInspectWorkspaceId.value;
});
const inspectRepository = computed(() =>
  repositories.value.find((item) => item.workspaceId === inspectWorkspaceId.value) ?? null,
);
const inspectWorkspace = computed(() =>
  props.workspaces.find((item) => item.id === inspectWorkspaceId.value) ?? null,
);

function toggleInspectPin() {
  if (inspectPinned.value) {
    inspectPinned.value = false;
    return;
  }
  const id = inspectWorkspaceId.value;
  if (!id) return;
  pinnedInspectWorkspaceId.value = id;
  inspectPinned.value = true;
}

function selectInspectWorkspace(id: string) {
  if (!id) return;
  pinnedInspectWorkspaceId.value = id;
  inspectPinned.value = true;
}

watch(
  () => props.workspaces.map((item) => item.id).join("\0"),
  () => {
    if (!inspectPinned.value) return;
    if (!props.workspaces.some((item) => item.id === pinnedInspectWorkspaceId.value)) {
      inspectPinned.value = false;
      pinnedInspectWorkspaceId.value = "";
    }
  },
);

watch(
  () => props.workspaces.filter((workspace) => workspace.kind === "git").map((workspace) => workspace.id).join("\0"),
  () => {
    const gitIds = props.workspaces.filter((workspace) => workspace.kind === "git").map((workspace) => workspace.id);
    if (!gitIds.includes(selectedWorkspaceId.value)) {
      selectedWorkspaceId.value = gitIds[0] ?? "";
    }
  },
  { immediate: true },
);

function setViewMode(next: "list" | "tree") {
  if (props.readonly) return;
  viewMode.value = next;
  localStorage.setItem(props.viewModeStorageKey, next);
}

let refreshTicket = 0;
let refreshQueued = false;
let refreshPromise: Promise<void> | null = null;

async function performRefresh() {
  const ticket = ++refreshTicket;
  const showLoading = repositories.value.length === 0;
  if (showLoading) loading.value = true;
  if (!hookFailure.value) error.value = "";
  try {
    if (props.projectId === PREVIEW_PROJECT_ID) {
      repositories.value = [previewGitRepository, previewGitRepositoryB];
      emit("updated", repositories.value);
      return;
    }
    const repositoriesForProject = await backend.listProjectGitStatus(props.projectId);
    if (ticket !== refreshTicket) return;
    const workspaceIds = new Set(props.workspaces.map((item) => item.id));
    repositories.value = workspaceIds.size
      ? repositoriesForProject.filter((repository) => workspaceIds.has(repository.workspaceId))
      : repositoriesForProject;
    emit("updated", repositories.value);
    historyEpoch.value += 1;
  } catch (cause) {
    if (!hookFailure.value) {
      error.value = cause instanceof Error ? cause.message : String(cause);
    }
  } finally {
    if (showLoading) loading.value = false;
  }
}

function refresh(): Promise<void> {
  refreshQueued = true;
  if (!refreshPromise) {
    refreshPromise = (async () => {
      while (refreshQueued) {
        refreshQueued = false;
        await performRefresh();
      }
    })().finally(() => {
      refreshPromise = null;
      if (refreshQueued) void refresh();
    });
  }
  return refreshPromise;
}

async function run(
  key: string,
  action: () => Promise<unknown>,
  plan?: { workspaceId: string; kind: HookPlanKind },
) {
  busy.value = key;
  error.value = "";
  notice.value = "";
  hookFailure.value = null;
  hookPlan.value = null;
  try {
    await action();
    await refresh();
  } catch (cause) {
    const parsed = parseGitHookFailure(cause);
    if (parsed && plan) {
      hookFailure.value = parsed;
      hookPlan.value = plan;
      error.value = parsed.message;
    } else {
      error.value = cause instanceof Error ? cause.message : String(cause);
    }
  } finally {
    busy.value = "";
  }
}

async function confirmDirect(workspace: Workspace, repository: GitRepositoryStatus, action: string) {
  if (workspaceMode(workspace) !== "direct") return true;
  return confirmDialog(
    t("scm.confirmDirect", { action, name: workspace.name, branch: repository.branch }),
    { type: "warning" },
  );
}

function stage(workspaceId: string, paths: string[], staged: boolean) {
  if (!paths.length) return;
  return run(`stage:${workspaceId}`, () => backend.gitStageFiles(workspaceId, paths, staged));
}

function discard(workspaceId: string, files: { path: string; worktreeStatus: string }[]) {
  if (!files.length) return;
  const message = discardConfirmMessage(files);
  if (!message) return;
  discardAsk.value = { workspaceId, files, message };
}

function confirmDiscard() {
  const ask = discardAsk.value;
  if (!ask) return;
  discardAsk.value = null;
  return run(`discard:${ask.workspaceId}`, async () => {
    await backend.gitDiscardFiles(ask.workspaceId, ask.files.map((file) => file.path));
    emit("worktreeChanged", ask.workspaceId, ask.files.map((file) => file.path));
  });
}

function openGitSettings(section: string) {
  void router.push({ path: "/settings", query: { section } });
}

async function generateMessage(workspaceId: string) {
  busy.value = `ai:${workspaceId}`;
  error.value = "";
  try {
    messages[workspaceId] = await backend.generateGitCommitMessage(workspaceId);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = "";
  }
}

async function commit(workspace: Workspace, repository: GitRepositoryStatus, next?: "push" | "sync") {
  const message = messages[workspace.id]?.trim();
  if (!message || !repository.changes.some((item) => item.staged)) return;
  if (
    workspaceMode(workspace) === "direct"
    && !(await confirmDialog(
      t("scm.confirmCommitDirect", { name: workspace.name, branch: repository.branch }),
      { type: "warning" },
    ))
  ) {
    return;
  }
  if (next === "push" && !(await confirmDirect(workspace, repository, t("scm.commitPush")))) return;
  if (next === "sync" && !(await confirmDirect(workspace, repository, t("scm.commitSync")))) return;
  const kind: HookPlanKind = next === "push" ? "commit-push" : next === "sync" ? "commit-sync" : "commit";
  return run(`commit:${workspace.id}`, async () => {
    rememberCommitMessage(workspace.id, message);
    await backend.gitCommitWorkspace(workspace.id, message);
    messages[workspace.id] = "";
    if (next === "push") await backend.gitPushWorkspace(workspace.id);
    if (next === "sync") {
      await backend.gitSyncWorkspace(workspace.id);
      emit("worktreeChanged", workspace.id, []);
    }
  }, { workspaceId: workspace.id, kind });
}

async function publishBranch(workspace: Workspace, repository: GitRepositoryStatus) {
  if (!canPublishBranch(repository)) return;
  const message = messages[workspace.id] ?? "";
  if (message.trim() && repository.changes.some((item) => item.staged)) {
    return commit(workspace, repository, "push");
  }
  const stagedCount = repository.changes.filter((item) => item.staged).length;
  if (
    publishSkipsStagedChanges(message, stagedCount)
    && !(await confirmDialog(publishWithoutCommitConfirm(stagedCount), { type: "warning" }))
  ) {
    return;
  }
  if (!(await confirmDirect(workspace, repository, t("scm.action.publish")))) return;
  return run(`push:${workspace.id}`, async () => {
    await backend.gitPushWorkspace(workspace.id);
    if (publishSkipsStagedChanges(message, stagedCount)) notice.value = publishStagedLeftNotice();
  }, { workspaceId: workspace.id, kind: "push" });
}

async function syncRepo(workspace: Workspace, repository: GitRepositoryStatus) {
  if (!(await confirmDirect(workspace, repository, t("scm.sync")))) return;
  return run(`sync:${workspace.id}`, async () => {
    await backend.gitSyncWorkspace(workspace.id);
    emit("worktreeChanged", workspace.id, []);
  }, { workspaceId: workspace.id, kind: "sync" });
}

async function pullRepo(workspace: Workspace, repository: GitRepositoryStatus) {
  if (!(await confirmDirect(workspace, repository, t("scm.pull")))) return;
  return run(`pull:${workspace.id}`, async () => {
    await backend.gitPullWorkspace(workspace.id);
    emit("worktreeChanged", workspace.id, []);
  }, { workspaceId: workspace.id, kind: "pull" });
}

async function pushRepo(workspace: Workspace, repository: GitRepositoryStatus) {
  if (!(await confirmDirect(workspace, repository, t("scm.push")))) return;
  return run(`push:${workspace.id}`, () => backend.gitPushWorkspace(workspace.id), {
    workspaceId: workspace.id,
    kind: "push",
  });
}

async function renameBranch(workspace: Workspace, repository: GitRepositoryStatus, branch: string, renameRemote = false) {
  if (!(await confirmDirect(workspace, repository, t("scm.action.rename")))) return;
  return run(`switch:${workspace.id}`, async () => {
    const report = await backend.gitRenameWorkspaceBranch(workspace.id, branch, renameRemote);
    notice.value = renameBranchNotice(report);
    const stored = store.data?.workspaces.find((item) => item.id === workspace.id);
    if (stored?.branch === repository.branch) stored.branch = branch;
    emit("worktreeChanged", workspace.id, []);
  });
}

function setUpstream(workspace: Workspace, upstream: string) {
  return run(`switch:${workspace.id}`, async () => {
    const next = await backend.gitSetBranchUpstream(workspace.id, upstream);
    notice.value = t("scm.upstreamSet", { name: next });
  });
}

async function switchBranch(
  workspace: Workspace,
  repository: GitRepositoryStatus,
  branch: string,
  create: boolean,
  from = "",
  detach = false,
) {
  const verb = detach ? t("scm.action.detachCheckout") : create ? t("scm.action.createSwitch") : t("scm.action.switchTo");
  if (!(await confirmDirect(workspace, repository, verb))) return;
  return run(`switch:${workspace.id}`, async () => {
    await backend.gitSwitchWorkspaceBranch(workspace.id, branch, create, from || null, detach);
    emit("worktreeChanged", workspace.id, []);
  });
}

function onMerged() {
  void refresh();
}

function pullMerge(workspace: Workspace) {
  const merge = store.mergeSessionFor(workspace.id);
  if (!merge) return;
  return run(`pull-merge:${workspace.id}`, async () => {
    const result = await backend.gitPullMergeSession(merge.id);
    if (result.status === "conflict") {
      notice.value = t("scm.remoteConflicts", { n: result.conflicts.length });
      return;
    }
    notice.value = result.status === "up_to_date"
      ? t("scm.remoteNoCommits")
      : t("scm.remoteMerged");
  });
}

function finishMerge(workspace: Workspace) {
  const merge = store.mergeSessionFor(workspace.id);
  if (!merge) return;
  return run(`finish-merge:${workspace.id}`, async () => {
    await store.finishMergeSession(merge.id);
  });
}

async function abortMergeSession(workspace: Workspace) {
  const merge = store.mergeSessionFor(workspace.id);
  if (!merge) return;
  if (!(await confirmDialog(
    t("scm.abortMergeConfirm", { source: merge.sourceLabel, target: merge.targetBranch }),
    { danger: true, type: "warning" },
  ))) return;
  return run(`abort-merge:${workspace.id}`, async () => {
    await store.abortMergeSession(merge.id);
    notice.value = t("scm.abortMergeDone");
  });
}

function resolveWithAi(workspace: Workspace) {
  const merge = store.mergeSessionFor(workspace.id);
  if (merge) void store.resolveMergeWithAi(merge.id);
}

async function repairHookRuntime() {
  const plan = hookPlan.value;
  if (!plan) return;
  return run(`repair:${plan.workspaceId}`, async () => {
    notice.value = await backend.gitRepairWorktreeRuntime(plan.workspaceId);
  });
}

async function skipHooksOnce() {
  const failure = hookFailure.value;
  const plan = hookPlan.value;
  if (!failure?.skippable || !plan) return;
  const id = plan.workspaceId;
  return run(`skip:${id}`, async () => {
    if (plan.kind === "push" || (plan.kind === "commit-push" && failure.operation === "push")) {
      await backend.gitPushWorkspace(id, true);
      return;
    }
    if (plan.kind === "pull" || failure.operation === "pull") {
      await backend.gitPullWorkspace(id, true);
      emit("worktreeChanged", id, []);
      return;
    }
    if (plan.kind === "sync" || failure.operation === "sync") {
      await backend.gitSyncWorkspace(id, true);
      emit("worktreeChanged", id, []);
      return;
    }
    if (failure.operation === "commit") {
      const message = messages[id]?.trim();
      if (!message) throw new Error(t("scm.emptyMessage"));
      await backend.gitCommitWorkspace(id, message, true);
      messages[id] = "";
    }
    if (plan.kind === "commit-push") await backend.gitPushWorkspace(id, true);
    if (plan.kind === "commit-sync") {
      await backend.gitSyncWorkspace(id, true);
      emit("worktreeChanged", id, []);
    }
  }, plan);
}

function setExpanded(workspaceId: string, open: boolean) {
  const next = new Set(collapsedRepos.value);
  if (open) next.delete(workspaceId);
  else next.add(workspaceId);
  collapsedRepos.value = next;
}

function openChange(workspaceId: string, payload: { group: ScmGroupId; path: string }) {
  emit("openChange", { workspaceId, path: payload.path, group: payload.group });
}

function openFile(workspaceId: string, path: string) {
  emit("openFile", { workspaceId, path });
}

watch(
  () => props.workspaces.map((item) => item.id).join("|"),
  (ids) => {
    hookFailure.value = null;
    hookPlan.value = null;
    collapsedRepos.value = new Set();
    for (const workspace of props.workspaces) {
      if (messages[workspace.id] == null) messages[workspace.id] = "";
    }
    if (ids) void refresh();
  },
  { immediate: true },
);

const graphView = ref<InstanceType<typeof ScmGraphView> | null>(null);

async function revealCommit(workspaceId: string, hash: string) {
  if (workspaceId) selectedWorkspaceId.value = workspaceId;
  await graphView.value?.revealCommit(hash);
}

defineExpose({ refresh, revealCommit });
</script>

<template>
  <UiPaneView class="source-control-panel">
    <UiPane class="scm-changes-pane" :title="t('scm.changes')" fill actions-visible storage-key="multara.scm.changesCollapsed" :min-body="120">
      <template #actions>
        <UiButton
          v-if="!readonly"
          variant="ghost"
          size="icon"
          :tip="viewMode === 'list' ? t('scm.viewFolders') : t('scm.viewFlat')"
          @click="setViewMode(viewMode === 'list' ? 'tree' : 'list')"
        >
          <Codicon v-if="viewMode === 'list'" name="list-tree" />
          <Codicon v-else name="list-flat" />
        </UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('scm.refresh')" :disabled="Boolean(busy)" @click="refresh()">
          <Codicon name="refresh" />
        </UiButton>
      </template>
    <div v-if="hookFailure" class="scm-hook">
      <div class="scm-hook-copy">
        <p class="scm-hook-title">{{ hookTitle }}</p>
        <p v-for="(line, index) in hookDetails" :key="index" class="scm-hook-line">{{ line }}</p>
      </div>
      <div class="scm-hook-options">
        <UiButton v-if="hookFailure.repairable" size="small" variant="primary" type="button" :disabled="Boolean(busy)" @click="repairHookRuntime">{{ t("scm.repairRuntime") }}</UiButton>
        <UiButton v-if="hookFailure.skippable" size="small" type="button" :disabled="Boolean(busy)" @click="skipHooksOnce">{{ t("scm.skipHooks") }}</UiButton>
      </div>
    </div>
    <UiNotice v-else-if="error" class="scm-error" tone="error" sticky :message="error" @close="error = ''" />
    <UiNotice v-else-if="notice" class="scm-notice" :message="notice" @close="notice = ''" />
    <p v-if="loading && !items.length" class="tree-empty">{{ t("scm.loadingStatus") }}</p>
    <p v-else-if="!items.length" class="tree-empty">{{ t("scm.addFirst") }}</p>

    <ScmChangesView
      v-for="item in items"
      :key="item.workspace.id"
      v-model:message="messages[item.workspace.id]"
      :expanded="!collapsedRepos.has(item.workspace.id)"
      :workspace="item.workspace"
      :repository="item.repository"
      :busy="busy"
      :loading="loading"
      :view-mode="viewMode"
      :compact="items.length === 1"
      :readonly="readonly"
      :selected="item.workspace.id === selectedWorkspaceId"
      :merge-session="store.mergeSessionFor(item.workspace.id)"
      @select="selectedWorkspaceId = item.workspace.id"
      @merge-into="mergeDialogWorkspaceId = item.workspace.id"
      @finish-merge="finishMerge(item.workspace)"
      @pull-merge="pullMerge(item.workspace)"
      @abort-merge-session="abortMergeSession(item.workspace)"
      @resolve-with-ai="resolveWithAi(item.workspace)"
      @update:expanded="setExpanded(item.workspace.id, $event)"
      @primary="item.repository && commit(item.workspace, item.repository)"
      @commit="(next) => item.repository && commit(item.workspace, item.repository, next)"
      @publish="item.repository && publishBranch(item.workspace, item.repository)"
      @sync="item.repository && syncRepo(item.workspace, item.repository)"
      @fetch="item.repository && run(`fetch:${item.workspace.id}`, () => backend.gitFetchWorkspace(item.workspace.id))"
      @pull="item.repository && pullRepo(item.workspace, item.repository)"
      @push="item.repository && pushRepo(item.workspace, item.repository)"
      @generate="generateMessage(item.workspace.id)"
      @open-settings="openGitSettings"
      @refresh="refresh()"
      @toggle-view="setViewMode(viewMode === 'list' ? 'tree' : 'list')"
      @switch-branch="(branch, create, from, detach) => item.repository && switchBranch(item.workspace, item.repository, branch, create, from, detach)"
      @rename-branch="(branch, renameRemote) => item.repository && renameBranch(item.workspace, item.repository, branch, renameRemote)"
      @set-upstream="(upstream) => setUpstream(item.workspace, upstream)"
      @open="openChange(item.workspace.id, $event)"
      @open-file="openFile(item.workspace.id, $event)"
      @stage="stage(item.workspace.id, $event.paths, $event.staged)"
      @discard="discard(item.workspace.id, $event)"
      @error="error = $event"
    />
    </UiPane>

    <ScmGraphView
      v-if="!readonly && workspaces.some((item) => item.kind === 'git')"
      ref="graphView"
      :workspaces="workspaces"
      :workspace-id="selectedWorkspaceId"
      :epoch="historyEpoch"
      :live="projectId !== PREVIEW_PROJECT_ID"
      :branch="repositories.find((item) => item.workspaceId === selectedWorkspaceId)?.branch"
      :upstream="repositories.find((item) => item.workspaceId === selectedWorkspaceId)?.upstream"
      @open="emit('openChange', $event)"
      @open-changes="emit('openChanges', $event)"
      @error="error = $event"
      @updated="refresh()"
    />

    <!-- GitLens Inspect：搜索与比较 + 行/文件历史 + 可视化历史 + 矩形图 -->
    <template v-if="!readonly && inspectWorkspaceId">
      <ScmSearchCompareView
        :title="t('scm.searchCompare.title')"
        storage-key="multara.scm.searchCompareCollapsed"
        size-key="multara.scm.searchCompareSize"
        :workspace-id="inspectWorkspaceId"
        :repository="inspectRepository"
        :epoch="historyEpoch"
        @open="emit('openChange', $event)"
        @open-changes="emit('openChanges', $event)"
      />
      <ScmHistoryListView
        ref="lineHistoryView"
        kind="line"
        :title="t('scm.inspect.lineHistory')"
        storage-key="multara.scm.lineHistoryCollapsed"
        size-key="multara.scm.lineHistorySize"
        :workspace-id="inspectWorkspaceId"
        :path="activePath"
        :line="activeLine"
        :epoch="historyEpoch"
        :live="projectId !== PREVIEW_PROJECT_ID"
        @open="emit('openChange', $event)"
        @open-file="(path) => emit('openFile', { workspaceId: inspectWorkspaceId, path })"
        @switch-kind="switchHistoryKind"
      />
      <ScmHistoryListView
        ref="fileHistoryView"
        kind="file"
        :title="t('scm.inspect.fileHistory')"
        storage-key="multara.scm.fileHistoryCollapsed"
        size-key="multara.scm.fileHistorySize"
        :workspace-id="inspectWorkspaceId"
        :path="activePath"
        :line="activeLine"
        :epoch="historyEpoch"
        :live="projectId !== PREVIEW_PROJECT_ID"
        @open="emit('openChange', $event)"
        @open-file="(path) => emit('openFile', { workspaceId: inspectWorkspaceId, path })"
        @switch-kind="switchHistoryKind"
      />
      <ScmVisualHistoryView
        :title="t('scm.inspect.visualHistory')"
        storage-key="multara.scm.visualHistoryCollapsed"
        size-key="multara.scm.visualHistorySize"
        :workspace-id="inspectWorkspaceId"
        :path="activePath"
        :epoch="historyEpoch"
        :branch="inspectRepository?.branch || ''"
        :isolation-mode="inspectWorkspace ? workspaceMode(inspectWorkspace) : ''"
        :workspaces="workspaces"
        :repositories="repositories"
        :pinned="inspectPinned"
        default-scope="repo"
        @open="emit('openChange', $event)"
        @open-file="(path) => emit('openFile', { workspaceId: inspectWorkspaceId, path })"
        @select-workspace="selectInspectWorkspace"
        @toggle-pin="toggleInspectPin"
      />
      <ScmTreemapView
        mode="files"
        :title="t('scm.inspect.fileTreemap')"
        storage-key="multara.scm.fileTreemapCollapsed"
        size-key="multara.scm.fileTreemapSize"
        :workspace-id="inspectWorkspaceId"
        :path="activePath"
        :epoch="historyEpoch"
        :branch="inspectRepository?.branch || ''"
        :isolation-mode="inspectWorkspace ? workspaceMode(inspectWorkspace) : ''"
        :workspaces="workspaces"
        :repositories="repositories"
        :pinned="inspectPinned"
        default-scope="repo"
        @open-file="(path) => emit('openFile', { workspaceId: inspectWorkspaceId, path })"
        @select-workspace="selectInspectWorkspace"
        @toggle-pin="toggleInspectPin"
      />
      <ScmTreemapView
        mode="commits"
        :title="t('scm.inspect.commitTreemap')"
        storage-key="multara.scm.commitTreemapCollapsed"
        size-key="multara.scm.commitTreemapSize"
        :workspace-id="inspectWorkspaceId"
        :path="activePath"
        :epoch="historyEpoch"
        :branch="inspectRepository?.branch || ''"
        :isolation-mode="inspectWorkspace ? workspaceMode(inspectWorkspace) : ''"
        :workspaces="workspaces"
        :repositories="repositories"
        :pinned="inspectPinned"
        default-scope="repo"
        @open-file="(path) => emit('openFile', { workspaceId: inspectWorkspaceId, path })"
        @select-workspace="selectInspectWorkspace"
        @toggle-pin="toggleInspectPin"
      />
    </template>

    <MergeIntoDialog
      v-if="mergeDialogItem?.repository"
      :workspace="mergeDialogItem.workspace"
      :repository="mergeDialogItem.repository"
      @close="mergeDialogWorkspaceId = ''"
      @merged="onMerged"
    />

    <UiModal v-if="discardAsk" @click.self="discardAsk = null">
      <div class="modal" role="dialog" aria-modal="true" :aria-label="t('scm.discardConfirmTitle')">
        <h2>{{ t("scm.discardConfirmTitle") }}</h2>
        <p>{{ discardAsk.message }}</p>
        <div class="modal-actions">
          <UiButton @click="discardAsk = null">{{ t("common.cancel") }}</UiButton>
          <UiButton variant="danger" @click="confirmDiscard">{{ t("scm.discard") }}</UiButton>
        </div>
      </div>
    </UiModal>
  </UiPaneView>
</template>
