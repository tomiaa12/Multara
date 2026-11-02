<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type { GitRepositoryStatus, Workspace } from "../../domain/types";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import { syncAction } from "../workbench/scm-branch";
import { workspaceMode, workspaceModeLabel } from "../workbench/workspace-mode";

export type DeleteWorkspaceChoice = {
  workspaceId: string;
  deleteLocalBranch: boolean;
  deleteRemoteBranch: boolean;
  deleteCopyDirectory: boolean;
};

const PROTECTED_BRANCHES = new Set(["main", "master"]);

const props = defineProps<{
  projectId: string;
  projectName: string;
  workspaces: Workspace[];
}>();

const emit = defineEmits<{
  cancel: [];
  confirm: [choices: DeleteWorkspaceChoice[]];
  openWorkbench: [];
}>();

const choices = reactive<Record<string, DeleteWorkspaceChoice>>({});
const statuses = ref<Record<string, GitRepositoryStatus>>({});
const statusError = ref("");
const syncBusyId = ref("");
const actionError = ref("");

function isProtected(branch: string | null | undefined) {
  return Boolean(branch && PROTECTED_BRANCHES.has(branch));
}

function resetChoices() {
  for (const key of Object.keys(choices)) delete choices[key];
  for (const workspace of props.workspaces) {
    choices[workspace.id] = {
      workspaceId: workspace.id,
      deleteLocalBranch: false,
      deleteRemoteBranch: false,
      deleteCopyDirectory: false,
    };
  }
}

watch(
  () => props.workspaces.map((item) => item.id).join("\0"),
  resetChoices,
  { immediate: true },
);

function deleteHint(workspace: Workspace) {
  const mode = workspaceMode(workspace);
  if (mode === "direct") return t("dashboard.deleteHint.direct");
  if (mode === "worktree") return t("dashboard.deleteHint.worktree");
  if (workspace.isolationMode === "copy") return t("dashboard.deleteHint.folderCopy");
  return t("dashboard.deleteHint.folderDirect");
}

function statusOf(workspaceId: string) {
  return statuses.value[workspaceId] ?? null;
}

function isDirty(status: GitRepositoryStatus | null) {
  return Boolean(status?.changes?.length);
}

function looksLikeConflict(message: string) {
  return /conflict|冲突|CONFLICT|未合并|merging/i.test(message);
}

async function loadStatuses() {
  statusError.value = "";
  try {
    const list = await backend.listProjectGitStatus(props.projectId);
    const next: Record<string, GitRepositoryStatus> = {};
    for (const item of list) next[item.workspaceId] = item;
    statuses.value = next;
  } catch (cause) {
    statusError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function runSync(workspace: Workspace) {
  const status = statusOf(workspace.id);
  if (!status) return;
  const action = syncAction(status);
  if (!action || syncBusyId.value) return;
  syncBusyId.value = workspace.id;
  actionError.value = "";
  try {
    const next = action.kind === "publish"
      ? await backend.gitPushWorkspace(workspace.id)
      : await backend.gitSyncWorkspace(workspace.id);
    statuses.value = { ...statuses.value, [workspace.id]: next };
    if (next.merging) {
      actionError.value = t("dashboard.deleteConflictGotoWorkbench");
      emit("openWorkbench");
    }
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    try {
      const list = await backend.listProjectGitStatus(props.projectId);
      const refreshed = list.find((item) => item.workspaceId === workspace.id);
      if (refreshed) statuses.value = { ...statuses.value, [workspace.id]: refreshed };
      if (refreshed?.merging || looksLikeConflict(message)) {
        actionError.value = t("dashboard.deleteConflictGotoWorkbench");
        emit("openWorkbench");
        return;
      }
    } catch {
      /* keep original message */
    }
    actionError.value = message;
  } finally {
    syncBusyId.value = "";
  }
}

function openWorkbenchForConflict() {
  actionError.value = t("dashboard.deleteConflictGotoWorkbench");
  emit("openWorkbench");
}

const rows = computed(() =>
  props.workspaces.map((workspace) => {
    const mode = workspaceMode(workspace);
    const branch = workspace.branch?.trim() || "";
    const protectedBranch = isProtected(branch);
    const status = statusOf(workspace.id);
    const sync = status ? syncAction(status) : null;
    return {
      workspace,
      mode,
      modeLabel: workspaceModeLabel(workspace),
      branch,
      protectedBranch,
      canDeleteLocal: mode === "worktree" && Boolean(branch) && !protectedBranch,
      canDeleteRemote:
        (mode === "worktree" || mode === "direct") && Boolean(branch) && !protectedBranch,
      canDeleteCopy: mode === "folder" && workspace.isolationMode === "copy",
      hint: deleteHint(workspace),
      dirty: isDirty(status),
      merging: Boolean(status?.merging),
      sync,
      changeCount: status?.changes?.length ?? 0,
    };
  }),
);

function submit() {
  emit(
    "confirm",
    props.workspaces.map((workspace) => choices[workspace.id] ?? {
      workspaceId: workspace.id,
      deleteLocalBranch: false,
      deleteRemoteBranch: false,
      deleteCopyDirectory: false,
    }),
  );
}

onMounted(() => {
  void loadStatuses();
});

watch(
  () => props.projectId,
  () => {
    void loadStatuses();
  },
);
</script>

<template>
  <UiModal @click.self="emit('cancel')">
    <div class="modal delete-project-dialog" role="alertdialog" aria-modal="true">
      <div class="delete-project-intro">
        <h2>{{ t("dashboard.delete") }}</h2>
        <p>{{ t("dashboard.deleteConfirm", { name: projectName }) }}</p>
      </div>

      <div v-if="rows.length" class="delete-project-list vs-scroll">
        <section v-for="row in rows" :key="row.workspace.id" class="delete-project-row">
          <header>
            <strong>{{ row.workspace.name }}</strong>
            <span class="workspace-mode-pill" :class="`pill-${row.mode}`">{{ row.modeLabel }}</span>
            <UiTooltip
              v-if="row.sync"
              :content="syncBusyId === row.workspace.id ? t('scm.syncing') : row.sync.tooltip"
            >
              <button
                type="button"
                class="delete-project-sync"
                :disabled="Boolean(syncBusyId) || row.merging"
                @click="runSync(row.workspace)"
              >
                <Codicon
                  :name="syncBusyId === row.workspace.id ? 'sync' : row.sync.icon"
                  :spin="syncBusyId === row.workspace.id"
                  :size="16"
                />
                <span v-if="row.sync.text">{{ row.sync.text }}</span>
              </button>
            </UiTooltip>
          </header>
          <p class="delete-project-meta">
            <template v-if="row.branch">{{ t("dashboard.deleteBranch", { branch: row.branch }) }}</template>
            <template v-else>{{ t("dashboard.deleteNoBranch") }}</template>
          </p>
          <p class="delete-project-hint">{{ row.hint }}</p>
          <p v-if="row.dirty" class="delete-project-warn">
            {{ t("dashboard.deleteUncommitted", { count: row.changeCount }) }}
          </p>
          <p v-if="row.merging" class="delete-project-warn">
            {{ t("dashboard.deleteHasConflict") }}
            <button type="button" class="delete-project-link" @click="openWorkbenchForConflict">
              {{ t("dashboard.deleteOpenWorkbench") }}
            </button>
          </p>
          <label v-if="row.canDeleteLocal" class="check-row">
            <input v-model="choices[row.workspace.id].deleteLocalBranch" type="checkbox" class="ui-checkbox" />
            <span>{{ t("dashboard.deleteLocalBranch") }}</span>
          </label>
          <label v-if="row.canDeleteRemote" class="check-row">
            <input v-model="choices[row.workspace.id].deleteRemoteBranch" type="checkbox" class="ui-checkbox" />
            <span>{{ t("dashboard.deleteRemoteBranch") }}</span>
          </label>
          <label v-if="row.canDeleteCopy" class="check-row">
            <input v-model="choices[row.workspace.id].deleteCopyDirectory" type="checkbox" class="ui-checkbox" />
            <span>{{ t("dashboard.deleteCopyDirectory") }}</span>
          </label>
          <p v-if="row.protectedBranch" class="delete-project-warn">{{ t("dashboard.deleteProtectedBranch") }}</p>
          <p v-else-if="row.mode === 'direct' && row.branch" class="delete-project-warn">
            {{ t("dashboard.deleteDirectLocalDisabled") }}
          </p>
        </section>
      </div>
      <p v-else class="delete-project-hint">{{ t("dashboard.deleteNoWorkspaces") }}</p>

      <p v-if="statusError" class="delete-project-warn">{{ statusError }}</p>
      <p v-if="actionError" class="delete-project-warn">{{ actionError }}</p>

      <div class="modal-actions">
        <UiButton @click="emit('cancel')">{{ t("common.cancel") }}</UiButton>
        <UiButton variant="danger" :disabled="Boolean(syncBusyId)" @click="submit">
          {{ t("common.delete") }}
        </UiButton>
      </div>
    </div>
  </UiModal>
</template>
