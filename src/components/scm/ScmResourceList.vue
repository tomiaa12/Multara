<script setup lang="ts">
import { computed, ref } from "vue";
import type { GitFileStatus } from "../../domain/types";
import { t } from "../../i18n";
import FileTypeIcon from "../FileTypeIcon.vue";
import ContextMenu, { type ContextMenuItem } from "../ui/ContextMenu.vue";
import UiButton from "../ui/UiButton.vue";
import { dirNameOf, fileNameOf } from "../workbench/workspace-mode";
import {
  SCM_GROUP_LABEL,
  scmFileTree,
  scmResourceLetter,
  type ScmGroupId,
  type ScmTreeNode,
} from "../workbench/scm-groups";
import GitStatusBadge from "../workbench/GitStatusBadge.vue";
import { folderGitStatus, gitStatusClass, gitStatusTitle } from "../workbench/tree-git-status";
import { treeGuideLeft, treeRowPad } from "../workbench/tree-layout";
import { scmBusyTitle } from "../workbench/scm-commit-action";

const props = defineProps<{
  workspaceId: string;
  groups: Array<{ id: ScmGroupId; files: GitFileStatus[] }>;
  viewMode: "list" | "tree";
  busy: string;
  readonly?: boolean;
}>();
const emit = defineEmits<{
  open: [payload: { group: ScmGroupId; path: string }];
  openFile: [path: string];
  stage: [payload: { paths: string[]; staged: boolean }];
  discard: [files: Array<{ path: string; worktreeStatus: string }>];
}>();

const closedGroups = ref(new Set<string>());
const closedDirs = ref(new Set<string>());
const selected = ref(new Set<string>());
const anchor = ref("");
const menu = ref<{ x: number; y: number; group: ScmGroupId; paths: string[] } | null>(null);

const menuItems = computed<ContextMenuItem[]>(() => {
  if (!menu.value) return [];
  const many = menu.value.paths.length > 1;
  const group = menu.value.group;
  const items: ContextMenuItem[] = [
    { id: "open", label: many ? t("scm.resource.openSelected") : t("workbench.gitOpenChanges"), disabled: many },
    { id: "openFile", label: t("scm.resource.openFile"), disabled: many },
  ];
  if (props.readonly) return items;
  if (group === "staged") {
    items.push({ id: "unstage", label: many ? t("scm.resource.unstageSelected") : t("scm.unstage") });
  } else {
    items.push({ id: "discard", label: many ? t("scm.resource.discardSelected") : t("scm.resource.discardChanges") });
    items.push({
      id: "stage",
      label: many ? t("scm.resource.stageSelected") : group === "merge" ? t("scm.resource.markResolved") : t("workbench.gitStage"),
    });
  }
  return items;
});

function groupKey(group: ScmGroupId, path = "") {
  return path ? `${group}:${path}` : group;
}

function isGroupOpen(group: ScmGroupId) {
  return !closedGroups.value.has(group);
}

function toggleGroup(group: ScmGroupId) {
  const next = new Set(closedGroups.value);
  if (next.has(group)) next.delete(group);
  else next.add(group);
  closedGroups.value = next;
}

function toggleDir(group: ScmGroupId, path: string) {
  const key = groupKey(group, path);
  const next = new Set(closedDirs.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  closedDirs.value = next;
}

type Row =
  | { kind: "dir"; path: string; name: string; depth: number; count: number }
  | { kind: "file"; file: GitFileStatus; depth: number };

const rowsByGroup = computed(
  () =>
    new Map(
      props.groups
        .filter((group) => isGroupOpen(group.id))
        .map((group) => [group.id, rowsFor(group.id, group.files)]),
    ),
);

const filesByGroup = computed(
  () =>
    new Map(
      props.groups.map((group) => [group.id, new Map(group.files.map((file) => [file.path, file]))]),
    ),
);

function rowsFor(group: ScmGroupId, files: GitFileStatus[]): Row[] {
  if (props.viewMode === "list") {
    return [...files]
      .sort((a, b) => a.path.localeCompare(b.path))
      .map((file) => ({ kind: "file", file, depth: 0 }));
  }
  const flat: Row[] = [];
  const walk = (nodes: ScmTreeNode[], depth: number) => {
    for (const node of nodes) {
      if (node.kind === "dir") {
        flat.push({ kind: "dir", path: node.path, name: node.name, depth, count: node.count });
        if (!closedDirs.value.has(groupKey(group, node.path))) walk(node.children, depth + 1);
      } else {
        flat.push({ kind: "file", file: node.file, depth });
      }
    }
  };
  walk(scmFileTree(files), 0);
  return flat;
}

function fileIds(group: ScmGroupId) {
  return props.groups.find((item) => item.id === group)?.files.map((file) => groupKey(group, file.path)) ?? [];
}

function onFileClick(group: ScmGroupId, file: GitFileStatus, event: MouseEvent) {
  const id = groupKey(group, file.path);
  const ids = fileIds(group);
  if (event.shiftKey && anchor.value && ids.includes(anchor.value)) {
    const start = ids.indexOf(anchor.value);
    const end = ids.indexOf(id);
    const [from, to] = start < end ? [start, end] : [end, start];
    selected.value = new Set(ids.slice(from, to + 1));
    return;
  }
  if (event.metaKey || event.ctrlKey) {
    const next = new Set(selected.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected.value = next;
    anchor.value = id;
    return;
  }
  selected.value = new Set([id]);
  anchor.value = id;
  emit("open", { group, path: file.path });
}

function selectedPaths(group: ScmGroupId, path: string) {
  const id = groupKey(group, path);
  const files = props.groups.find((item) => item.id === group)?.files ?? [];
  if (!selected.value.has(id)) return [path];
  const picked = files.filter((file) => selected.value.has(groupKey(group, file.path))).map((file) => file.path);
  return picked.length ? picked : [path];
}

function openMenu(group: ScmGroupId, path: string, event: MouseEvent) {
  event.preventDefault();
  menu.value = { x: event.clientX, y: event.clientY, group, paths: selectedPaths(group, path) };
}

function filesFor(group: ScmGroupId, paths: string[]) {
  const files = filesByGroup.value.get(group);
  return paths.map((path) => ({ path, worktreeStatus: files?.get(path)?.worktreeStatus ?? "M" }));
}

function onMenuSelect(id: string) {
  const current = menu.value;
  menu.value = null;
  if (!current) return;
  if (id === "open" && current.paths.length === 1) {
    emit("open", { group: current.group, path: current.paths[0] });
  }
  if (id === "openFile" && current.paths.length === 1) {
    emit("openFile", current.paths[0]);
  }
  if (id === "stage") emit("stage", { paths: current.paths, staged: true });
  if (id === "unstage") emit("stage", { paths: current.paths, staged: false });
  if (id === "discard") emit("discard", filesFor(current.group, current.paths));
}

function rowDepth(row: Row) {
  return props.viewMode === "list" ? 1 : row.depth + 1;
}

function rowPad(row: Row) {
  return `${treeRowPad(rowDepth(row))}px`;
}

function rowName(row: Row) {
  return row.kind === "dir" ? row.name : fileNameOf(row.file.path);
}

function rowTitle(row: Row) {
  return row.kind === "dir" ? row.path : row.file.path;
}

function rowStatusLetter(group: ScmGroupId, row: Row) {
  if (row.kind === "file") return scmResourceLetter(row.file, group);
  const prefix = `${row.path}/`;
  const letters = (props.groups.find((item) => item.id === group)?.files ?? [])
    .filter((file) => file.path === row.path || file.path.startsWith(prefix))
    .map((file) => scmResourceLetter(file, group));
  return folderGitStatus(letters);
}

function rowStatusClass(group: ScmGroupId, row: Row) {
  return gitStatusClass(rowStatusLetter(group, row));
}

function rowStatusTitle(group: ScmGroupId, row: Row) {
  if (row.kind === "file") return gitStatusTitle(scmResourceLetter(row.file, group));
  return row.count > 0 ? t("git.folderDirty", { count: row.count }) : "";
}

function onRowClick(group: ScmGroupId, row: Row, event: MouseEvent) {
  if (row.kind === "dir") {
    toggleDir(group, row.path);
    return;
  }
  onFileClick(group, row.file, event);
}

function onRowEnter(group: ScmGroupId, row: Row) {
  if (row.kind === "dir") {
    toggleDir(group, row.path);
    return;
  }
  emit("open", { group, path: row.file.path });
}

function onRowMenu(group: ScmGroupId, row: Row, event: MouseEvent) {
  if (row.kind !== "file") return;
  openMenu(group, row.file.path, event);
}

function stageAllPaths(group: { id: ScmGroupId; files: GitFileStatus[] }) {
  const paths = group.files.map((file) => file.path);
  if (group.id !== "changes") return paths;
  const untracked = props.groups.find((item) => item.id === "untracked");
  return paths.concat(untracked?.files.map((file) => file.path) ?? []);
}
</script>

<template>
  <div v-for="group in groups" :key="group.id" class="scm-group scm-file-tree">
    <header @click="toggleGroup(group.id)">
      <button type="button" class="scm-group-toggle">
        <Codicon name="chevron-down" class="tree-chevron" :class="{ 'is-open': isGroupOpen(group.id) }" :size="16" />
        {{ SCM_GROUP_LABEL[group.id] }}
      </button>
      <small class="scm-group-count">{{ group.files.length }}</small>
      <div v-if="!readonly" class="scm-group-actions">
        <UiButton
          v-if="group.id !== 'staged'"
          variant="ghost"
          size="icon"
          :tip="scmBusyTitle(busy, `discard:${workspaceId}`, t('scm.resource.discardAll'))"
          :disabled="Boolean(busy)"
          @click.stop="emit('discard', group.files.map((file) => ({ path: file.path, worktreeStatus: file.worktreeStatus })))"
        >
          <Codicon name="discard" :size="12" />
        </UiButton>
        <UiButton
          v-if="group.id === 'staged'"
          variant="ghost"
          size="icon"
          :tip="scmBusyTitle(busy, `stage:${workspaceId}`, t('scm.resource.unstageAll'))"
          :disabled="Boolean(busy)"
          @click.stop="emit('stage', { paths: group.files.map((file) => file.path), staged: false })"
        >
          <Codicon name="remove" :size="12" />
        </UiButton>
        <UiButton
          v-else
          variant="ghost"
          size="icon"
          :tip="scmBusyTitle(busy, `stage:${workspaceId}`, group.id === 'merge' ? t('scm.resource.markAllResolved') : t('scm.resource.stageAll'))"
          :disabled="Boolean(busy)"
          @click.stop="emit('stage', { paths: stageAllPaths(group), staged: true })"
        >
          <Codicon name="add" :size="12" />
        </UiButton>
      </div>
    </header>
    <template v-if="isGroupOpen(group.id)">
      <template v-for="row in rowsByGroup.get(group.id) ?? []" :key="row.kind === 'dir' ? `d:${row.path}` : `f:${row.file.path}`">
        <div
          :class="[
            viewMode === 'tree' ? 'tree-entry' : 'scm-file',
            rowStatusClass(group.id, row),
            {
              selected: row.kind === 'file' && selected.has(groupKey(group.id, row.file.path)),
              'is-selected': row.kind === 'file' && selected.has(groupKey(group.id, row.file.path)),
            },
          ]"
          :style="{ paddingLeft: rowPad(row), '--tree-depth': rowDepth(row) }"
          role="button"
          tabindex="0"
          @click="onRowClick(group.id, row, $event)"
          @keyup.enter="onRowEnter(group.id, row)"
          @contextmenu="onRowMenu(group.id, row, $event)"
        >
          <span
            v-for="level in rowDepth(row)"
            :key="`guide:${level}`"
            class="tree-indent-guide"
            :class="{ innermost: level === rowDepth(row) }"
            :style="{ left: `${treeGuideLeft(level)}px` }"
          />
          <Codicon
            v-if="viewMode === 'tree' && row.kind === 'dir'"
            name="chevron-down"
            class="tree-chevron"
            :class="{ 'is-open': !closedDirs.has(groupKey(group.id, row.path)) }"
            :size="10"
          />
          <span v-else-if="viewMode === 'tree'" class="tree-chevron" />
          <FileTypeIcon
            :name="rowName(row)"
            :directory="row.kind === 'dir'"
            :open="row.kind === 'dir' && !closedDirs.has(groupKey(group.id, row.path))"
          />
          <UiTooltip v-if="viewMode === 'tree'" :content="rowTitle(row)">
            <span class="tree-name">{{ rowName(row) }}</span>
          </UiTooltip>
          <span v-else-if="row.kind === 'file'" class="scm-file-label">
            <span class="scm-file-name">{{ fileNameOf(row.file.path) }}</span>
            <UiTooltip v-if="dirNameOf(row.file.path)" :content="row.file.path">
              <span class="scm-file-path">{{ dirNameOf(row.file.path) }}</span>
            </UiTooltip>
          </span>
          <div v-if="row.kind === 'file'" class="scm-file-actions">
            <UiButton
              v-if="row.file.worktreeStatus !== 'D' && row.file.indexStatus !== 'D'"
              variant="ghost"
              size="icon"
              :tip="t('scm.resource.openFile')"
              @click.stop="emit('openFile', row.file.path)"
            >
              <Codicon name="go-to-file" :size="12" />
            </UiButton>
            <UiButton
              v-if="group.id !== 'staged'"
              variant="ghost"
              size="icon"
              :tip="t('scm.resource.discardChanges')"
              @click.stop="emit('discard', [{ path: row.file.path, worktreeStatus: row.file.worktreeStatus }])"
            >
              <Codicon name="discard" :size="12" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon"
              :tip="group.id === 'staged' ? t('scm.unstage') : t('workbench.gitStage')"
              @click.stop="emit('stage', { paths: [row.file.path], staged: group.id !== 'staged' })"
            >
              <Codicon v-if="group.id === 'staged'" name="remove" :size="12" />
              <Codicon v-else name="add" :size="12" />
            </UiButton>
          </div>
          <UiTooltip v-if="rowStatusLetter(group.id, row)" :content="rowStatusTitle(group.id, row)">
            <GitStatusBadge :letter="rowStatusLetter(group.id, row)" :bubble="row.kind === 'dir'" />
          </UiTooltip>
        </div>
      </template>
    </template>
  </div>
  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="menuItems"
    @select="onMenuSelect"
    @close="menu = null"
  />
</template>
