<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import type { GitRepositoryStatus, Workspace } from "../../domain/types";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import { hasMoreBelow } from "../ui/scroll-fade";
import { syncAction } from "../workbench/scm-branch";
import {
  buildWorkspaceDeleteRow,
  emptyChoice,
  looksLikeConflict,
  type WorkspaceDeleteChoice,
} from "./workspace-delete-model";

const props = withDefaults(
  defineProps<{
    projectId: string;
    workspaces: Workspace[];
    /** 每行右侧显示单独删除 */
    rowDelete?: boolean;
  }>(),
  { rowDelete: false },
);

const emit = defineEmits<{
  openWorkbench: [];
  remove: [choice: WorkspaceDeleteChoice];
}>();

const choices = reactive<Record<string, WorkspaceDeleteChoice>>({});
const statuses = ref<Record<string, GitRepositoryStatus>>({});
const statusError = ref("");
const syncBusyId = ref("");
const actionError = ref("");
const bodyEl = ref<HTMLElement | null>(null);
const moreBelow = ref(false);
let bodyObserver: ResizeObserver | undefined;

function updateScrollFade() {
  moreBelow.value = hasMoreBelow(bodyEl.value);
}

function resetChoices() {
  for (const key of Object.keys(choices)) delete choices[key];
  for (const workspace of props.workspaces) {
    choices[workspace.id] = emptyChoice(workspace.id);
  }
}

watch(
  () => props.workspaces.map((item) => item.id).join("\0"),
  resetChoices,
  { immediate: true },
);

function statusOf(workspaceId: string) {
  return statuses.value[workspaceId] ?? null;
}

const rows = computed(() =>
  props.workspaces.map((workspace) => buildWorkspaceDeleteRow(workspace, statusOf(workspace.id))),
);

const localIds = computed(() =>
  rows.value.filter((row) => row.canDeleteLocal).map((row) => row.workspace.id),
);
const remoteIds = computed(() =>
  rows.value.filter((row) => row.canDeleteRemote).map((row) => row.workspace.id),
);

const allLocal = computed({
  get: () =>
    localIds.value.length > 0 &&
    localIds.value.every((id) => choices[id]?.deleteLocalBranch),
  set: (value: boolean) => {
    for (const id of localIds.value) {
      if (choices[id]) choices[id].deleteLocalBranch = value;
    }
  },
});

const allRemote = computed({
  get: () =>
    remoteIds.value.length > 0 &&
    remoteIds.value.every((id) => choices[id]?.deleteRemoteBranch),
  set: (value: boolean) => {
    for (const id of remoteIds.value) {
      if (choices[id]) choices[id].deleteRemoteBranch = value;
    }
  },
});

const localPartial = computed(
  () =>
    localIds.value.some((id) => choices[id]?.deleteLocalBranch) &&
    !allLocal.value,
);
const remotePartial = computed(
  () =>
    remoteIds.value.some((id) => choices[id]?.deleteRemoteBranch) &&
    !allRemote.value,
);

async function loadStatuses() {
  statusError.value = "";
  try {
    const list = await backend.listProjectGitStatus(props.projectId);
    const next: Record<string, GitRepositoryStatus> = {};
    for (const item of list) next[item.workspaceId] = item;
    statuses.value = next;
  } catch (cause) {
    statusError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    void nextTick(updateScrollFade);
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
    let next: GitRepositoryStatus;
    if (action.kind === "publish") {
      next = await backend.gitPushWorkspace(workspace.id);
    } else if (status.behind > 0 && !status.ahead) {
      next = await backend.gitPullWorkspace(workspace.id);
    } else {
      next = await backend.gitSyncWorkspace(workspace.id);
    }
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
    void nextTick(updateScrollFade);
  }
}

function openWorkbenchForConflict() {
  actionError.value = t("dashboard.deleteConflictGotoWorkbench");
  emit("openWorkbench");
}

function choiceOf(workspaceId: string) {
  return choices[workspaceId] ?? emptyChoice(workspaceId);
}

function emitRemove(workspaceId: string) {
  emit("remove", { ...choiceOf(workspaceId) });
}

function snapshotChoices(): WorkspaceDeleteChoice[] {
  return props.workspaces.map((workspace) => ({
    ...choiceOf(workspace.id),
  }));
}

onMounted(() => {
  void loadStatuses();
  bodyObserver = new ResizeObserver(() => updateScrollFade());
  void nextTick(() => {
    if (bodyEl.value) bodyObserver?.observe(bodyEl.value);
  });
});

onBeforeUnmount(() => {
  bodyObserver?.disconnect();
  bodyObserver = undefined;
});

watch(
  () => props.projectId,
  () => {
    void loadStatuses();
  },
);

watch(rows, () => nextTick(updateScrollFade));

defineExpose({
  choices,
  rows,
  syncBusyId,
  allLocal,
  allRemote,
  localPartial,
  remotePartial,
  localIds,
  remoteIds,
  actionError,
  statusError,
  snapshotChoices,
  loadStatuses,
});
</script>

<template>
  <div class="ds-scroll-fade" :class="{ 'has-more-below': moreBelow }">
    <div ref="bodyEl" class="modal-body vs-scroll workspace-delete-body" @scroll="updateScrollFade">
      <div v-if="rows.length" class="delete-project-list">
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
                @click.stop="runSync(row.workspace)"
              >
                <Codicon
                  :name="syncBusyId === row.workspace.id ? 'sync' : row.sync.icon"
                  :spin="syncBusyId === row.workspace.id"
                  :size="16"
                />
                <span v-if="row.sync.text">{{ row.sync.text }}</span>
              </button>
            </UiTooltip>
            <UiButton
              v-if="rowDelete"
              class="icon-button delete-project-row-delete"
              size="icon"
              variant="danger"
              :tip="t('dashboard.removeWorkspace')"
              :disabled="Boolean(syncBusyId)"
              @click.stop="emitRemove(row.workspace.id)"
            >
              <Codicon name="trash" :size="14" />
            </UiButton>
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
          <div v-if="row.canDeleteLocal || row.canDeleteRemote" class="delete-project-branch-opts">
            <label v-if="row.canDeleteLocal" class="check-row">
              <input
                v-model="choices[row.workspace.id].deleteLocalBranch"
                type="checkbox"
                class="ui-checkbox"
              />
              <span>{{ t("dashboard.deleteLocalBranch") }}</span>
            </label>
            <label v-if="row.canDeleteRemote" class="check-row">
              <input
                v-model="choices[row.workspace.id].deleteRemoteBranch"
                type="checkbox"
                class="ui-checkbox"
              />
              <span>{{ t("dashboard.deleteRemoteBranch") }}</span>
            </label>
          </div>
          <label v-if="row.canDeleteCopy" class="check-row">
            <input
              v-model="choices[row.workspace.id].deleteCopyDirectory"
              type="checkbox"
              class="ui-checkbox"
            />
            <span>{{ t("dashboard.deleteCopyDirectory") }}</span>
          </label>
          <p v-if="row.protectedBranch" class="delete-project-warn">
            {{ t("dashboard.deleteProtectedBranch") }}
          </p>
          <p v-else-if="row.mode === 'direct' && row.branch" class="delete-project-warn">
            {{ t("dashboard.deleteDirectLocalDisabled") }}
          </p>
        </section>
      </div>
      <p v-else class="delete-project-hint">{{ t("dashboard.deleteNoWorkspaces") }}</p>

      <p v-if="statusError" class="delete-project-warn">{{ statusError }}</p>
      <p v-if="actionError" class="delete-project-warn">{{ actionError }}</p>
    </div>
  </div>
</template>
