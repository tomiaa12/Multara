<script setup lang="ts">
import { computed, ref } from "vue";
import type { GitFileStatus } from "../../domain/types";
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
import { gitStatusClass, gitStatusTitle } from "../workbench/tree-git-status";
import { scmBusyTitle } from "../workbench/scm-commit-action";

const props = defineProps<{
  workspaceId: string;
  groups: Array<{ id: ScmGroupId; files: GitFileStatus[] }>;
  viewMode: "list" | "tree";
  busy: string;
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
    { id: "open", label: many ? "打开所选" : "打开更改", disabled: many },
    { id: "openFile", label: "打开文件", disabled: many },
  ];
  if (group === "staged") {
    items.push({ id: "unstage", label: many ? "取消暂存所选" : "取消暂存" });
  } else {
    items.push({ id: "discard", label: many ? "撤回所选" : "撤回更改" });
    items.push({ id: "stage", label: many ? "暂存所选" : group === "merge" ? "标记为已解决" : "暂存更改" });
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
  const files = props.groups.find((item) => item.id === group)?.files ?? [];
  return paths.map((path) => {
    const file = files.find((item) => item.path === path);
    return { path, worktreeStatus: file?.worktreeStatus ?? "M" };
  });
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

const SCM_INDENT = 16;

function visualDepth(row: Row) {
  return props.viewMode === "list" ? 2 : 2 + row.depth;
}

function rowPad(row: Row) {
  return `${visualDepth(row) * SCM_INDENT}px`;
}

function guideLeft(level: number) {
  return level * SCM_INDENT + SCM_INDENT / 2;
}
</script>

<template>
  <div v-for="group in groups" :key="group.id" class="scm-group">
    <header @click="toggleGroup(group.id)">
      <button type="button" class="scm-group-toggle">
        <Codicon name="chevron-down" class="tree-chevron" :class="{ 'is-open': isGroupOpen(group.id) }" :size="16" />
        {{ SCM_GROUP_LABEL[group.id] }}
      </button>
      <small class="scm-group-count">{{ group.files.length }}</small>
      <div class="scm-group-actions">
        <UiButton
          v-if="group.id !== 'staged'"
          variant="ghost"
          size="icon"
          :title="scmBusyTitle(busy, `discard:${workspaceId}`, '撤回全部更改')"
          :disabled="Boolean(busy)"
          @click.stop="emit('discard', group.files.map((file) => ({ path: file.path, worktreeStatus: file.worktreeStatus })))"
        >
          <Codicon name="discard" :size="12" />
        </UiButton>
        <UiButton
          v-if="group.id === 'staged'"
          variant="ghost"
          size="icon"
          :title="scmBusyTitle(busy, `stage:${workspaceId}`, '全部取消暂存')"
          :disabled="Boolean(busy)"
          @click.stop="emit('stage', { paths: group.files.map((file) => file.path), staged: false })"
        >
          <Codicon name="remove" :size="12" />
        </UiButton>
        <UiButton
          v-else
          variant="ghost"
          size="icon"
          :title="scmBusyTitle(busy, `stage:${workspaceId}`, group.id === 'merge' ? '全部标记为已解决' : '全部暂存')"
          :disabled="Boolean(busy)"
          @click.stop="emit('stage', { paths: group.files.map((file) => file.path), staged: true })"
        >
          <Codicon name="add" :size="12" />
        </UiButton>
      </div>
    </header>
    <template v-if="isGroupOpen(group.id)">
      <template v-for="row in rowsFor(group.id, group.files)" :key="row.kind === 'dir' ? `d:${row.path}` : `f:${row.file.path}`">
        <button
          v-if="row.kind === 'dir'"
          type="button"
          class="scm-dir"
          :style="{ paddingLeft: rowPad(row) }"
          @click="toggleDir(group.id, row.path)"
        >
          <span
            v-for="level in visualDepth(row) - 1"
            :key="`guide:${level}`"
            class="tree-indent-guide"
            :class="{ innermost: level === visualDepth(row) - 1 }"
            :style="{ left: `${guideLeft(level)}px` }"
          />
          <Codicon name="chevron-down" class="tree-chevron" :class="{ 'is-open': !closedDirs.has(groupKey(group.id, row.path)) }" :size="10" />
          <FileTypeIcon :name="row.name" directory :open="!closedDirs.has(groupKey(group.id, row.path))" />
          <span>{{ row.name }}</span>
          <small>{{ row.count }}</small>
        </button>
        <div
          v-else
          class="scm-file"
          :class="[gitStatusClass(scmResourceLetter(row.file, group.id)), { 'is-selected': selected.has(groupKey(group.id, row.file.path)) }]"
          :style="{ paddingLeft: rowPad(row) }"
          role="button"
          tabindex="0"
          @click="onFileClick(group.id, row.file, $event)"
          @keyup.enter="emit('open', { group: group.id, path: row.file.path })"
          @contextmenu="openMenu(group.id, row.file.path, $event)"
        >
          <span
            v-for="level in visualDepth(row) - 1"
            :key="`guide:${level}`"
            class="tree-indent-guide"
            :class="{ innermost: level === visualDepth(row) - 1 }"
            :style="{ left: `${guideLeft(level)}px` }"
          />
          <FileTypeIcon :name="fileNameOf(row.file.path)" :size="16" />
          <span class="scm-file-name">{{ fileNameOf(row.file.path) }}</span>
          <span v-if="viewMode === 'list'" class="scm-file-path" :title="row.file.path">{{ dirNameOf(row.file.path) }}</span>
          <span v-else class="scm-file-path" />
          <div class="scm-file-actions">
            <UiButton
              v-if="row.file.worktreeStatus !== 'D' && row.file.indexStatus !== 'D'"
              variant="ghost"
              size="icon"
              title="打开文件"
              @click.stop="emit('openFile', row.file.path)"
            >
              <Codicon name="go-to-file" :size="12" />
            </UiButton>
            <UiButton
              v-if="group.id !== 'staged'"
              variant="ghost"
              size="icon"
              title="撤回更改"
              @click.stop="emit('discard', [{ path: row.file.path, worktreeStatus: row.file.worktreeStatus }])"
            >
              <Codicon name="discard" :size="12" />
            </UiButton>
            <UiButton
              variant="ghost"
              size="icon"
              :title="group.id === 'staged' ? '取消暂存' : '暂存更改'"
              @click.stop="emit('stage', { paths: [row.file.path], staged: group.id !== 'staged' })"
            >
              <Codicon v-if="group.id === 'staged'" name="remove" :size="12" />
              <Codicon v-else name="add" :size="12" />
            </UiButton>
          </div>
          <b :title="gitStatusTitle(scmResourceLetter(row.file, group.id))">{{ scmResourceLetter(row.file, group.id) }}</b>
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
