<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { FolderPlus, Settings, Trash2 } from "@lucide/vue";
import * as backend from "../../services/backend";
import type { WorkspaceCandidate } from "../../domain/types";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import { nativeFilePath } from "../chat/workspace-file-drag";
import {
  branchNameFromRequirement,
  defaultNewBranchName,
  requirementBranchNames,
} from "../workbench/workspace-mode";

type GitMode = "worktree-new" | "worktree-existing" | "current";

type PickerItem = WorkspaceCandidate & {
  gitMode: GitMode;
  baseRef: string;
  checkoutName: string;
};

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
const picking = ref(false);
const error = ref("");
const dropOver = ref(false);
let dragDepth = 0;

const gitModes: Array<{ id: GitMode; label: string }> = [
  { id: "worktree-new", label: t("workspace.gitMode.new") },
  { id: "worktree-existing", label: t("workspace.gitMode.existing") },
  { id: "current", label: t("workspace.gitMode.current") },
];

const boundPaths = computed(() =>
  (store.data?.workspaces ?? [])
    .filter((workspace) => workspace.projectId === props.projectId)
    .map((workspace) => ({
      path: normalizeBindPath(workspace.sourcePath),
      git: workspace.kind === "git",
    })),
);

const knownBranches = computed(() =>
  requirementBranchNames(store.data?.workspaces ?? [], props.projectId),
);
const requirementName = computed(
  () => store.data?.projects.find((project) => project.id === props.projectId)?.name?.trim() ?? "",
);
const branchPrefix = computed(() => store.data?.settings.branchPrefix || "feat");
const generatingPath = ref("");
const generatedHint = ref("");
const alreadyAddedCount = computed(() => items.value.filter((item) => isAlreadyBound(item.path)).length);
const canSubmit = computed(() =>
  items.value.length > 0 && alreadyAddedCount.value === 0 && !busy.value && !picking.value,
);

function normalizeBindPath(path: string) {
  return path.replace(/^\\\\\?\\/i, "").replaceAll("\\", "/").replace(/\/+$/, "").toLowerCase();
}

function isAlreadyBound(path: string) {
  const normalized = normalizeBindPath(path);
  return boundPaths.value.some((bound) => {
    if (normalized === bound.path) return true;
    return bound.git && normalized.startsWith(`${bound.path}/`);
  });
}

function toItem(candidate: WorkspaceCandidate): PickerItem {
  return {
    ...candidate,
    gitMode: "worktree-new",
    baseRef: candidate.currentBranch ?? candidate.branches[0] ?? "HEAD",
    checkoutName:
      candidate.kind === "git"
        ? defaultNewBranchName(requirementName.value || candidate.name, knownBranches.value, branchPrefix.value)
        : "",
  };
}

function openBranchSettings() {
  emit("close");
  void router.push({ path: "/settings", query: { section: "branch" } });
}

async function generateBranchName(item: PickerItem) {
  const name = requirementName.value || item.name;
  if (!name || generatingPath.value) return;
  generatingPath.value = item.path;
  generatedHint.value = t("workspace.generatingBranch");
  error.value = "";
  try {
    item.checkoutName = await backend.generateBranchName(name);
    generatedHint.value = t("workspace.filledBranch", { name: item.checkoutName });
  } catch (cause) {
    item.checkoutName = branchNameFromRequirement(name, branchPrefix.value);
    generatedHint.value = t("workspace.filledBranch", { name: item.checkoutName });
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    generatingPath.value = "";
    window.setTimeout(() => {
      generatedHint.value = "";
    }, 2400);
  }
}

async function addPaths(sourcePaths: string[]) {
  if (!sourcePaths.length) return;
  error.value = "";
  const existing = new Set(items.value.map((item) => item.path));
  const next = [...items.value];
  for (const sourcePath of sourcePaths) {
    const scanned = await backend.scanWorkspaceCandidates(sourcePath);
    for (const candidate of scanned) {
      if (existing.has(candidate.path)) continue;
      existing.add(candidate.path);
      next.push(toItem(candidate));
    }
  }
  items.value = next;
}

async function pickFolders() {
  if (picking.value || busy.value) return;
  error.value = "";
  try {
    const sourcePaths = await backend.selectFolders();
    if (!sourcePaths.length) return;
    picking.value = true;
    await addPaths(sourcePaths);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    picking.value = false;
  }
}

function removeItem(path: string) {
  items.value = items.value.filter((item) => item.path !== path);
}

function setGitMode(item: PickerItem, mode: GitMode) {
  item.gitMode = mode;
  if (mode === "worktree-existing" && !item.branches.includes(item.checkoutName)) {
    item.checkoutName = item.currentBranch ?? item.branches[0] ?? "HEAD";
  }
  if (mode === "worktree-new" && !item.checkoutName.trim()) {
    item.checkoutName = defaultNewBranchName(requirementName.value || item.name, knownBranches.value, branchPrefix.value);
  }
}

function onDragEnter(event: DragEvent) {
  if (!event.dataTransfer?.types.includes("Files")) return;
  event.preventDefault();
  dragDepth += 1;
  dropOver.value = true;
}

function onDragOver(event: DragEvent) {
  if (!event.dataTransfer?.types.includes("Files")) return;
  event.preventDefault();
  event.dataTransfer.dropEffect = "copy";
}

function onDragLeave() {
  dragDepth -= 1;
  if (dragDepth > 0) return;
  dragDepth = 0;
  dropOver.value = false;
}

async function onDrop(event: DragEvent) {
  if (!event.dataTransfer?.types.includes("Files")) return;
  event.preventDefault();
  dragDepth = 0;
  dropOver.value = false;
  const paths = [...(event.dataTransfer.files ?? [])]
    .map(nativeFilePath)
    .filter(Boolean);
  if (!paths.length) return;
  picking.value = true;
  error.value = "";
  try {
    await addPaths(paths);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    picking.value = false;
  }
}

onMounted(() => {
  window.addEventListener("dragenter", onDragEnter);
  window.addEventListener("dragover", onDragOver);
  window.addEventListener("dragleave", onDragLeave);
  window.addEventListener("drop", onDrop);
  // 传了 initialPaths 时：打开即按已有 addPaths 流程预填
  if (props.initialPaths.length) {
    picking.value = true;
    void addPaths(props.initialPaths)
      .catch((cause) => {
        error.value = cause instanceof Error ? cause.message : String(cause);
      })
      .finally(() => {
        picking.value = false;
      });
  }
});

async function submit() {
  if (!canSubmit.value) return;
  if (alreadyAddedCount.value) {
    error.value = t("workspace.removeAddedFirst");
    return;
  }
  const missingName = items.value.find(
    (item) => item.kind === "git" && item.gitMode === "worktree-new" && !item.checkoutName.trim(),
  );
  if (missingName) {
    error.value = t("workspace.needBranchName", { name: missingName.name });
    return;
  }
  const requests = items.value
    .filter((item) => !isAlreadyBound(item.path))
    .map((item) => ({
      name: item.name,
      path: item.path,
      mode:
        item.kind !== "git" || item.gitMode === "current"
          ? ("direct" as const)
          : ("isolated" as const),
      baseRef: item.gitMode === "worktree-new" ? item.baseRef : item.checkoutName,
      checkoutName: item.checkoutName.trim(),
      createBranch: item.gitMode === "worktree-new",
    }));
  if (!requests.length) return;
  store.queueAddWorkspaces(props.projectId, requests);
  emit("added");
}

onBeforeUnmount(() => {
  window.removeEventListener("dragenter", onDragEnter);
  window.removeEventListener("dragover", onDragOver);
  window.removeEventListener("dragleave", onDragLeave);
  window.removeEventListener("drop", onDrop);
  dragDepth = 0;
  dropOver.value = false;
});
</script>

<template>
  <UiModal :class="{ 'is-over': dropOver }">
    <form class="modal workspace-picker-modal" @submit.prevent="submit">
      <div class="workspace-picker-head">
        <h2>{{ t("workspace.addTitle") }}</h2>
        <p v-if="alreadyAddedCount">{{ t("workspace.alreadyCount", { count: alreadyAddedCount }) }}</p>
        <p v-else-if="items.length">{{ t("workspace.addedCount", { count: items.length }) }}</p>
        <p v-else>{{ t("workspace.addHint") }}</p>
      </div>
      <div v-if="error" class="error-card">{{ error }}</div>
      <button
        type="button"
        class="workspace-drop"
        :class="{ 'is-compact': items.length, 'is-over': dropOver }"
        :disabled="busy"
        @click="pickFolders"
      >
        <span class="workspace-drop-icon">
          <FolderPlus :size="items.length ? 14 : 18" />
        </span>
        <strong>{{ t("workspace.addFolder") }}</strong>
        <span v-if="!items.length" class="workspace-drop-hint">{{ t("workspace.dropHint") }}</span>
      </button>
      <div v-if="items.length" class="workspace-pick-list">
        <article
          v-for="(item, index) in items"
          :key="item.path"
          class="workspace-pick-item"
          :class="{ 'is-added': isAlreadyBound(item.path) }"
        >
          <div class="workspace-pick-top">
            <div class="workspace-pick-copy">
              <div class="workspace-pick-name">
                <strong>{{ item.name }}</strong>
                <span class="tag" :class="{ git: item.kind === 'git' }">{{ item.kind === "git" ? t("workspace.kind.git") : t("workspace.kind.folder") }}</span>
                <span v-if="isAlreadyBound(item.path)" class="tag is-added">{{ t("workspace.alreadyAdded") }}</span>
              </div>
              <UiPathText :path="item.path" tag="small" />
            </div>
            <p v-if="isAlreadyBound(item.path)" class="workspace-pick-hint is-added">{{ t("workspace.alreadyHint") }}</p>
            <label v-else-if="item.kind === 'git'" class="workspace-pick-field is-mode">
              <UiSelect
                :model-value="item.gitMode"
                :options="gitModes.map((mode) => ({ value: mode.id, label: mode.label }))"
                @update:model-value="(value: unknown) => setGitMode(item, value as GitMode)"
              />
            </label>
            <p v-else class="workspace-pick-hint">{{ t("workspace.useCurrentDir") }}</p>
            <UiButton
              class="icon-button"
              type="button"
              size="icon"
              :title="isAlreadyBound(item.path) ? t('workspace.removeItem') : t('common.delete')"
              @click="removeItem(item.path)"
            >
              <Trash2 :size="15" />
            </UiButton>
          </div>
          <div v-if="!isAlreadyBound(item.path) && item.kind === 'git' && item.gitMode === 'worktree-new'" class="workspace-pick-opts">
            <label class="workspace-pick-field">
              <span>{{ t("workspace.baseBranch") }}</span>
              <UiSelect
                v-model="item.baseRef"
                :search-placeholder="t('workspace.searchBranch')"
                :options="(item.branches.length ? item.branches : ['HEAD']).map((branch) => ({ value: branch, label: branch }))"
              />
            </label>
            <label class="workspace-pick-field">
              <span>{{ t("workspace.newBranch") }}</span>
              <div class="workspace-branch-wrap">
                <div class="workspace-branch-field">
                  <UiInput
                    v-model="item.checkoutName"
                    :placeholder="t('workspace.branchPlaceholder')"
                    autocomplete="off"
                    :list="knownBranches.length ? `known-branches-${index}` : undefined"
                  />
                  <UiButton
                    type="button"
                    variant="ghost"
                    size="icon"
                    :title="t('workspace.generateBranch')"
                    :disabled="(!requirementName && !item.name) || generatingPath === item.path"
                    @click="generateBranchName(item)"
                  >
                    <Codicon v-if="generatingPath === item.path" name="loading" spin />
                    <Codicon v-else name="sparkle" />
                  </UiButton>
                  <UiButton
                    type="button"
                    variant="ghost"
                    size="icon"
                    :title="t('workspace.configureBranch')"
                    @click="openBranchSettings"
                  >
                    <Settings :size="14" />
                  </UiButton>
                </div>
                <small v-if="generatingPath === item.path || generatedHint" class="workspace-branch-hint">{{ generatedHint }}</small>
              </div>
              <datalist v-if="knownBranches.length" :id="`known-branches-${index}`">
                <option v-for="branch in knownBranches" :key="branch" :value="branch" />
              </datalist>
            </label>
          </div>
          <div v-else-if="!isAlreadyBound(item.path) && item.kind === 'git' && item.gitMode === 'worktree-existing'" class="workspace-pick-opts">
            <label class="workspace-pick-field">
              <span>{{ t("workspace.existingBranch") }}</span>
              <UiSelect
                v-model="item.checkoutName"
                :search-placeholder="t('workspace.searchBranch')"
                :options="(item.branches.length ? item.branches : ['HEAD']).map((branch) => ({ value: branch, label: branch }))"
              />
            </label>
          </div>
        </article>
      </div>
      <div class="modal-actions">
        <UiButton type="button" :disabled="busy" @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton v-if="items.length" type="submit" variant="primary" :disabled="!canSubmit">
          {{ busy ? t("workspace.adding") : t("workspace.addSubmit", { count: items.length }) }}
        </UiButton>
      </div>
    </form>
  </UiModal>
</template>
