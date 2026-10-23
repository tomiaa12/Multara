<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import {
  Archive,
  ArchiveRestore,
  ArrowRight,
  ChevronDown,
  ChevronRight,
  FolderPlus,
  Pencil,
  Plus,
  Search,
  SearchX,
  Trash2,
  X,
} from "@lucide/vue";
import { useRouter } from "vue-router";
import { t } from "../i18n";
import AddWorkspaceModal from "../components/dashboard/AddWorkspaceModal.vue";
import RequirementEmptyState from "../components/dashboard/RequirementEmptyState.vue";
import WorkspaceClipGrid from "../components/dashboard/WorkspaceClipGrid.vue";
import ContextMenu, {
  type ContextMenuItem,
} from "../components/ui/ContextMenu.vue";
import SegmentedTabs from "../components/ui/SegmentedTabs.vue";
import TagManageDialog from "../components/ui/TagManageDialog.vue";
import TagPicker from "../components/ui/TagPicker.vue";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";
import {
  workspaceMode,
  workspaceModeLabel,
} from "../components/workbench/workspace-mode";
import {
  buildHomeProject,
  matchesHomeFilter,
  workspaceKindLabel,
  type HomeFilter,
} from "./dashboard/project-home";
import {
  addRequirementCategory,
  assignRequirementTags,
  loadRequirementCategories,
  projectMatchesTags,
  recolorRequirementCategory,
  removeRequirementCategory,
  renameRequirementCategory,
  saveRequirementCategories,
  type RequirementCategory,
  type RequirementCategoryState,
} from "./dashboard/requirement-categories";

const TAG_FILTER_MAX_HEIGHT = 52;

defineOptions({ name: "DashboardView" });

const store = useAppStore();
const router = useRouter();
const showCreate = ref(false);
const projectName = ref("");
const note = ref("");
const busy = ref(false);
const error = ref("");
const workspaceProjectId = ref("");
const showWorkspacePicker = ref(false);
const expandedProjects = ref(new Set<string>());
const filter = ref<HomeFilter>("all");
const query = ref("");
const categories = ref<RequirementCategoryState>(loadRequirementCategories(localStorage));
const categoryFilter = ref<string[]>([]);
const draftTags = ref<string[]>([]);
const editingId = ref("");
const showManage = ref(false);
const tagsExpanded = ref(false);
const tagsOverflow = ref(false);
const tagChips = ref<HTMLElement | null>(null);
const workspaceMenu = ref<{ x: number; y: number; workspaceId: string } | null>(
  null,
);
const workspaceMenuItems: ContextMenuItem[] = [
  { id: "reveal", label: t("dashboard.reveal") },
];

const rows = computed(() => {
  const data = store.data;
  if (!data) return [];
  return data.projects.map((project) =>
    buildHomeProject(project, data, store.runStates),
  );
});

const statusRows = computed(() =>
  rows.value.filter((row) => matchesHomeFilter(row, filter.value, "")),
);
const visibleRows = computed(() =>
  rows.value.filter((row) => {
    if (!matchesHomeFilter(row, filter.value, query.value)) return false;
    return projectMatchesTags(categories.value.assignments[row.id], categoryFilter.value);
  }),
);
const categoryTabs = computed(() =>
  categories.value.categories.map((item) => ({
    ...item,
    count: statusRows.value.filter((row) => categories.value.assignments[row.id]?.includes(item.id)).length,
  })),
);
const saveLabel = computed(() => {
  if (busy.value) return t("common.saving");
  if (editingId.value) return t("common.save");
  return t("dashboard.createProject");
});
function tagsFor(projectId: string): RequirementCategory[] {
  return (categories.value.assignments[projectId] ?? [])
    .map((id) => categories.value.categories.find((item) => item.id === id))
    .filter((item): item is RequirementCategory => Boolean(item));
}
const hasProjects = computed(() => Boolean(store.data?.projects.length));
const activeRows = computed(() => rows.value.filter((row) => !row.archived));
const archivedCount = computed(() => rows.value.length - activeRows.value.length);
const runningCount = computed(() => activeRows.value.filter((row) => row.filters.includes("running")).length);
const homeTabs = computed(() => [
  { id: "all", label: t("dashboard.tab.all"), count: activeRows.value.length },
  { id: "running", label: t("dashboard.tab.running"), count: runningCount.value },
  { id: "archived", label: t("dashboard.tab.archived"), count: archivedCount.value },
]);

function setFilter(next: HomeFilter) {
  filter.value = next;
}

function persistCategories(next: RequirementCategoryState) {
  categories.value = next;
  saveRequirementCategories(localStorage, next);
}

function toggleCategory(id: string) {
  categoryFilter.value = categoryFilter.value.includes(id)
    ? categoryFilter.value.filter((item) => item !== id)
    : [...categoryFilter.value, id];
}

function createCategory(name: string) {
  const id = crypto.randomUUID();
  const next = addRequirementCategory(categories.value, name, id);
  if (next === categories.value) return "";
  persistCategories(next);
  return id;
}

function onPickerCreate(name: string) {
  const id = createCategory(name);
  if (id) draftTags.value = [...draftTags.value, id];
}

function assignDraftTags(projectId: string) {
  persistCategories(assignRequirementTags(categories.value, projectId, draftTags.value));
}

function openCreate() {
  editingId.value = "";
  projectName.value = "";
  note.value = "";
  draftTags.value = [];
  showCreate.value = true;
}

function openEdit(projectId: string, name: string, projectNote: string) {
  editingId.value = projectId;
  projectName.value = name;
  note.value = projectNote;
  draftTags.value = [...(categories.value.assignments[projectId] ?? [])];
  showCreate.value = true;
}

function closeEditor() {
  showCreate.value = false;
  editingId.value = "";
}

async function measureTags() {
  await nextTick();
  const el = tagChips.value;
  if (!el) {
    tagsOverflow.value = false;
    return;
  }
  const collapsed = el.classList.contains("is-collapsed");
  if (collapsed) el.classList.remove("is-collapsed");
  tagsOverflow.value = el.scrollHeight > TAG_FILTER_MAX_HEIGHT;
  if (collapsed) el.classList.add("is-collapsed");
}

watch(() => categories.value.categories.length, () => {
  void measureTags();
});
onMounted(() => {
  void measureTags();
});

function toggleProject(projectId: string) {
  const expanded = new Set(expandedProjects.value);
  if (expanded.has(projectId)) expanded.delete(projectId);
  else expanded.add(projectId);
  expandedProjects.value = expanded;
}

function openWorkspacePicker(projectId: string) {
  error.value = "";
  workspaceProjectId.value = projectId;
  showWorkspacePicker.value = true;
}

function closeWorkspacePicker() {
  showWorkspacePicker.value = false;
  workspaceProjectId.value = "";
}

async function onWorkspacesAdded() {
  const id = workspaceProjectId.value;
  closeWorkspacePicker();
  if (!id) return;
  const expanded = new Set(expandedProjects.value);
  expanded.add(id);
  expandedProjects.value = expanded;
}

function projectJobs(projectId: string) {
  return store.workspaceJobs.filter((job) => job.projectId === projectId);
}

function canOpenProject(projectId: string, workspaceCount: number) {
  return workspaceCount > 0 && !store.isProjectPreparing(projectId);
}

async function saveProject() {
  if (!projectName.value.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    if (editingId.value) {
      await store.updateProject(editingId.value, projectName.value, note.value);
      assignDraftTags(editingId.value);
      closeEditor();
      return;
    }
    const project = await store.createProject(projectName.value, note.value);
    if (project) assignDraftTags(project.id);
    closeEditor();
    projectName.value = "";
    note.value = "";
    draftTags.value = [];
    if (project) {
      const expanded = new Set(expandedProjects.value);
      expanded.add(project.id);
      expandedProjects.value = expanded;
      openWorkspacePicker(project.id);
    }
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

function openWorkspaceMenu(event: MouseEvent, workspaceId: string) {
  event.preventDefault();
  event.stopPropagation();
  workspaceMenu.value = { x: event.clientX, y: event.clientY, workspaceId };
}

async function onWorkspaceMenuSelect(id: string) {
  const menu = workspaceMenu.value;
  workspaceMenu.value = null;
  if (!menu || id !== "reveal") return;
  try {
    await backend.revealWorkspacePath(menu.workspaceId);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function toggleArchive(projectId: string, archived: boolean) {
  error.value = "";
  try {
    await store.setProjectArchived(projectId, !archived);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function removeProject(projectId: string, projectName: string) {
  if (!window.confirm(t("dashboard.deleteConfirm", { name: projectName })))
    return;
  await store.deleteProject(projectId);
}
</script>

<template>
  <div class="page req-home" :class="{ 'is-empty': !hasProjects }">
    <template v-if="!hasProjects">
      <div v-if="error" class="error-card">{{ error }}</div>
      <RequirementEmptyState @create="openCreate" />
    </template>

    <template v-else>
      <header class="page-header">
        <div>
          <h1>{{ t("dashboard.title") }}</h1>
          <p>{{ t("dashboard.subtitle") }}</p>
        </div>
        <div class="header-actions">
          <UiButton variant="primary" @click="openCreate">
            <Plus :size="15" /> {{ t("dashboard.create") }}
          </UiButton>
        </div>
      </header>

      <div v-if="error" class="error-card">{{ error }}</div>

      <section class="page-section">
        <div class="req-filters">
          <div class="page-toolbar">
            <SegmentedTabs v-model="filter" :label="t('dashboard.filterLabel')" :items="homeTabs" />
            <label class="ds-search req-search">
              <Search :size="14" />
              <UiInput v-model="query" type="search" :placeholder="t('dashboard.search')" />
            </label>
          </div>
          <div class="req-filter-bar">
            <div class="tag-filter">
              <div
                ref="tagChips"
                class="tag-filter-chips"
                :class="{ 'is-collapsed': !tagsExpanded }"
              >
                <button
                  v-for="item in categoryTabs"
                  :key="item.id"
                  type="button"
                  class="tag-filter-chip"
                  :class="{ 'is-on': categoryFilter.includes(item.id) }"
                  @click="toggleCategory(item.id)"
                >
                  <i class="tag-dot" :style="{ background: item.color }" />
                  {{ item.name }}
                  <em>{{ item.count }}</em>
                </button>
              </div>
              <button
                v-if="tagsOverflow"
                type="button"
                class="tag-filter-chip"
                @click="tagsExpanded = !tagsExpanded"
              >
                {{ tagsExpanded ? t("dashboard.tagsCollapse") : t("dashboard.tagsExpand") }}
              </button>
            </div>
            <UiButton size="small" variant="ghost" @click="showManage = true">{{ t("dashboard.manageTags") }}</UiButton>
          </div>
        </div>

      <div v-if="visibleRows.length" class="req-list">
        <article
          v-for="row in visibleRows"
          :key="row.id"
          class="req-card"
          :class="{
            'is-open': expandedProjects.has(row.id),
            'is-archived': row.archived,
          }"
          tabindex="0"
          @click="toggleProject(row.id)"
          @keydown.enter="toggleProject(row.id)"
          @keydown.space.prevent="toggleProject(row.id)"
        >
          <div class="req-row">
            <span class="req-expand" aria-hidden="true">
              <ChevronDown v-if="expandedProjects.has(row.id)" :size="16" />
              <ChevronRight v-else :size="16" />
            </span>
            <div class="req-main">
              <div class="req-title">
                <h3>{{ row.name }}</h3>
                <span
                  v-if="row.pillText"
                  class="ds-badge"
                  :data-tone="row.pill === 'running' ? 'primary' : undefined"
                >
                  <i v-if="row.pill === 'running'" class="ds-dot" data-tone="primary" />
                  {{ row.pillText }}
                </span>
                <span v-if="row.archived" class="ds-badge">{{ t("dashboard.archivedBadge") }}</span>
                <span v-for="tag in tagsFor(row.id)" :key="tag.id" class="tag-chip">
                  <i class="tag-dot" :style="{ background: tag.color }" />
                  <span>{{ tag.name }}</span>
                </span>
              </div>
              <p v-if="row.note.trim()" class="req-note">{{ row.note }}</p>
              <div
                v-if="(row.workspaces.length || projectJobs(row.id).length) && !expandedProjects.has(row.id)"
                class="req-projects"
              >
                <span class="req-project-count"
                  >{{ t("dashboard.projectCount", { n: row.workspaces.length + projectJobs(row.id).length }) }}</span
                >
                <span
                  v-for="workspace in row.workspaces"
                  :key="workspace.id"
                  class="req-project-chip"
                >
                  {{ workspace.name }}
                </span>
                <span
                  v-for="job in projectJobs(row.id)"
                  :key="job.id"
                  class="req-project-chip"
                  :class="{ 'is-failed': job.status === 'failed' }"
                >
                  {{ job.status === "preparing" ? t("dashboard.preparingJob", { name: job.name }) : job.name }}
                </span>
              </div>
              <small
                v-if="row.workingMeta && !expandedProjects.has(row.id)"
                class="req-meta"
                >{{ row.workingMeta }}</small
              >
            </div>
            <div class="req-actions" @click.stop>
              <UiButton
                class="icon-button"
                size="icon"
                variant="ghost"
                :title="t('dashboard.edit')"
                @click="openEdit(row.id, row.name, row.note)"
              >
                <Pencil :size="14" />
              </UiButton>
              <UiButton
                class="icon-button"
                size="icon"
                variant="ghost"
                :title="row.archived ? t('dashboard.unarchive') : t('dashboard.archive')"
                @click="toggleArchive(row.id, row.archived)"
              >
                <ArchiveRestore v-if="row.archived" :size="14" />
                <Archive v-else :size="14" />
              </UiButton>
              <UiButton
                class="icon-button req-delete"
                size="icon"
                variant="danger"
                :title="t('dashboard.delete')"
                @click="removeProject(row.id, row.name)"
              >
                <Trash2 :size="14" />
              </UiButton>
              <UiButton
                size="small"
                variant="ghost"
                :title="t('dashboard.addWorkspaceTitle')"
                @click="openWorkspacePicker(row.id)"
              >
                <FolderPlus :size="14" /> {{ t("workspace.addTitle") }}
              </UiButton>
              <UiButton
                size="small"
                :disabled="!canOpenProject(row.id, row.workspaces.length)"
                :title="store.isProjectPreparing(row.id) ? t('dashboard.preparingBlocked') : ''"
                @click="router.push(`/projects/${row.id}`)"
              >
                {{ store.isProjectPreparing(row.id) ? t("dashboard.preparing") : t("common.open") }}
                <ArrowRight v-if="!store.isProjectPreparing(row.id)" :size="13" />
              </UiButton>
            </div>
          </div>

          <div
            class="req-extra-wrap"
            :class="{ 'is-open': expandedProjects.has(row.id) }"
            @click.stop
          >
            <div class="req-extra">
              <template v-if="row.pill === 'setup' && !projectJobs(row.id).length">
                <p class="req-setup">
                  {{ t("dashboard.noWorkspaces") }}
                </p>
              </template>
              <template v-else>
                <p v-if="!row.workers.length" class="req-idle">{{ row.idleLine }}</p>
                <div v-if="row.workers.length" class="req-block">
                  <div class="req-block-label">{{ t("dashboard.working") }}</div>
                  <div class="req-workers">
                    <div class="req-faces">
                      <span
                        v-for="worker in row.workers"
                        :key="worker.key"
                        class="req-face"
                        :class="worker.kind"
                        :title="worker.title"
                      >
                        <img
                          v-if="worker.image"
                          :src="worker.image"
                          :alt="worker.name"
                        />
                        <template v-else>{{ worker.initial }}</template>
                      </span>
                    </div>
                    <p>
                      <template
                        v-for="(part, index) in row.workerParts"
                        :key="`${row.id}-${index}`"
                      >
                        <b v-if="part.bold">{{ part.text }}</b>
                        <template v-else>{{ part.text }}</template>
                      </template>
                    </p>
                  </div>
                </div>
                <div class="req-block">
                  <div class="req-block-label">
                    {{ workspaceKindLabel(row.gitCount, row.folderCount) }}
                  </div>
                  <WorkspaceClipGrid
                    :active="expandedProjects.has(row.id)"
                    :items="row.workspaces.length + projectJobs(row.id).length"
                  >
                    <div
                      v-for="job in projectJobs(row.id)"
                      :key="job.id"
                      class="req-ws is-preparing"
                      :class="{ 'is-failed': job.status === 'failed' }"
                    >
                      <div class="req-ws-top">
                        <strong>{{ job.name }}</strong>
                        <span class="req-mode pill-mute">{{
                          job.status === "preparing" ? t("dashboard.jobPreparing") : t("dashboard.jobFailed")
                        }}</span>
                      </div>
                      <p>{{ job.status === "preparing" ? t("dashboard.jobPreparingHint") : job.error }}</p>
                      <UiButton
                        v-if="job.status === 'failed'"
                        size="small"
                        @click="store.dismissWorkspaceJob(job.id)"
                      >
                        {{ t("common.close") }}
                      </UiButton>
                    </div>
                    <div
                      v-for="workspace in row.workspaces"
                      :key="workspace.id"
                      class="req-ws"
                      @contextmenu="openWorkspaceMenu($event, workspace.id)"
                    >
                      <div class="req-ws-top">
                        <strong>{{ workspace.name }}</strong>
                        <span
                          class="req-mode"
                          :class="{
                            'pill-worktree':
                              workspaceMode(workspace) === 'worktree',
                            'pill-direct':
                              workspaceMode(workspace) === 'direct',
                            'pill-folder':
                              workspaceMode(workspace) === 'folder',
                          }"
                          >{{ workspaceModeLabel(workspace) }}</span
                        >
                      </div>
                      <p v-if="workspace.kind === 'git'">
                        {{ workspace.branch || "HEAD" }}
                      </p>
                    </div>
                  </WorkspaceClipGrid>
                </div>
              </template>
            </div>
          </div>
        </article>
      </div>

      <div v-else class="ds-card ds-empty req-filter-empty">
        <span class="ds-empty-icon"><SearchX :size="18" /></span>
        <h3>{{ filter === "archived" && !query.trim() && !categoryFilter.length ? t("dashboard.archivedEmpty") : t("dashboard.noMatch") }}</h3>
        <p>{{ filter === "archived" && !query.trim() ? t("dashboard.archivedEmptyHint") : t("dashboard.noMatchHint") }}</p>
        <UiButton
          @click="
            setFilter('all');
            categoryFilter = [];
            query = '';
          "
          >{{ t("dashboard.viewAll") }}</UiButton
        >
      </div>
      </section>
    </template>

    <ContextMenu
      v-if="workspaceMenu"
      :x="workspaceMenu.x"
      :y="workspaceMenu.y"
      :items="workspaceMenuItems"
      @select="onWorkspaceMenuSelect"
      @close="workspaceMenu = null"
    />

    <UiModal
      v-if="showCreate"
      @click.self="closeEditor"
    >
      <form class="modal" @submit.prevent="saveProject">
        <div class="modal-heading">
          <h2>{{ editingId ? t("dashboard.edit") : t("dashboard.create") }}</h2>
          <UiButton class="icon-button" size="icon" :title="t('common.close')" @click="closeEditor">
            <X :size="15" />
          </UiButton>
        </div>
        <label>
          <span>{{ t("dashboard.nameLabel") }}</span>
          <UiInput
            v-model="projectName"
            autofocus
            :placeholder="t('dashboard.namePlaceholder')"
          />
        </label>
        <label>
          <span>{{ t("dashboard.noteLabel") }}</span>
          <UiTextarea
            v-model="note"
            rows="3"
            :placeholder="t('dashboard.notePlaceholder')"
          />
        </label>
        <label>
          <span>{{ t("dashboard.tagsLabel") }}</span>
          <TagPicker
            v-model="draftTags"
            :tags="categories.categories"
            @create="onPickerCreate"
            @manage="showManage = true"
          />
        </label>
        <div class="modal-actions">
          <UiButton @click="closeEditor">{{ t("common.cancel") }}</UiButton>
          <UiButton
            type="submit"
            variant="primary"
            :class="{ 'is-loading': busy }"
            :disabled="busy || !projectName.trim()"
          >{{ saveLabel }}</UiButton>
        </div>
      </form>
    </UiModal>

    <TagManageDialog
      v-if="showManage"
      :tags="categories.categories"
      @close="showManage = false"
      @create="createCategory"
      @rename="(id, name) => persistCategories(renameRequirementCategory(categories, id, name))"
      @recolor="(id, color) => persistCategories(recolorRequirementCategory(categories, id, color))"
      @remove="(id) => {
        persistCategories(removeRequirementCategory(categories, id));
        categoryFilter = categoryFilter.filter((item) => item !== id);
        draftTags = draftTags.filter((item) => item !== id);
      }"
    />

    <AddWorkspaceModal
      v-if="showWorkspacePicker"
      :project-id="workspaceProjectId"
      @close="closeWorkspacePicker"
      @added="onWorkspacesAdded"
    />
  </div>
</template>
