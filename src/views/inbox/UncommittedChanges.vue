<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import CommitMultiDiffEditor, { type CommitDiffFile } from "../../components/scm/CommitMultiDiffEditor.vue";
import Codicon from "../../components/ui/Codicon.vue";
import ContextMenu, { type ContextMenuItem } from "../../components/ui/ContextMenu.vue";
import FileTypeIcon from "../../components/FileTypeIcon.vue";
import UiButton from "../../components/ui/UiButton.vue";
import { loadScmDiff } from "../../components/workbench/scm-diff";
import { scmGroups, scmResourceLetter, type ScmGroupId } from "../../components/workbench/scm-groups";
import { fileNameOf } from "../../components/workbench/workspace-mode";

const SIDE_KEY = "multara.inbox.uncommitted.sideBySide";
const WRAP_KEY = "multara.inbox.uncommitted.wordWrap";
const FILES_KEY = "multara.inbox.uncommitted.files";
const MODE_KEY = "multara.inbox.uncommitted.filesMode";
const POS_KEY = "multara.inbox.uncommitted.filesPos";
const GROUP_ORDER: ScmGroupId[] = ["changes", "merge", "staged"];

const props = defineProps<{
  projectId: string;
  theme: string;
}>();

const files = ref<CommitDiffFile[]>([]);
const loading = ref(false);
const sideBySide = ref(localStorage.getItem(SIDE_KEY) !== "0");
const wordWrap = ref(localStorage.getItem(WRAP_KEY) !== "0");
const filesOpen = ref(localStorage.getItem(FILES_KEY) !== "0");
const filesMode = ref<"list" | "tree">(localStorage.getItem(MODE_KEY) === "tree" ? "tree" : "list");
const filesPos = ref<"left" | "right">(localStorage.getItem(POS_KEY) === "left" ? "left" : "right");
const expandEpoch = ref(0);
const focusPath = ref("");
const menu = ref<{ x: number; y: number } | null>(null);

function remember(key: string, value: string) {
  localStorage.setItem(key, value);
}

const menuItems = computed<ContextMenuItem[]>(() => [
  { id: "expand", label: t("inbox.changes.expandAll") },
  { id: "wrap", label: t("inbox.changes.wordWrap"), checked: wordWrap.value },
  { type: "sep" },
  { id: "inline", label: t("inbox.changes.diffInline"), checked: !sideBySide.value },
  { id: "side", label: t("inbox.changes.diffSide"), checked: sideBySide.value },
  { type: "sep" },
  { id: "files", label: t("inbox.changes.showFiles"), checked: filesOpen.value },
  { id: "list", label: t("inbox.changes.filesList"), checked: filesMode.value === "list" },
  { id: "tree", label: t("inbox.changes.filesTree"), checked: filesMode.value === "tree" },
  { id: "pos-left", label: t("inbox.changes.filesLeft"), checked: filesPos.value === "left" },
  { id: "pos-right", label: t("inbox.changes.filesRight"), checked: filesPos.value === "right" },
]);

type FileRow = { key: string; depth: number; name: string; path: string; directory: boolean };
type DirNode = { dirs: Map<string, DirNode>; files: string[] };

function treeRows(list: CommitDiffFile[]): FileRow[] {
  const root: DirNode = { dirs: new Map(), files: [] };
  for (const file of list) {
    const parts = file.path.split("/");
    let node = root;
    for (const part of parts.slice(0, -1)) {
      let next = node.dirs.get(part);
      if (!next) {
        next = { dirs: new Map(), files: [] };
        node.dirs.set(part, next);
      }
      node = next;
    }
    node.files.push(file.path);
  }
  const flat: FileRow[] = [];
  const walk = (node: DirNode, prefix: string, depth: number) => {
    for (const [name, child] of node.dirs) {
      const path = prefix ? `${prefix}/${name}` : name;
      flat.push({ key: `dir:${path}`, depth, name, path, directory: true });
      walk(child, path, depth + 1);
    }
    for (const filePath of node.files) {
      flat.push({ key: filePath, depth, name: fileNameOf(filePath), path: filePath, directory: false });
    }
  };
  walk(root, "", 0);
  return flat;
}

const rows = computed<FileRow[]>(() => {
  if (filesMode.value === "list") {
    return files.value.map((file) => ({
      key: file.path,
      depth: 0,
      name: fileNameOf(file.path),
      path: file.path,
      directory: false,
    }));
  }
  return treeRows(files.value);
});

async function load() {
  if (!props.projectId) {
    files.value = [];
    return;
  }
  loading.value = true;
  try {
    const repos = await backend.listProjectGitStatus(props.projectId);
    const targets: Array<{ workspaceId: string; path: string; group: ScmGroupId; status: string }> = [];
    const seen = new Set<string>();
    for (const repo of repos ?? []) {
      const groups = scmGroups(repo.changes ?? []);
      for (const group of GROUP_ORDER) {
        for (const file of groups[group]) {
          const key = `${repo.workspaceId}\n${file.path}`;
          if (seen.has(key)) continue;
          seen.add(key);
          targets.push({
            workspaceId: repo.workspaceId,
            path: file.path,
            group,
            status: scmResourceLetter(file, group),
          });
        }
      }
    }
    const loaded = await Promise.all(targets.map(async (target) => {
      const diff = await loadScmDiff(target, {
        readWorktree: async (path) => {
          const file = await backend.readWorkspaceFile(target.workspaceId, path);
          return { content: file.content, language: file.language };
        },
        readIndex: async (path) => {
          const file = await backend.gitIndexFile(target.workspaceId, path);
          return { content: file.content, binary: file.binary };
        },
        readRevision: async (revision, path) => {
          const file = await backend.gitRevisionFile(target.workspaceId, revision, path);
          return { content: file.content, binary: file.binary };
        },
      });
      return {
        path: target.path,
        status: target.status,
        original: diff.original,
        modified: diff.modified,
        language: diff.language,
        binary: diff.binary,
      } satisfies CommitDiffFile;
    }));
    files.value = loaded;
  } catch {
    files.value = [];
  } finally {
    loading.value = false;
  }
}

function pickFile(row: FileRow) {
  if (row.directory) return;
  focusPath.value = row.path;
}

function openMenu(event: MouseEvent) {
  const button = event.currentTarget;
  if (!(button instanceof HTMLElement)) return;
  const rect = button.getBoundingClientRect();
  menu.value = { x: rect.left, y: rect.bottom };
}

function onMenu(id: string) {
  const keepsOpen = id !== "expand";
  if (!keepsOpen) menu.value = null;
  if (id === "expand") {
    expandEpoch.value += 1;
    return;
  }
  if (id === "wrap") {
    wordWrap.value = !wordWrap.value;
    remember(WRAP_KEY, wordWrap.value ? "1" : "0");
    return;
  }
  if (id === "inline" || id === "side") {
    sideBySide.value = id === "side";
    remember(SIDE_KEY, sideBySide.value ? "1" : "0");
    return;
  }
  if (id === "files") {
    filesOpen.value = !filesOpen.value;
    remember(FILES_KEY, filesOpen.value ? "1" : "0");
    return;
  }
  if (id === "list" || id === "tree") {
    filesMode.value = id;
    remember(MODE_KEY, id);
    return;
  }
  if (id === "pos-left" || id === "pos-right") {
    filesPos.value = id === "pos-left" ? "left" : "right";
    remember(POS_KEY, filesPos.value);
  }
}

onMounted(() => {
  void load();
});

watch(() => props.projectId, () => {
  void load();
});
</script>

<template>
  <section class="uncommitted-changes" :class="{ 'files-left': filesOpen && filesPos === 'left' }">
    <header class="uncommitted-changes-bar">
      <strong>{{ t("inbox.tool.uncommitted") }}</strong>
      <UiButton variant="ghost" size="icon" :tip="t('inbox.changes.menu')" @click="openMenu">
        <Codicon name="ellipsis" :size="16" />
      </UiButton>
    </header>
    <div class="uncommitted-changes-body">
      <div class="uncommitted-changes-diff">
        <p v-if="loading" class="tree-empty">{{ t("scm.loadingStatus") }}</p>
        <p v-else-if="!files.length" class="tree-empty">{{ t("inbox.changes.empty") }}</p>
        <CommitMultiDiffEditor
          v-else
          diff-id="uncommitted"
          :files="files"
          :theme="theme"
          :side-by-side="sideBySide"
          :word-wrap="wordWrap"
          :expand-epoch="expandEpoch"
          :focus-path="focusPath"
        />
      </div>
      <aside v-if="filesOpen" class="uncommitted-changes-files vs-scroll">
        <p v-if="!files.length" class="tree-empty">{{ loading ? t("scm.loadingStatus") : t("inbox.changes.empty") }}</p>
        <button
          v-for="row in rows"
          :key="row.key"
          type="button"
          class="uncommitted-changes-file"
          :class="{ active: !row.directory && row.path === focusPath }"
          :style="{ paddingLeft: `${8 + row.depth * 12}px` }"
          @click="pickFile(row)"
        >
          <FileTypeIcon :name="row.name" :directory="row.directory" />
          <span>{{ row.name }}</span>
        </button>
      </aside>
    </div>
    <ContextMenu
      v-if="menu"
      fit
      :x="menu.x"
      :y="menu.y"
      :items="menuItems"
      @select="onMenu"
      @close="menu = null"
    />
  </section>
</template>
