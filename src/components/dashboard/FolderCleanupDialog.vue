<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { GitRepositoryStatus, Project, Workspace } from "../../domain/types";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import { useAppStore } from "../../stores/app";
import { confirmDialog } from "../ui/confirm-dialog";
import {
  cleanupMarker,
  formatDiskSize,
  hasConflict,
  type CleanupMarker,
} from "../../views/dashboard/folder-cleanup";
import { rememberRestoreHints } from "../../views/dashboard/workspace-restore";
import SegmentedTabs from "../ui/SegmentedTabs.vue";

const emit = defineEmits<{ close: [] }>();

const store = useAppStore();
const tab = ref("archived");
const checked = ref<Record<string, boolean>>({});
const sizes = ref<Record<string, { bytes: number; missing: boolean; failed?: boolean }>>({});
const statuses = ref<Record<string, GitRepositoryStatus>>({});
const busy = ref(false);
const sizing = new Set<string>();
const statusStarted = new Set<string>();
const error = ref("");

const tabs = computed(() => [
  { id: "archived", label: t("dashboard.cleanupArchived") },
  { id: "all", label: t("dashboard.cleanupAll") },
]);

const projects = computed(() => store.data?.projects ?? []);
const workspaces = computed(() => store.data?.workspaces ?? []);

type Row = {
  workspace: Workspace;
  marker: CleanupMarker;
  dirty: boolean;
  conflict: boolean;
  missing: boolean;
  bytes: number;
};

type Group = {
  project: Project;
  rows: Row[];
  checked: boolean;
  indeterminate: boolean;
};

function rowOf(workspace: Workspace): Row {
  const status = statuses.value[workspace.id];
  const size = sizes.value[workspace.id];
  const marker = cleanupMarker(workspace);
  const conflict = Boolean(workspace.sessionId) || (status ? hasConflict(status) : false);
  const dirty = status ? status.changes.length > 0 : false;
  return {
    workspace,
    marker,
    dirty,
    conflict,
    missing: size?.missing ?? false,
    bytes: size?.bytes ?? 0,
  };
}

const groups = computed<Group[]>(() => {
  const visible = projects.value.filter((project) =>
    tab.value === "archived" ? Boolean(project.archivedAt) : true,
  );
  return visible
    .map((project) => {
      const rows = workspaces.value
        .filter((workspace) => workspace.projectId === project.id)
        .map(rowOf);
      const selected = rows.filter((row) => checked.value[row.workspace.id]).length;
      return {
        project,
        rows,
        checked: rows.length > 0 && selected === rows.length,
        indeterminate: selected > 0 && selected < rows.length,
      };
    })
    .filter((group) => group.rows.length > 0);
});

const visibleIds = computed(
  () => new Set(groups.value.flatMap((group) => group.rows.map((row) => row.workspace.id))),
);
const selectedCount = computed(
  () => [...visibleIds.value].filter((id) => checked.value[id]).length,
);

function paint(el: unknown, indeterminate: boolean) {
  if (el instanceof HTMLInputElement) el.indeterminate = indeterminate;
}

function toggleRow(id: string, on: boolean) {
  checked.value = { ...checked.value, [id]: on };
}

function toggleGroup(group: Group, on: boolean) {
  const next = { ...checked.value };
  for (const row of group.rows) next[row.workspace.id] = on;
  checked.value = next;
}

async function measureIds(ids: string[]) {
  const queue = ids.filter((id) => !(id in sizes.value) && !sizing.has(id));
  const workers = Array.from({ length: Math.min(2, queue.length) }, async () => {
    while (queue.length) {
      const id = queue.shift();
      if (!id || id in sizes.value || sizing.has(id)) continue;
      sizing.add(id);
      try {
        const rows = await backend.workspaceDirectorySizes([id]);
        const row = rows[0];
        sizes.value = {
          ...sizes.value,
          [id]: row
            ? { bytes: row.bytes, missing: row.missing }
            : { bytes: 0, missing: false, failed: true },
        };
      } catch {
        sizes.value = { ...sizes.value, [id]: { bytes: 0, missing: false, failed: true } };
      } finally {
        sizing.delete(id);
      }
    }
  });
  await Promise.all(workers);
}

async function loadStatuses(projectIds: string[]) {
  const pending = projectIds.filter((id) => !statusStarted.has(id));
  await Promise.all(pending.map(async (projectId) => {
    statusStarted.add(projectId);
    const group = await backend.listProjectGitStatus(projectId).catch(() => []);
    const next = { ...statuses.value };
    for (const status of group) next[status.workspaceId] = status;
    statuses.value = next;
  }));
}

watch(
  () => groups.value.map((group) => `${group.project.id}:${group.rows.map((row) => row.workspace.id).join(",")}`).join("|"),
  () => {
    void measureIds(groups.value.flatMap((group) => group.rows.map((row) => row.workspace.id)));
    void loadStatuses(groups.value.map((group) => group.project.id));
  },
  { immediate: true },
);

function markerLabel(marker: CleanupMarker) {
  if (marker === "worktree") return t("dashboard.cleanupWorktree");
  if (marker === "branch") return t("dashboard.cleanupBranch");
  return t("dashboard.cleanupFolder");
}

async function confirm() {
  if (busy.value || !selectedCount.value) return;
  const accepted = await confirmDialog(
    t("dashboard.cleanupConfirmAsk", { count: selectedCount.value }),
    { danger: true, type: "warning", primary: t("dashboard.cleanupConfirm") },
  );
  if (!accepted) return;
  busy.value = true;
  error.value = "";
  const chosen = workspaces.value.filter(
    (workspace) => checked.value[workspace.id] && visibleIds.value.has(workspace.id),
  );
  try {
    for (const workspace of chosen) {
      const status = statuses.value[workspace.id];
      rememberRestoreHints([
        {
          projectId: workspace.projectId,
          sourcePath: workspace.sourcePath,
          branch: workspace.branch,
          upstream: status?.upstream ?? null,
        },
      ]);
      await store.detachWorkspace(workspace.id, {
        workspaceId: workspace.id,
        deleteLocalBranch: false,
        deleteRemoteBranch: false,
        deleteCopyDirectory: workspace.isolationMode === "copy",
      });
      const next = { ...checked.value };
      delete next[workspace.id];
      checked.value = next;
    }
    emit("close");
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <UiModal @close="emit('close')">
    <form class="modal cleanup-modal" @submit.prevent="confirm">
      <header class="modal-heading">
        <div>
          <h2>{{ t("dashboard.cleanupTitle") }}</h2>
          <p>{{ t("dashboard.cleanupHint") }}</p>
        </div>
      </header>
      <div class="modal-body vs-scroll cleanup-body">
        <SegmentedTabs v-model="tab" :label="t('dashboard.cleanupTitle')" :items="tabs" />
        <p v-if="!groups.length" class="cleanup-empty">{{ t("dashboard.cleanupEmpty") }}</p>
        <section v-for="group in groups" :key="group.project.id" class="cleanup-group">
          <label class="cleanup-project">
            <input
              class="ui-checkbox"
              type="checkbox"
              :checked="group.checked"
              :ref="(el) => paint(el, group.indeterminate)"
              @change="toggleGroup(group, ($event.target as HTMLInputElement).checked)"
            />
            <strong>{{ group.project.name }}</strong>
            <span v-if="group.project.archivedAt" class="ds-badge">{{ t("dashboard.archivedBadge") }}</span>
          </label>
          <ul>
            <li v-for="row in group.rows" :key="row.workspace.id">
              <label class="cleanup-row">
                <input
                  class="ui-checkbox"
                  type="checkbox"
                  :checked="Boolean(checked[row.workspace.id])"
                  @change="toggleRow(row.workspace.id, ($event.target as HTMLInputElement).checked)"
                />
                <span class="cleanup-mark" :data-kind="row.marker">{{ markerLabel(row.marker) }}</span>
                <span class="cleanup-name">{{ row.workspace.branch || row.workspace.name }}</span>
                <span class="cleanup-path">
                  <UiPathText :path="row.workspace.workPath" />
                </span>
                <span v-if="row.conflict" class="cleanup-flag">{{ t("dashboard.cleanupConflict") }}</span>
                <span v-else-if="row.dirty" class="cleanup-flag">{{ t("dashboard.cleanupDirty") }}</span>
                <span class="cleanup-size">
                  <template v-if="!sizes[row.workspace.id]">{{ t("dashboard.cleanupMeasuring") }}</template>
                  <template v-else-if="sizes[row.workspace.id]?.failed">—</template>
                  <template v-else-if="sizes[row.workspace.id]?.missing">{{ t("dashboard.cleanupMissing") }}</template>
                  <template v-else>{{ formatDiskSize(sizes[row.workspace.id]?.bytes ?? 0) }}</template>
                </span>
              </label>
            </li>
          </ul>
        </section>
        <p v-if="error" class="error-card">{{ error }}</p>
      </div>
      <div class="modal-actions">
        <UiButton type="button" :disabled="busy" @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton type="submit" variant="primary" :disabled="busy || !selectedCount">
          {{ busy ? t("dashboard.cleanupBusy") : t("dashboard.cleanupConfirm") }}
        </UiButton>
      </div>
    </form>
  </UiModal>
</template>
