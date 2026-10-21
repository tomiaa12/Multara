<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { t } from "../../i18n";
import type { GitRepositoryStatus, MergeSession, Workspace } from "../../domain/types";
import FileTypeIcon from "../FileTypeIcon.vue";
import ContextMenu, { type ContextMenuItem } from "../ui/ContextMenu.vue";
import UiButton from "../ui/UiButton.vue";
import ScmCommitBox from "./ScmCommitBox.vue";
import ScmResourceList from "./ScmResourceList.vue";
import {
  SCM_GROUP_ORDER,
  scmGroups,
  type ScmGroupId,
} from "../workbench/scm-groups";
import {
  canPublishBranch,
  canSyncChanges,
  scmBusyTitle,
  scmPrimaryAction,
} from "../workbench/scm-commit-action";
import {
  workspaceBranchName,
  workspaceMode,
  workspaceModeHint,
} from "../workbench/workspace-mode";
import {
  branchAction,
  branchNameError,
  checkoutRows,
  syncAction,
  type CheckoutRow,
} from "../workbench/scm-branch";

const message = defineModel<string>("message", { required: true });
const expanded = defineModel<boolean>("expanded", { required: true });

const props = defineProps<{
  workspace: Workspace;
  repository: GitRepositoryStatus | null;
  busy: string;
  loading: boolean;
  viewMode: "list" | "tree";
  compact: boolean;
  selected: boolean;
  /** 这个工作区是解决冲突用的临时合并 Worktree 时传入 */
  mergeSession?: MergeSession | null;
}>();

const emit = defineEmits<{
  select: [];
  primary: [];
  commit: [next?: "push" | "sync"];
  publish: [];
  sync: [];
  fetch: [];
  pull: [];
  push: [];
  abort: [];
  mergeInto: [];
  finishMerge: [];
  pullMerge: [];
  abortMergeSession: [];
  resolveWithAi: [];
  generate: [];
  openSettings: [section: string];
  refresh: [];
  toggleView: [];
  switchBranch: [branch: string, create: boolean, from: string, detach: boolean];
  renameBranch: [branch: string, renameRemote: boolean];
  setUpstream: [upstream: string];
  open: [payload: { group: ScmGroupId; path: string }];
  openFile: [path: string];
  stage: [payload: { paths: string[]; staged: boolean }];
  discard: [files: Array<{ path: string; worktreeStatus: string }>];
  error: [message: string];
}>();

const commitMenu = ref<{ x: number; y: number } | null>(null);
const moreMenu = ref<{ x: number; y: number } | null>(null);
const checkoutOpen = ref(false);
const renameRemote = ref(false);
const checkoutQuery = ref("");
const checkoutIndex = ref(0);
const checkoutInput = ref<HTMLInputElement | null>(null);
const checkoutMode = ref<"pick" | "from" | "name" | "detach" | "rename" | "upstream">("pick");
const checkoutFrom = ref("");
const checkoutError = ref("");

const groups = computed(() => {
  if (!props.repository) return [];
  const split = scmGroups(props.repository.changes);
  return SCM_GROUP_ORDER.filter((id) => split[id].length).map((id) => ({ id, files: split[id] }));
});

const action = computed(() =>
  props.repository ? scmPrimaryAction(props.repository, canCommit.value) : "commit",
);

const canCommit = computed(() =>
  Boolean(message.value.trim() && props.repository?.changes.some((item) => item.staged)),
);

const canPrimary = computed(() => {
  if (!props.repository) return false;
  if (action.value === "publish") return canPublishBranch(props.repository);
  if (action.value === "sync") return canSyncChanges(props.repository);
  return canCommit.value;
});

const commitMenuItems = computed<ContextMenuItem[]>(() => [
  { id: "commit", label: t("scm.commit"), disabled: !canCommit.value },
  { id: "commit-push", label: t("scm.commitPush"), disabled: !canCommit.value || !props.repository?.hasRemote },
  { id: "commit-sync", label: t("scm.commitSync"), disabled: !canCommit.value || !props.repository?.hasRemote || !props.repository.upstream },
]);

const branchBar = computed(() => props.repository ? branchAction(props.repository) : null);
const canCheckout = computed(() => workspaceMode(props.workspace) === "direct");
const syncBar = computed(() => props.repository ? syncAction(props.repository) : null);
const branchBusy = computed(() => props.busy.startsWith(`switch:${props.workspace.id}`));
const branchTitle = computed(() => {
  if (branchBusy.value) return t("scm.checkingOut");
  if (!canCheckout.value) return branchBar.value?.label ?? "";
  return branchBar.value?.tooltip ?? "";
});
const repoBusy = computed(() => props.busy.endsWith(`:${props.workspace.id}`));
const syncBusy = computed(() =>
  ["sync", "pull", "push"].some((kind) => props.busy.startsWith(`${kind}:${props.workspace.id}`)),
);

const moreMenuItems = computed<ContextMenuItem[]>(() => {
  const repository = props.repository;
  const items: ContextMenuItem[] = [
    { id: "pull", label: t("scm.pull"), icon: "arrow-down", disabled: !repository?.hasRemote || !repository.upstream || repository.merging },
    { id: "push", label: t("scm.push"), icon: "arrow-up", disabled: !repository?.hasRemote || repository.merging },
  ];
  if (canCheckout.value) {
    items.push({ id: "checkout", label: t("scm.checkoutTo"), icon: "git-branch", disabled: Boolean(props.busy) });
  }
  items.push(
    {
      id: "copy-branch",
      label: t("scm.copyBranch"),
      icon: "copy",
      disabled: !workspaceBranchName(props.workspace, repository?.branch),
    },
    {
      id: "rename-branch",
      label: t("scm.renameBranch"),
      icon: "edit",
      disabled: Boolean(props.busy) || !repository?.branch || repository.merging || Boolean(props.mergeSession),
    },
    {
      id: "set-upstream",
      label: t("scm.changeUpstream"),
      icon: "git-branch",
      disabled: Boolean(props.busy) || !repository?.branch || !repository.hasRemote || repository.merging,
    },
    { id: "fetch", label: t("scm.fetch"), icon: "cloud-download", disabled: !repository?.hasRemote || repository.merging },
    { type: "sep" },
    {
      id: "merge-into",
      label: t("scm.mergeInto"),
      icon: "git-merge",
      disabled: !repository || repository.merging || Boolean(props.busy) || Boolean(props.workspace.sessionId),
    },
    { type: "sep" },
    { id: "git-settings", label: t("scm.gitSettings"), icon: "gear" },
  );
  return items;
});

const mergeTargetLabel = computed(() => {
  const merge = props.mergeSession;
  if (!merge) return "";
  return merge.targetRemote ? `${merge.targetRemote}/${merge.targetBranch}` : merge.targetBranch;
});
const unresolvedCount = computed(() => groups.value.find((group) => group.id === "merge")?.files.length ?? 0);

const checkoutList = computed(() => {
  if (!props.repository) return [] as CheckoutRow[];
  if (checkoutMode.value === "name" || checkoutMode.value === "rename") return [];
  if (checkoutMode.value === "upstream") {
    const upstream = props.repository.upstream;
    const needle = checkoutQuery.value.trim().toLowerCase();
    const current: CheckoutRow[] = [];
    if (!needle || upstream?.toLowerCase().includes(needle)) {
      current.push(
        { type: "sep", label: t("scm.currentUpstream") },
        upstream
          ? { type: "branch", name: upstream, remote: true }
          : { type: "note", label: t("scm.noUpstream") },
      );
    }
    const remotes = checkoutRows(props.repository, checkoutQuery.value).filter(
      (row) => (row.type === "branch" && row.remote && row.name !== upstream)
        || (row.type === "sep" && row.label === t("scm.remoteBranches")),
    );
    return [...current, ...remotes];
  }
  if (checkoutMode.value === "from" || checkoutMode.value === "detach") {
    return checkoutRows(props.repository, checkoutQuery.value).filter((row) => row.type !== "command");
  }
  return checkoutRows(props.repository, checkoutQuery.value);
});

const checkoutChoices = computed(() =>
  checkoutList.value.filter((row) => row.type === "branch" || row.type === "command"),
);

const checkoutPlaceholder = computed(() => {
  if (checkoutMode.value === "upstream") return t("scm.pickUpstream");
  if (checkoutMode.value === "rename") return t("scm.newBranchName");
  if (checkoutMode.value === "name") return t("scm.branchName");
  if (checkoutMode.value === "from") return t("scm.pickStartRef");
  if (checkoutMode.value === "detach") return t("scm.pickDetach");
  return t("scm.pickCheckout");
});

function openAt(event: MouseEvent, target: "commit" | "more") {
  event.preventDefault();
  event.stopPropagation();
  const button = event.currentTarget as HTMLElement;
  const rect = button.getBoundingClientRect();
  const point = { x: rect.left, y: rect.bottom + 4 };
  commitMenu.value = target === "commit" ? point : null;
  moreMenu.value = target === "more" ? point : null;
  checkoutOpen.value = false;
}

function onCommitMenu(id: string) {
  commitMenu.value = null;
  if (id === "commit") emit("commit");
  if (id === "commit-push") emit("commit", "push");
  if (id === "commit-sync") emit("commit", "sync");
}

function onMoreMenu(id: string) {
  moreMenu.value = null;
  if (id === "pull") emit("pull");
  if (id === "push") emit("push");
  if (id === "fetch") emit("fetch");
  if (id === "checkout") openCheckout();
  if (id === "copy-branch") void copyBranchName();
  if (id === "rename-branch") openRename();
  if (id === "set-upstream") openUpstream();
  if (id === "merge-into") emit("mergeInto");
  if (id === "git-settings") emit("openSettings", "commit");
}

async function copyBranchName() {
  const branch = workspaceBranchName(props.workspace, props.repository?.branch);
  if (!branch) return;
  try {
    await navigator.clipboard.writeText(branch);
  } catch {
    emit("error", t("scm.copyFailed"));
  }
}

function openCheckout() {
  if (!canCheckout.value || !props.repository || props.busy) return;
  checkoutMode.value = "pick";
  checkoutQuery.value = "";
  checkoutIndex.value = 0;
  checkoutFrom.value = "";
  checkoutError.value = "";
  checkoutOpen.value = true;
  void nextTick(() => checkoutInput.value?.focus());
}

function closeCheckout() {
  checkoutOpen.value = false;
}

function moveCheckout(step: number) {
  const count = checkoutChoices.value.length;
  if (!count) return;
  checkoutIndex.value = (checkoutIndex.value + step + count) % count;
}

function openRename() {
  const branch = props.repository?.branch;
  if (!branch || props.busy) return;
  checkoutMode.value = "rename";
  checkoutQuery.value = branch;
  renameRemote.value = false;
  checkoutIndex.value = 0;
  checkoutFrom.value = "";
  checkoutError.value = "";
  checkoutOpen.value = true;
  void nextTick(() => {
    checkoutInput.value?.focus();
    checkoutInput.value?.select();
  });
}

function openUpstream() {
  if (!props.repository?.branch || props.busy) return;
  checkoutMode.value = "upstream";
  checkoutQuery.value = "";
  checkoutIndex.value = 0;
  checkoutError.value = "";
  checkoutOpen.value = true;
  void nextTick(() => checkoutInput.value?.focus());
}

function acceptCheckout() {
  if (checkoutMode.value === "upstream") {
    const choice = checkoutChoices.value[checkoutIndex.value];
    if (!choice || choice.type !== "branch") return;
    emit("setUpstream", choice.name);
    closeCheckout();
    return;
  }
  if (checkoutMode.value === "rename") {
    const name = checkoutQuery.value.trim();
    checkoutError.value = branchNameError(name);
    if (checkoutError.value) return;
    if (name !== props.repository?.branch) emit("renameBranch", name, renameRemote.value);
    closeCheckout();
    return;
  }
  if (checkoutMode.value === "name") {
    const name = checkoutQuery.value.trim();
    checkoutError.value = branchNameError(name);
    if (checkoutError.value) return;
    emit("switchBranch", name, true, checkoutFrom.value, false);
    closeCheckout();
    return;
  }
  const choice = checkoutChoices.value[checkoutIndex.value];
  if (!choice) return;
  if (choice.type === "branch") {
    if (checkoutMode.value === "from") {
      checkoutFrom.value = choice.name;
      checkoutMode.value = "name";
      checkoutQuery.value = "";
      checkoutError.value = "";
      checkoutIndex.value = 0;
      void nextTick(() => checkoutInput.value?.focus());
      return;
    }
    emit("switchBranch", choice.name, false, "", checkoutMode.value === "detach");
    closeCheckout();
    return;
  }
  if (choice.id === "create") {
    const typed = checkoutQuery.value.trim();
    checkoutFrom.value = "";
    checkoutMode.value = "name";
    checkoutQuery.value = typed;
    checkoutError.value = "";
    void nextTick(() => checkoutInput.value?.focus());
    return;
  }
  if (choice.id === "create-from" || choice.id === "detach") {
    checkoutMode.value = choice.id === "detach" ? "detach" : "from";
    checkoutQuery.value = "";
    checkoutIndex.value = 0;
    checkoutError.value = "";
  }
}

watch(checkoutQuery, () => {
  checkoutIndex.value = 0;
});

function checkoutActive(row: CheckoutRow) {
  const choice = checkoutChoices.value[checkoutIndex.value];
  return choice === row;
}

function onPrimary() {
  if (action.value === "publish") emit("publish");
  else if (action.value === "sync") emit("sync");
  else emit("primary");
}
</script>

<template>
  <section
    class="scm-repository"
    :class="[{ 'is-nested': !compact, 'is-open': expanded, 'is-selected': selected }, `is-${workspaceMode(workspace)}`]"
    @click.capture="workspace.kind === 'git' && emit('select')"
  >
    <div v-if="!compact || repository" class="scm-provider" :class="{ 'is-nested': !compact }" @click="expanded = !expanded">
      <button
        type="button"
        class="scm-repo-toggle"
        :title="workspaceModeHint(workspace)"
      >
        <Codicon name="chevron-down" class="tree-chevron" :class="{ 'is-open': expanded }" :size="16" />
        <Codicon v-if="workspace.kind === 'git'" name="source-control" :size="16" />
        <FileTypeIcon v-else :name="workspace.name" directory :open="expanded" :size="16" />
        <span class="scm-repo-name">{{ workspace.name }}</span>
        <span v-if="workspace.kind === 'git'" class="scm-repo-kind">Git</span>
      </button>
      <div v-if="repository && branchBar" class="scm-repo-actions">
        <button
          type="button"
          class="scm-status-action"
          :class="{ 'is-static': !canCheckout }"
          :data-scm-branch="workspace.id"
          :title="branchTitle"
          :disabled="canCheckout && Boolean(busy)"
          @click.stop="openCheckout"
        >
          <Codicon v-if="branchBusy" name="loading" spin :size="16" />
          <span>{{ branchBar.label }}</span>
        </button>
        <button
          v-if="syncBar"
          type="button"
          class="scm-status-action"
          :title="syncBusy ? t('scm.syncing') : syncBar.tooltip"
          :disabled="repoBusy || repository.merging"
          @click.stop="syncBar.kind === 'publish' ? emit('publish') : emit('sync')"
        >
          <Codicon :name="syncBusy ? 'sync' : syncBar.icon" :spin="syncBusy" :size="16" />
          <span v-if="syncBar.text">{{ syncBar.text }}</span>
        </button>
        <button
          type="button"
          class="scm-status-action is-icon scm-repo-menu"
          :title="viewMode === 'list' ? t('scm.viewTree') : t('scm.viewList')"
          @click.stop="emit('toggleView')"
        >
          <Codicon :name="viewMode === 'list' ? 'list-tree' : 'list-flat'" :size="16" />
        </button>
        <button
          type="button"
          class="scm-status-action is-icon scm-repo-menu"
          :title="t('scm.commit')"
          :disabled="!canCommit || repoBusy"
          @click.stop="emit('commit')"
        >
          <Codicon name="check" :size="16" />
        </button>
        <button
          type="button"
          class="scm-status-action is-icon scm-repo-menu"
          :title="t('scm.refresh')"
          :disabled="Boolean(busy)"
          @click.stop="emit('refresh')"
        >
          <Codicon name="refresh" :size="16" />
        </button>
        <button
          type="button"
          class="scm-status-action is-icon"
          :title="t('scm.moreActions')"
          :disabled="Boolean(busy)"
          @click.stop="openAt($event, 'more')"
        >
          <Codicon name="ellipsis" :size="16" />
        </button>
      </div>
    </div>
    <div v-if="expanded" class="scm-repository-body">
      <div v-if="workspace.kind !== 'git'" class="scm-folder-card">
        {{ t("scm.notGit") }}<br />
        {{ workspaceModeHint(workspace) }}
      </div>
      <template v-else-if="repository">
        <p v-if="workspaceMode(workspace) === 'direct'" class="scm-direct-warn">
          {{ t("scm.editingRealBranch", { branch: repository.branch }) }}
        </p>
        <div v-if="mergeSession" class="scm-merge-session">
          <div class="scm-merge-session-title">
            <Codicon name="git-merge" :size="16" />
            <span>
              {{ t("scm.mergeProgress", { source: mergeSession.sourceLabel, target: mergeTargetLabel }) }}
            </span>
          </div>
          <p>
            {{ unresolvedCount ? t("scm.conflictsLeft", { count: unresolvedCount }) : t("scm.conflictsDone") }}
            · {{ t("scm.mergeCleanup", { action: mergeSession.push ? t("scm.mergeThenPush") : t("scm.mergeThenLocal") }) }}
          </p>
          <div class="scm-merge-session-actions">
            <UiButton
              size="small"
              :disabled="Boolean(busy)"
              :title="scmBusyTitle(busy, `abort-merge:${workspace.id}`, t('scm.abortMergeTitle'))"
              @click="emit('abortMergeSession')"
            >
              {{ t("scm.abortMerge") }}
            </UiButton>
            <UiButton
              v-if="mergeSession.push && !unresolvedCount"
              size="small"
              :disabled="Boolean(busy)"
              :title="scmBusyTitle(busy, `pull-merge:${workspace.id}`, t('scm.pullLatestTitle'))"
              @click="emit('pullMerge')"
            >
              {{ t("scm.pullLatest") }}
            </UiButton>
            <UiButton
              v-if="unresolvedCount"
              size="small"
              :disabled="Boolean(busy)"
              :title="t('scm.aiResolveTitle')"
              @click="emit('resolveWithAi')"
            >
              {{ t("scm.aiResolve") }}
            </UiButton>
            <UiButton
              size="small"
              variant="primary"
              :disabled="Boolean(busy)"
              :title="scmBusyTitle(busy, `finish-merge:${workspace.id}`, t('scm.finishMergeTitle'))"
              @click="emit('finishMerge')"
            >
              {{ t("scm.finishMerge") }}
            </UiButton>
          </div>
        </div>
        <ScmCommitBox
          v-if="!mergeSession"
          v-model="message"
          :workspace-id="workspace.id"
          :repository="repository"
          :action="action"
          :busy="busy"
          :can-commit="canCommit"
          :can-primary="canPrimary"
          @submit="onPrimary"
          @menu="openAt($event, 'commit')"
          @generate="emit('generate')"
          @settings="emit('openSettings', 'commit')"
        />
        <div v-if="repository.merging && !mergeSession" class="scm-merge-banner">
          <span>{{ t("scm.resolvingMerge") }}</span>
          <UiButton
            size="small"
            :disabled="Boolean(busy)"
            :title="scmBusyTitle(busy, `abort:${workspace.id}`, t('scm.abortMergeShortTitle'))"
            @click="emit('abort')"
          >
            {{ t("scm.abortMergeShort") }}
          </UiButton>
        </div>
        <ScmResourceList
          :workspace-id="workspace.id"
          :groups="groups"
          :view-mode="viewMode"
          :busy="busy"
          @open="emit('open', $event)"
          @open-file="emit('openFile', $event)"
          @stage="emit('stage', $event)"
          @discard="emit('discard', $event)"
        />
      </template>
      <p v-else-if="loading" class="tree-empty">{{ t("scm.loadingRepo") }}</p>
    </div>

    <ContextMenu v-if="commitMenu" :x="commitMenu.x" :y="commitMenu.y" :items="commitMenuItems" @select="onCommitMenu" @close="commitMenu = null" />
    <ContextMenu v-if="moreMenu" :x="moreMenu.x" :y="moreMenu.y" :items="moreMenuItems" @select="onMoreMenu" @close="moreMenu = null" />
    <Teleport to="body">
      <div v-if="checkoutOpen" class="scm-checkout-backdrop" @mousedown="closeCheckout">
      <div
        class="scm-checkout-pick"
        role="dialog"
        :aria-label="checkoutPlaceholder"
        @mousedown.stop
      >
        <input
          ref="checkoutInput"
          v-model="checkoutQuery"
          :placeholder="checkoutPlaceholder"
          :aria-label="checkoutPlaceholder"
          spellcheck="false"
          @keydown.escape.stop="closeCheckout"
          @keydown.enter.prevent="acceptCheckout"
          @keydown.down.prevent="moveCheckout(1)"
          @keydown.up.prevent="moveCheckout(-1)"
        />
        <p v-if="checkoutError" class="scm-checkout-error">{{ checkoutError }}</p>
        <label v-if="checkoutMode === 'rename' && repository?.upstream" class="scm-rename-remote">
          <input v-model="renameRemote" type="checkbox" />
          <span>{{ t("scm.renameRemoteToo") }}</span>
        </label>
        <p v-if="checkoutMode === 'rename' && repository?.upstream && !renameRemote" class="scm-rename-hint">
          {{ t("scm.renameRemoteHint") }}
        </p>
        <div class="scm-checkout-list vs-scroll">
          <template v-for="(row, index) in checkoutList" :key="`${row.type}-${index}-${row.type === 'branch' ? row.name : row.type === 'command' ? row.id : row.label}`">
            <div v-if="row.type === 'sep'" class="scm-checkout-sep">{{ row.label }}</div>
            <div v-else-if="row.type === 'note'" class="scm-checkout-note">{{ row.label }}</div>
            <button
              v-else
              type="button"
              :class="{ active: checkoutActive(row) }"
              @mouseenter="checkoutIndex = checkoutChoices.indexOf(row)"
              @click="checkoutIndex = checkoutChoices.indexOf(row); acceptCheckout()"
            >
              <Codicon :name="row.type === 'branch' ? (row.remote ? 'cloud' : 'git-branch') : row.icon" :size="16" />
              <span>{{ row.type === "branch" ? row.name : row.label }}</span>
            </button>
          </template>
        </div>
      </div>
      </div>
    </Teleport>
  </section>
</template>
