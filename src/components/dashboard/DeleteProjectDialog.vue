<script setup lang="ts">
import { computed, ref } from "vue";
import type { Workspace } from "../../domain/types";
import { t } from "../../i18n";
import WorkspaceDeletePanel from "./WorkspaceDeletePanel.vue";
import type { WorkspaceDeleteChoice } from "./workspace-delete-model";

export type DeleteWorkspaceChoice = WorkspaceDeleteChoice;

const props = defineProps<{
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
const showBulkLocal = computed(() => (panel.value?.localIds.length ?? 0) > 0);
const showBulkRemote = computed(() => (panel.value?.remoteIds.length ?? 0) > 0);

const allLocal = computed({
  get: () => panel.value?.allLocal ?? false,
  set: (value: boolean) => {
    if (panel.value) panel.value.allLocal = value;
  },
});
const allRemote = computed({
  get: () => panel.value?.allRemote ?? false,
  set: (value: boolean) => {
    if (panel.value) panel.value.allRemote = value;
  },
});

function onLocalInput(event: Event) {
  const input = event.target as HTMLInputElement;
  allLocal.value = input.checked;
}
function onRemoteInput(event: Event) {
  const input = event.target as HTMLInputElement;
  allRemote.value = input.checked;
}

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
        :project-id="projectId"
        :workspaces="workspaces"
        @open-workbench="emit('openWorkbench')"
      />

      <div class="modal-actions delete-project-foot">
        <div class="delete-project-bulk">
          <label v-if="showBulkLocal" class="check-row">
            <input
              type="checkbox"
              class="ui-checkbox"
              :checked="allLocal"
              :indeterminate.prop="Boolean(panel?.localPartial)"
              @change="onLocalInput"
            />
            <span>{{ t("dashboard.deleteAllLocal") }}</span>
          </label>
          <label v-if="showBulkRemote" class="check-row">
            <input
              type="checkbox"
              class="ui-checkbox"
              :checked="allRemote"
              :indeterminate.prop="Boolean(panel?.remotePartial)"
              @change="onRemoteInput"
            />
            <span>{{ t("dashboard.deleteAllRemote") }}</span>
          </label>
        </div>
        <div class="delete-project-foot-end">
          <UiButton @click="emit('cancel')">{{ t("common.cancel") }}</UiButton>
          <UiButton variant="danger" :disabled="syncBusy" @click="submit">
            {{ t("common.delete") }}
          </UiButton>
        </div>
      </div>
    </div>
  </UiModal>
</template>
