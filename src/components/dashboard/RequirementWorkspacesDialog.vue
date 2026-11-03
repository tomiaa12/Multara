<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { X } from "@lucide/vue";
import type { Workspace } from "../../domain/types";
import { confirmDialog } from "../ui/confirm-dialog";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import {
  detachWorkspacePrompt,
  workspaceMode,
} from "../workbench/workspace-mode";
import WorkspaceDeletePanel from "./WorkspaceDeletePanel.vue";
import type { WorkspaceDeleteChoice } from "./workspace-delete-model";

const props = defineProps<{
  projectId: string;
  projectName: string;
  workspaces: Workspace[];
}>();

const emit = defineEmits<{
  close: [];
  openWorkbench: [];
  changed: [];
}>();

const store = useAppStore();
const panel = ref<InstanceType<typeof WorkspaceDeletePanel> | null>(null);
const busyId = ref("");
const error = ref("");

const title = computed(() =>
  t("dashboard.workspacesManageTitle", { name: props.projectName }),
);

async function confirmRemove(choice: WorkspaceDeleteChoice) {
  const workspace = props.workspaces.find((item) => item.id === choice.workspaceId);
  if (!workspace || busyId.value) return;
  const row = panel.value?.rows.find((item) => item.workspace.id === workspace.id);
  const dirtyCount = row ? (row.dirty ? row.changeCount : 0) : null;
  const sharedCheckout = (store.data?.workspaces ?? []).some(
    (item) => item.id !== workspace.id && item.workPath === workspace.workPath,
  );
  const message = detachWorkspacePrompt({
    name: workspace.name,
    mode: workspaceMode(workspace),
    isolationMode: workspace.isolationMode,
    branch: row?.branch || workspace.branch || "",
    dirtyCount,
    unsavedCount: 0,
    sharedCheckout,
  });
  if (!(await confirmDialog(message, { danger: true, type: "warning" }))) return;

  if (row?.dirty) {
    const dirtyMessage = t("dashboard.removeWorkspaceDirtyConfirm", {
      name: workspace.name,
      count: row.changeCount,
    });
    if (!(await confirmDialog(dirtyMessage, { danger: true, type: "warning" }))) return;
  }

  const branchBits: string[] = [];
  if (choice.deleteLocalBranch) branchBits.push(t("dashboard.deleteLocalBranch"));
  if (choice.deleteRemoteBranch) branchBits.push(t("dashboard.deleteRemoteBranch"));
  if (choice.deleteCopyDirectory) branchBits.push(t("dashboard.deleteCopyDirectory"));
  if (branchBits.length) {
    const branchMessage = t("dashboard.removeWorkspaceBranchConfirm", {
      name: workspace.name,
      options: branchBits.join("、"),
    });
    if (!(await confirmDialog(branchMessage, { danger: true, type: "warning" }))) return;
  }

  void runRemove(choice);
}

async function runRemove(choice: WorkspaceDeleteChoice) {
  busyId.value = choice.workspaceId;
  error.value = "";
  try {
    await store.detachWorkspace(choice.workspaceId, {
      workspaceId: choice.workspaceId,
      deleteLocalBranch: choice.deleteLocalBranch,
      deleteRemoteBranch: choice.deleteRemoteBranch,
      deleteCopyDirectory: choice.deleteCopyDirectory,
    });
    emit("changed");
    await nextTick();
    if (!props.workspaces.length) emit("close");
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busyId.value = "";
  }
}
</script>

<template>
  <UiModal @click.self="emit('close')">
    <div class="modal delete-project-dialog requirement-workspaces-dialog" role="dialog" aria-modal="true">
      <header class="modal-heading delete-project-intro">
        <div>
          <h2>{{ title }}</h2>
          <p>{{ t("dashboard.workspacesManageHint") }}</p>
        </div>
        <UiButton class="icon-button" size="icon" :tip="t('common.close')" @click="emit('close')">
          <X :size="15" />
        </UiButton>
      </header>

      <WorkspaceDeletePanel
        ref="panel"
        row-delete
        :project-id="projectId"
        :workspaces="workspaces"
        @open-workbench="emit('openWorkbench')"
        @remove="confirmRemove"
      />

      <p v-if="error" class="delete-project-warn delete-project-foot-error">{{ error }}</p>

      <div class="modal-actions">
        <UiButton @click="emit('close')">{{ t("common.close") }}</UiButton>
      </div>
    </div>
  </UiModal>
</template>
