<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import WorkspacePickerFields from "./WorkspacePickerFields.vue";
import {
  boundWorkspacesOf,
  buildWorkspaceRequests,
  isAlreadyBoundPath,
  knownBranchesOf,
  validatePicker,
  type PickerItem,
} from "./workspace-picker";

const props = withDefaults(
  defineProps<{
    projectId: string;
    /** 打开时预扫并填入的绝对路径 */
    initialPaths?: string[];
  }>(),
  { initialPaths: () => [] },
);

const emit = defineEmits<{
  close: [];
  added: [];
}>();

const store = useAppStore();
const router = useRouter();
const items = ref<PickerItem[]>([]);
const busy = ref(false);
const error = ref("");
const picker = ref<InstanceType<typeof WorkspacePickerFields> | null>(null);

const bound = computed(() => boundWorkspacesOf(store.data?.workspaces ?? [], props.projectId));
const knownBranches = computed(() => knownBranchesOf(store.data?.workspaces ?? [], props.projectId));
const requirementName = computed(
  () => store.data?.projects.find((project) => project.id === props.projectId)?.name?.trim() ?? "",
);
const branchPrefix = computed(() => store.data?.settings.branchPrefix || "feat");
const alreadyAddedCount = computed(
  () => items.value.filter((item) => isAlreadyBoundPath(item.path, bound.value)).length,
);
const canSubmit = computed(
  () =>
    items.value.length > 0 &&
    alreadyAddedCount.value === 0 &&
    !busy.value &&
    !picker.value?.picking,
);

function openBranchSettings() {
  emit("close");
  void router.push({ path: "/settings", query: { section: "branch" } });
}

function submit() {
  if (!canSubmit.value) return;
  const problem = validatePicker(items.value, bound.value);
  if (problem?.code === "already") {
    error.value = t("workspace.removeAddedFirst");
    return;
  }
  if (problem?.code === "branch") {
    error.value = t("workspace.needBranchName", { name: problem.name ?? "" });
    return;
  }
  const requests = buildWorkspaceRequests(items.value, bound.value);
  if (!requests.length) return;
  store.queueAddWorkspaces(props.projectId, requests);
  emit("added");
}
</script>

<template>
  <UiModal>
    <form class="modal workspace-picker-modal" @submit.prevent="submit">
      <div class="workspace-picker-head">
        <h2>{{ t("workspace.addTitle") }}</h2>
        <p v-if="alreadyAddedCount">{{ t("workspace.alreadyCount", { count: alreadyAddedCount }) }}</p>
        <p v-else-if="items.length">{{ t("workspace.addedCount", { count: items.length }) }}</p>
        <p v-else>{{ t("workspace.addHint") }}</p>
      </div>
      <WorkspacePickerFields
        ref="picker"
        v-model="items"
        :requirement-name="requirementName"
        :bound="bound"
        :known-branches="knownBranches"
        :branch-prefix="branchPrefix"
        :initial-paths="initialPaths"
        allow-branch-settings
        @settings="openBranchSettings"
      />
      <div class="modal-actions">
        <UiButton type="button" :disabled="busy" @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton v-if="items.length" type="submit" variant="primary" :disabled="!canSubmit">
          {{ busy ? t("workspace.adding") : t("workspace.addSubmit", { count: items.length }) }}
        </UiButton>
      </div>
    </form>
  </UiModal>
</template>
