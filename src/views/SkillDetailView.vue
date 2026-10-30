<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, shallowRef, watch } from "vue";
import { FileText, LayoutDashboard, Sparkles } from "@lucide/vue";
import { loader, VueMonacoEditor } from "@guolao/vue-monaco-editor";
import { useRoute, useRouter } from "vue-router";
import type { FileContent, FileEntry } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import Codicon from "../components/ui/Codicon.vue";
import FileTypeIcon from "../components/FileTypeIcon.vue";
import ExplorerTreeList from "../components/workbench/ExplorerTreeList.vue";
import { monacoZoomOptions } from "../components/workbench/monaco-zoom";
import type { ExplorerRowNode } from "../components/workbench/explorer/explorer-rows";
import { nestExplorerRows, type ExplorerTreeNode } from "../components/workbench/explorer-tree";
import { monaco, monacoLanguageId } from "../monaco";
import { currentDocumentTheme, monacoThemeName, watchDocumentTheme } from "../editor/monaco-workspace";
import { t } from "../i18n";
import { relativeTime } from "./task-board";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";

loader.config({ monaco });

type Pane = "overview" | "files";
// 与工作台文件树同一套缩进。
const TREE_ROW_INSET = 18;
const TREE_INDENT = 14;
const TREE_CHEVRON = 16;

const route = useRoute();
const router = useRouter();
const store = useAppStore();
const pane = ref<Pane>("overview");
const fileCount = ref<number | null>(null);
const roots = ref<FileEntry[]>([]);
const children = ref<Record<string, FileEntry[]>>({});
const expanded = ref<Set<string>>(new Set());
const selectedPath = ref("");
const openFile = ref<FileContent | null>(null);
const draft = ref("");
const savedText = ref("");
const fileError = ref("");
const saving = ref(false);
const treeError = ref("");
let openToken = 0;

const resourceId = computed(() => String(route.params.resourceId ?? ""));
const skill = computed(() => store.data?.resources.find((item) => item.id === resourceId.value && item.kind === "skill") ?? null);
const users = computed(() => store.data?.agents.filter((agent) => agent.skillIds.includes(resourceId.value)) ?? []);
const dirty = computed(() => openFile.value !== null && draft.value !== savedText.value);
const sourceName = computed(() => {
  const owner = skill.value?.owner;
  if (!owner || owner === "multara") return "Multara";
  return store.data?.runtimes.find((runtime) => runtime.id === owner)?.name ?? owner;
});

function treeRowPad(depth: number) {
  return TREE_ROW_INSET + depth * TREE_INDENT;
}

function treeGuideLeft(level: number) {
  return TREE_ROW_INSET + (level - 1) * TREE_INDENT + TREE_CHEVRON / 2;
}

function toExplorerRowNodes(nodes: ExplorerTreeNode[]): ExplorerRowNode[] {
  return nodes.map((node) => ({
    entry: {
      ...node.entry,
      workspaceId: skill.value?.id ?? "",
      expandable: node.entry.isDirectory,
      expanded: node.expanded,
    },
    expanded: node.expanded,
    children: toExplorerRowNodes(node.children),
  }));
}

const treeNodes = computed(() => {
  const rows: FileEntry[] = [];
  const append = (entries: FileEntry[], depth: number) => {
    for (const entry of entries) {
      rows.push({ ...entry, depth });
      if (entry.isDirectory && expanded.value.has(entry.path)) {
        append(children.value[entry.path] ?? [], depth + 1);
      }
    }
  };
  append(roots.value, 0);
  return toExplorerRowNodes(
    nestExplorerRows(rows, (entry) => entry.isDirectory && expanded.value.has(entry.path)),
  );
});

async function loadCount() {
  if (!skill.value) return;
  try {
    fileCount.value = await backend.skillFileCount(skill.value.id);
  } catch {
    fileCount.value = null;
  }
}

async function loadRoot() {
  if (!skill.value) return;
  treeError.value = "";
  try {
    roots.value = await backend.listSkillDirectory(skill.value.id);
    const skillFile = roots.value.find((entry) => entry.name === "SKILL.md" && !entry.isDirectory);
    if (skillFile && !selectedPath.value) await openEntry(skillFile);
  } catch (cause) {
    treeError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function toggleDirectory(skillId: string, entry: FileEntry) {
  if (expanded.value.has(entry.path)) {
    expanded.value.delete(entry.path);
    expanded.value = new Set(expanded.value);
    return;
  }
  try {
    if (!children.value[entry.path]) {
      children.value = {
        ...children.value,
        [entry.path]: await backend.listSkillDirectory(skillId, entry.path),
      };
    }
    expanded.value = new Set(expanded.value).add(entry.path);
  } catch (cause) {
    treeError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function openEntry(entry: FileEntry) {
  if (!skill.value) return;
  if (entry.isDirectory) {
    await toggleDirectory(skill.value.id, entry);
    return;
  }
  if (dirty.value) {
    if (entry.path === selectedPath.value) return;
    if (!window.confirm(t("common.discardUnsavedConfirm"))) return;
  }
  const token = ++openToken;
  fileError.value = "";
  try {
    const content = await backend.readSkillFile(skill.value.id, entry.path);
    if (token !== openToken) return;
    selectedPath.value = entry.path;
    openFile.value = content;
    draft.value = content.content;
    savedText.value = content.content;
  } catch (cause) {
    if (token === openToken) fileError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function saveFile() {
  if (!skill.value || !openFile.value || !dirty.value || saving.value || openFile.value.readOnly) return;
  saving.value = true;
  fileError.value = "";
  const text = draft.value;
  try {
    await backend.writeSkillFile(skill.value.id, openFile.value.path, text);
    savedText.value = text;
    if (openFile.value.path === "SKILL.md") await store.refreshResources();
  } catch (cause) {
    fileError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    saving.value = false;
  }
}

function onKeydown(event: KeyboardEvent) {
  if (pane.value !== "files") return;
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    void saveFile();
  }
}

// 按 id 监听：保存 SKILL.md 后的资源刷新会换出新对象，不能因此关掉编辑器丢掉草稿
watch(() => skill.value?.id, () => {
  openToken += 1;
  roots.value = [];
  children.value = {};
  expanded.value = new Set();
  selectedPath.value = "";
  openFile.value = null;
  void loadCount();
}, { immediate: true });

watch(pane, (value) => {
  if (value === "files" && !roots.value.length) void loadRoot();
});

// Monaco 主题跟随 <html data-theme>，避免浅色主题下仍是深色编辑器
const editorTheme = ref(monacoThemeName(currentDocumentTheme()));
let stopThemeWatch: (() => void) | undefined;
onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  stopThemeWatch = watchDocumentTheme((theme) => {
    editorTheme.value = monacoThemeName(theme);
  });
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown);
  stopThemeWatch?.();
});

const editorPath = shallowRef("");
watch(openFile, (file) => {
  editorPath.value = file ? `${resourceId.value}/${file.path}` : "";
});

async function copyPath() {
  if (!skill.value) return;
  try {
    await navigator.clipboard.writeText(skill.value.sourcePath);
  } catch {
    window.prompt(t("common.path"), skill.value.sourcePath);
  }
}

async function reveal() {
  if (!skill.value) return;
  try {
    await backend.revealResource(skill.value.id);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

async function removeSkill() {
  if (!skill.value) return;
  if (!window.confirm(t("skills.deleteConfirm", { name: skill.value.name }))) return;
  try {
    await store.deleteSkill(skill.value.id);
    await router.push("/skills");
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}
</script>

<template>
  <div v-if="skill" class="agent-detail-page skill-detail-page">
    <header class="agent-detail-header">
      <div class="grow">
        <h1>{{ skill.name }}</h1>
        <p>{{ skill.description || t("skills.detail.noDescription") }}</p>
        <small class="skill-meta">
          <span v-if="fileCount !== null">{{ t("skills.detail.fileCount", { n: fileCount }) }}</span>
          <span>{{ t("skills.detail.agentCount", { n: users.length }) }}</span>
          <span v-if="skill.updatedAt">{{ t("skills.detail.updated", { time: relativeTime(skill.updatedAt) }) }}</span>
          <span>{{ sourceName }}</span>
        </small>
      </div>
      <div class="header-actions">
        <UiButton @click="copyPath">{{ t("common.copyPath") }}</UiButton>
        <UiButton @click="reveal">{{ t("skills.detail.reveal") }}</UiButton>
        <UiButton class="mcp-delete" @click="removeSkill">{{ t("common.delete") }}</UiButton>
      </div>
    </header>

    <nav class="ds-tabs agent-detail-tabs" role="tablist" :aria-label="t('skills.detail.tabsAria')">
      <button type="button" role="tab" :aria-selected="pane === 'overview'" :class="{ active: pane === 'overview' }" @click="pane = 'overview'"><LayoutDashboard :size="14" /> {{ t("skills.detail.tab.overview") }}</button>
      <button type="button" role="tab" :aria-selected="pane === 'files'" :class="{ active: pane === 'files' }" @click="pane = 'files'"><FileText :size="14" /> {{ t("skills.detail.tab.files") }}<em v-if="fileCount !== null">{{ fileCount }}</em></button>
    </nav>

    <main v-if="pane === 'overview'" class="agent-detail-body skill-overview">
      <section class="agent-settings-card">
        <h2>{{ t("skills.detail.props") }}</h2>
        <p>{{ t("skills.detail.propsHint") }}</p>
        <dl class="squad-detail-list">
          <div><dt>{{ t("common.name") }}</dt><dd>{{ skill.name }}</dd></div>
          <div class="is-multiline"><dt>{{ t("skills.detail.field.description") }}</dt><dd>{{ skill.description || t("common.noDescription") }}</dd></div>
          <div><dt>{{ t("common.source") }}</dt><dd>{{ sourceName }}</dd></div>
          <div><dt>{{ t("common.path") }}</dt><dd :title="skill.sourcePath">{{ skill.sourcePath }}</dd></div>
        </dl>
      </section>
      <section class="agent-settings-card">
        <h2>{{ t("skills.detail.usedBy", { n: users.length }) }}</h2>
        <p v-if="!users.length">{{ t("skills.detail.unusedHint") }}</p>
        <button v-for="agent in users" :key="agent.id" type="button" class="skill-user-row" @click="router.push(`/agents/${agent.id}`)">
          <AgentAvatar :value="agent.avatar" :name="agent.name" :size="28" />
          <span>{{ agent.name }}</span>
        </button>
      </section>
    </main>

    <main v-else class="skill-files">
      <div class="file-tree skill-file-tree">
        <p v-if="treeError" class="form-error">{{ treeError }}</p>
        <ExplorerTreeList :nodes="treeNodes">
          <template #row="{ entry }">
            <div
              class="tree-entry"
              role="button"
              tabindex="0"
              :class="{ selected: selectedPath === entry.path }"
              :style="{ paddingLeft: `${treeRowPad(entry.depth)}px`, '--tree-depth': entry.depth }"
              @click="openEntry(entry)"
              @keydown.enter.prevent="openEntry(entry)"
            >
              <span
                v-for="level in entry.depth"
                :key="`${entry.path}:${level}`"
                class="tree-indent-guide"
                :class="{ innermost: level === entry.depth }"
                :style="{ left: `${treeGuideLeft(level)}px` }"
              />
              <Codicon
                v-if="entry.isDirectory"
                name="chevron-down"
                class="tree-chevron"
                :class="{ 'is-open': expanded.has(entry.path) }"
                :size="10"
              />
              <span v-else class="tree-chevron" />
              <FileTypeIcon :name="entry.name" :directory="entry.isDirectory" :open="expanded.has(entry.path)" />
              <span class="tree-name">{{ entry.name }}</span>
            </div>
          </template>
          <template #edit />
        </ExplorerTreeList>
      </div>
      <section class="skill-editor">
        <header v-if="openFile">
          <strong>{{ openFile.path }}</strong>
          <UiButton variant="primary" size="small" :class="{ 'is-loading': saving }" :disabled="!dirty || saving || openFile.readOnly" @click="saveFile">{{ saving ? t("common.saving") : t("common.save") }}</UiButton>
        </header>
        <p v-if="fileError" class="form-error">{{ fileError }}</p>
        <VueMonacoEditor
          v-if="openFile"
          class="monaco-host"
          :value="draft"
          :language="monacoLanguageId(openFile.language || 'plaintext')"
          :path="editorPath"
          :theme="editorTheme"
          :options="{
            scrollBeyondLastLine: false,
            lineNumbers: 'on',
            lineNumbersMinChars: 3,
            lineDecorationsWidth: 10,
            glyphMargin: false,
            readOnly: openFile.readOnly,
            minimap: { enabled: false },
            automaticLayout: true,
            tabSize: 2,
            insertSpaces: true,
            stickyScroll: { enabled: true },
            ...monacoZoomOptions,
          }"
          width="100%"
          height="100%"
          @change="(value: string | undefined) => { draft = value ?? ''; }"
        />
        <div v-else class="ds-empty skill-editor-empty">
          <span class="ds-empty-icon"><FileText :size="18" /></span>
          <h3>{{ t("skills.detail.noFileOpen") }}</h3>
          <p>{{ t("skills.detail.pickFile") }}</p>
        </div>
      </section>
    </main>
  </div>

  <section v-else class="page panel placeholder">
    <span class="ds-empty-icon"><Sparkles :size="18" /></span>
    <h2>{{ t("skills.detail.notFound") }}</h2>
    <p>{{ t("skills.detail.notFoundHint") }}</p>
    <UiButton @click="router.push('/skills')">{{ t("skills.detail.back") }}</UiButton>
  </section>
</template>
