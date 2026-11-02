<script setup lang="ts">
import { computed, ref } from "vue";
import type { Workspace } from "../../domain/types";
import { t } from "../../i18n";
import WorkspaceDeletePanel from "./WorkspaceDeletePanel.vue";
import type { WorkspaceDeleteChoice } from "./workspace-delete-model";

export type DeleteWorkspaceChoice = WorkspaceDeleteChoice;

defineProps<{
  projectId: string;
  projectName: string;
  workspaces: Workspace[];
}>();

const emit = defineEmits<{
  cancel: [];
  confirm: [choices: WorkspaceDeleteChoice[]];
  openWorkbench: [];
}>();

const panel = ref<InstanceType<typeof WorkspaceDeletePanel> | null>(null);
const syncBusy = computed(() => Boolean(panel.value?.syncBusyId));

function submit() {
  emit("confirm", panel.value?.snapshotChoices() ?? []);
}
</script>

<template>
  <UiModal @click.self="emit('cancel')">
    <div class="modal delete-project-dialog" role="alertdialog" aria-modal="true">
      <header class="modal-heading delete-project-intro">
        <div>
          <h2>{{ t("dashboard.delete") }}</h2>
          <p>{{ t("dashboard.deleteConfirm", { name: projectName }) }}</p>
        </div>
      </header>

      <WorkspaceDeletePanel
        ref="panel"
        bulk-select
        :project-id="projectId"
        :workspaces="workspaces"
        @open-workbench="emit('openWorkbench')"
      >
        <template #footer>
          <UiButton @click="emit('cancel')">{{ t("common.cancel") }}</UiButton>
          <UiButton variant="danger" :disabled="syncBusy" @click="submit">
            {{ t("common.delete") }}
          </UiButton>
        </template>
      </WorkspaceDeletePanel>
    </div>
  </UiModal>
</template>
