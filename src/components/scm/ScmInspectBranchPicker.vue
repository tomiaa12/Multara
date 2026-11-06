<script setup lang="ts">
import { computed } from "vue";
import type { GitRepositoryStatus, Workspace } from "../../domain/types";
import { t } from "../../i18n";
import { workspaceMode } from "../workbench/workspace-mode";
import UiSelect from "../ui/UiSelect.vue";
import type { SelectOption } from "../ui/select-types";

const props = defineProps<{
  workspaceId: string;
  workspaces: Workspace[];
  repositories: GitRepositoryStatus[];
  pinned?: boolean;
}>();

const emit = defineEmits<{
  "update:workspaceId": [id: string];
}>();

const gitWorkspaces = computed(() => props.workspaces.filter((item) => item.kind === "git"));

function branchOf(workspace: Workspace): string {
  const repo = props.repositories.find((item) => item.workspaceId === workspace.id);
  return (repo?.branch || workspace.branch || "").trim() || t("scm.inspect.branchUnknown");
}

function branchMeta(workspace: Workspace): string {
  const branch = branchOf(workspace);
  const mode = workspaceMode(workspace);
  if (mode === "worktree") return `${branch} · ${t("scm.inspect.worktreeTag")}`;
  if (mode === "direct") return `${branch} · ${t("scm.inspect.directTag")}`;
  return branch;
}

const options = computed<SelectOption[]>(() =>
  gitWorkspaces.value.map((workspace) => ({
    value: workspace.id,
    // 主行：项目名；副行：分支（含 worktree / 仓库）
    label: workspace.name || t("scm.inspect.branchUnknown"),
    description: branchMeta(workspace),
    keywords: [branchOf(workspace), workspace.name],
  })),
);

const model = computed({
  get: () => props.workspaceId,
  set: (id: string) => {
    if (id && id !== props.workspaceId) emit("update:workspaceId", id);
  },
});
</script>

<template>
  <UiSelect
    v-model="model"
    class="scm-inspect-branch-picker"
    size="sm"
    variant="ghost"
    :searchable="options.length > 6"
    :options="options"
    :aria-label="t('scm.inspect.switchWorkspaceBranch')"
    :title="t('scm.inspect.switchWorkspaceBranch')"
  >
    <template #value="{ option }">
      <span class="scm-inspect-branch-value">
        <span class="scm-inspect-branch-project">{{ option?.label ?? "" }}</span>
        <span class="scm-inspect-branch-ref">{{ option?.description ?? "" }}</span>
      </span>
    </template>
  </UiSelect>
</template>
