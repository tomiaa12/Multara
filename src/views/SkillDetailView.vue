<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { FileText, LayoutDashboard, Sparkles } from "@lucide/vue";
import { useRoute, useRouter } from "vue-router";
import type { FileContent } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import EditorWithDiff from "../components/workbench/EditorWithDiff.vue";
import WorkspaceFileTree from "../components/workbench/WorkspaceFileTree.vue";
import { currentDocumentTheme, monacoThemeName, watchDocumentTheme } from "../editor/monaco-workspace";
import { confirmDialog } from "../components/ui/confirm-dialog";
import { t } from "../i18n";
import { relativeTime } from "./task-board";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";

type Pane = "overview" | "files";

const route = useRoute();
const router = useRouter();
const store = useAppStore();
const pane = ref<Pane>("overview");
const fileCount = ref<number | null>(null);
const selectedPath = ref("");
const openFile = ref<FileContent | null>(null);
const draft = ref("");
const savedText = ref("");
const fileError = ref("");
const saving = ref(false);
let openToken = 0;

const resourceId = computed(() => String(route.params.resourceId ?? ""));
const skill = computed(() => store.data?.resources.find((item) => item.id === resourceId.value && item.kind === "skill") ?? null);
const users = computed(() => store.data?.agents.filter((agent) => agent.skillIds.includes(resourceId.value)) ?? []);
const dirty = computed(() => openFile.value !== null && draft.value !== savedText.value);
const stickyScroll = computed(() => store.data?.settings.stickyScroll !== "false");
const wordWrap = computed(() => store.data?.settings.wordWrap === "true");
const sourceName = computed(() => {
  const owner = skill.value?.owner;
  if (!owner || owner === "multara") return "Multara";
  return store.data?.runtimes.find((runtime) => runtime.id === owner)?.name ?? owner;
});

function listSkillDirectory(path: string) {
  const id = skill.value?.id;
  if (!id) return Promise.resolve([]);
  return backend.listSkillDirectory(id, path || undefined);
}

async function loadCount() {
  if (!skill.value) return;
  try {
    fileCount.value = await backend.skillFileCount(skill.value.id);
  } catch {
    fileCount.value = null;
  }
}

async function openSkillPath(path: string) {
  if (!skill.value) return;
  if (dirty.value) {
    if (path === selectedPath.value) return;
    if (!(await confirmDialog(t("common.discardUnsavedConfirm"), { type: "warning" }))) return;
  }
  const token = ++openToken;
  fileError.value = "";
  try {
    const content = await backend.readSkillFile(skill.value.id, path);
    if (token !== openToken) return;
    selectedPath.value = path;
    openFile.value = content;
    draft.value = content.content;
    savedText.value = content.content;
  } catch (cause) {
    if (token === openToken) fileError.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function openDefaultFile() {
  if (!skill.value || selectedPath.value) return;
  try {
    const entries = await backend.listSkillDirectory(skill.value.id);
    const skillFile = entries?.find((entry) => entry.name === "SKILL.md" && !entry.isDirectory);
    if (skillFile) await openSkillPath(skillFile.path);
  } catch (cause) {
    fileError.value = cause instanceof Error ? cause.message : String(cause);
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
  selectedPath.value = "";
  openFile.value = null;
  draft.value = "";
  savedText.value = "";
  void loadCount();
  if (pane.value === "files") void openDefaultFile();
}, { immediate: true });

watch(pane, (value) => {
  if (value === "files") void openDefaultFile();
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

function onEditorChange(value: string, fromUser: boolean) {
  if (!fromUser) return;
  draft.value = value;
}

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
  if (!(await confirmDialog(t("skills.deleteConfirm", { name: skill.value.name }), { danger: true }))) return;
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
          <div>
            <dt>{{ t("common.path") }}</dt>
            <UiTooltip :content="skill.sourcePath">
              <dd>{{ skill.sourcePath }}</dd>
            </UiTooltip>
          </div>
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
      <WorkspaceFileTree
        class="skill-file-tree"
        :state-key="`skill.${resourceId}`"
        :selected-path="selectedPath"
        :list-directory="listSkillDirectory"
        @open="openSkillPath"
      />
      <section class="skill-editor">
        <header v-if="openFile">
          <strong>{{ openFile.path }}</strong>
          <UiButton variant="primary" size="small" :class="{ 'is-loading': saving }" :disabled="!dirty || saving || openFile.readOnly" @click="saveFile">{{ saving ? t("common.saving") : t("common.save") }}</UiButton>
        </header>
        <p v-if="fileError" class="form-error">{{ fileError }}</p>
        <div v-if="openFile" class="editor-main">
          <EditorWithDiff
            :file-path="openFile.path"
            :workspace-id="resourceId"
            :content="draft"
            :language="openFile.language || 'plaintext'"
            :read-only="Boolean(openFile.readOnly || saving)"
            :theme="editorTheme"
            :sticky-scroll="stickyScroll"
            :word-wrap="wordWrap"
            :git="false"
            @change="onEditorChange"
          />
        </div>
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
