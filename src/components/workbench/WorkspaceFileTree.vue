<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { Workspace } from "../../domain/types";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import Codicon from "../ui/Codicon.vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import UiTooltip from "../ui/UiTooltip.vue";
import ExplorerTreeList from "./ExplorerTreeList.vue";
import { buildExplorerRows, type ExplorerRowNode } from "./explorer/explorer-rows";
import { treeGuideLeft, treeRowPad } from "./tree-layout";
import type { FileEntry } from "../../domain/types";

const EXPANDED_PREFIX = "multara.fileTree.expanded.";

const props = defineProps<{
  workspace?: Workspace;
  /** 展开状态按这个键和工作区分开记，收件箱和工作台不要用同一个。 */
  stateKey: string;
  /** 嵌进多仓库列表时不再自己滚动。 */
  embedded?: boolean;
  selectedPath?: string;
  /** 不走工作区目录时用这个读子项，例如 Skill 目录。 */
  listDirectory?: (path: string) => Promise<FileEntry[]>;
}>();

const emit = defineEmits<{
  open: [path: string];
  pin: [path: string];
}>();

const levels = ref<Record<string, FileEntry[]>>({});
const expanded = ref(new Set<string>());
const loading = ref(false);
const failed = ref(false);

const storageKey = computed(() => `${EXPANDED_PREFIX}${props.stateKey}.${props.workspace?.id ?? "local"}`);

const nodes = computed<ExplorerRowNode[]>(() => buildExplorerRows({
  workspaceId: props.workspace?.id ?? props.stateKey,
  workspaceName: props.workspace?.name ?? "",
  roots: levels.value[""],
  childrenOf: (path) => levels.value[path],
  isExpanded: (path) => expanded.value.has(path),
  isNestExpanded: () => false,
  sort: { sortOrder: "default", lexicographic: "default", reverse: false },
  compactFolders: true,
  nestingPatterns: null,
  isHidden: () => false,
  forceVisible: () => false,
  breakCompactAt: () => false,
  missingFiles: [],
}));

function readExpanded() {
  try {
    const raw = JSON.parse(localStorage.getItem(storageKey.value) || "[]");
    if (!Array.isArray(raw)) return new Set<string>();
    return new Set(raw.filter((item) => typeof item === "string"));
  } catch {
    return new Set<string>();
  }
}

function writeExpanded() {
  localStorage.setItem(storageKey.value, JSON.stringify([...expanded.value]));
}

async function loadDir(path: string) {
  const entries = props.listDirectory
    ? await props.listDirectory(path)
    : await backend.listWorkspaceDirectory(props.workspace?.id ?? "", path || undefined);
  levels.value = { ...levels.value, [path]: entries ?? [] };
}

async function reload() {
  loading.value = true;
  failed.value = false;
  levels.value = {};
  expanded.value = readExpanded();
  try {
    await loadDir("");
  } catch {
    failed.value = true;
    loading.value = false;
    return;
  }
  const next = new Set(expanded.value);
  for (const path of next) {
    if (!path) continue;
    try {
      await loadDir(path);
    } catch {
      next.delete(path);
    }
  }
  expanded.value = next;
  writeExpanded();
  loading.value = false;
}

async function toggle(path: string) {
  const next = new Set(expanded.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
    if (!levels.value[path]) await loadDir(path);
  }
  expanded.value = next;
  writeExpanded();
}

function onRow(entry: { path: string; isDirectory: boolean }) {
  if (entry.isDirectory) {
    void toggle(entry.path);
    return;
  }
  emit("open", entry.path);
}

function onPin(entry: { path: string; isDirectory: boolean }) {
  if (entry.isDirectory) return;
  emit("pin", entry.path);
}

watch(() => props.workspace?.id ?? props.stateKey, () => {
  void reload();
}, { immediate: true });
</script>

<template>
  <div class="workspace-file-tree" :class="embedded ? 'is-embedded' : 'file-tree vs-scroll'">
    <p v-if="loading && !nodes.length" class="tree-empty">{{ t("scm.loadingStatus") }}</p>
    <p v-else-if="failed" class="tree-empty">{{ t("inbox.tool.emptyTree") }}</p>
    <p v-else-if="!nodes.length" class="tree-empty">{{ t("inbox.tool.emptyTree") }}</p>
    <ExplorerTreeList v-else :nodes="nodes">
      <template #row="{ entry, sticky }">
        <div
          class="tree-entry"
          role="button"
          tabindex="0"
          :class="{ 'is-sticky': sticky, selected: props.selectedPath === entry.path }"
          :style="{ paddingLeft: `${treeRowPad(entry.depth)}px`, '--tree-depth': entry.depth }"
          @click="onRow(entry)"
          @dblclick.stop="onPin(entry)"
          @keydown.enter.prevent="onRow(entry)"
        >
          <span
            v-for="level in entry.depth"
            :key="level"
            class="tree-indent-guide"
            :class="{ innermost: level === entry.depth }"
            :style="{ left: `${treeGuideLeft(level)}px` }"
          />
          <Codicon
            v-if="entry.expandable"
            name="chevron-down"
            class="tree-chevron"
            :class="{ 'is-open': entry.expanded }"
            :size="10"
          />
          <span v-else class="tree-chevron" />
          <FileTypeIcon
            :name="entry.compact ? entry.compact.names.at(-1) || entry.name : entry.name"
            :directory="entry.isDirectory"
            :open="entry.expanded"
          />
          <UiTooltip :content="entry.compact ? entry.compact.names.join('/') : entry.name">
            <span class="tree-name">{{ entry.compact ? entry.compact.names.join(" / ") : entry.name }}</span>
          </UiTooltip>
        </div>
      </template>
    </ExplorerTreeList>
  </div>
</template>
