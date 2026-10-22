<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Download, FolderTree, GitBranch, Info, MessageSquare, Shield, SlidersHorizontal, Workflow } from "@lucide/vue";
import {
  DEFAULT_EXPLORER_SETTINGS,
  filesExcludeFromText,
  filesExcludeToText,
  parseExplorerSettings,
  serializeExplorerSettings,
  type ExplorerSettings,
} from "../components/workbench/explorer/explorer-settings";
import { appVersion } from "../services/backend";
import { setUpdateProxy } from "../services/update";
import { useAppStore } from "../stores/app";
import { useUpdaterStore } from "../stores/updater";
import {
  DEFAULT_BRANCH_PROMPT,
  DEFAULT_COMMIT_PROMPT,
  displayPrompt,
  GEN_MODE_AGENT,
  normalizeGenMode,
  persistPrompt,
} from "../domain/ai-helper-defaults";
import { DEFAULT_FILE_ICON_THEME, FILE_ICON_THEMES, normalizeFileIconTheme, setFileIconTheme } from "../file-icons";
import { applyLanguageSetting, locale, t } from "../i18n";
import SideNav, { type SideNavGroup, type SideNavItem } from "../components/SideNav.vue";
import SettingsAiGenBlock from "./SettingsAiGenBlock.vue";

const SAVE_DEBOUNCE_MS = 300;

const SECTION_IDS = ["general", "explorer", "git", "branch", "commit", "automation", "updates", "about"] as const;
type SectionId = (typeof SECTION_IDS)[number];

const SECTION_META: Array<{
  id: SectionId;
  icon: typeof SlidersHorizontal;
  groupKey: string | null;
  labelKey: string;
  titleKey: string;
  descKey: string;
}> = [
  { id: "general", icon: SlidersHorizontal, groupKey: null, labelKey: "settings.section.general", titleKey: "settings.section.general", descKey: "settings.section.generalDesc" },
  { id: "explorer", icon: FolderTree, groupKey: null, labelKey: "settings.section.explorer", titleKey: "settings.section.explorer", descKey: "settings.section.explorerDesc" },
  { id: "git", icon: GitBranch, groupKey: "settings.group.git", labelKey: "settings.section.git", titleKey: "settings.section.gitTitle", descKey: "settings.section.gitDesc" },
  { id: "branch", icon: GitBranch, groupKey: "settings.group.git", labelKey: "settings.section.branch", titleKey: "settings.section.branch", descKey: "settings.section.branchDesc" },
  { id: "commit", icon: MessageSquare, groupKey: "settings.group.git", labelKey: "settings.section.commit", titleKey: "settings.section.commitTitle", descKey: "settings.section.commitDesc" },
  { id: "automation", icon: Workflow, groupKey: null, labelKey: "settings.section.automation", titleKey: "settings.section.automationTitle", descKey: "settings.section.automationDesc" },
  { id: "updates", icon: Shield, groupKey: null, labelKey: "settings.section.updates", titleKey: "settings.section.updates", descKey: "settings.section.updatesDesc" },
  { id: "about", icon: Info, groupKey: null, labelKey: "settings.section.about", titleKey: "settings.section.aboutTitle", descKey: "settings.section.aboutDesc" },
];

const store = useAppStore();
const updater = useUpdaterStore();
const route = useRoute();
const router = useRouter();
const form = reactive({
  language: "system",
  theme: "dark",
  fileIconTheme: DEFAULT_FILE_ICON_THEME as string,
  gitExecutable: "git",
  isolationRoot: "~/.multara",
  autoUpdate: true,
  updateProxy: "",
  formatOnSave: false,
  stickyScroll: true,
  runTimeoutMinutes: 30,
  permissionMode: "bypass",
  webhookPort: 43127,
  branchGenMode: GEN_MODE_AGENT,
  branchAgentId: "",
  branchPrompt: DEFAULT_BRANCH_PROMPT,
  commitGenMode: GEN_MODE_AGENT,
  commitAgentId: "",
  commitPrompt: DEFAULT_COMMIT_PROMPT,
  genApiUrl: "",
  genApiModel: "",
  genApiKey: "",
});
const explorerForm = reactive<ExplorerSettings>(structuredClone(DEFAULT_EXPLORER_SETTINGS));
const filesExcludeText = ref(filesExcludeToText(DEFAULT_EXPLORER_SETTINGS.filesExclude));
const message = ref("");
const version = ref("");
let hydrating = true;
let saveTimer = 0;

const section = computed<SectionId>(() => {
  const value = route.query.section;
  return SECTION_IDS.includes(value as SectionId) ? (value as SectionId) : "general";
});

const sections = computed(() =>
  SECTION_META.map((item) => ({
    ...item,
    label: t(item.labelKey),
    title: t(item.titleKey),
    description: t(item.descKey),
    group: item.groupKey ? t(item.groupKey) : null,
  })),
);

const current = computed(() => sections.value.find((item) => item.id === section.value) ?? sections.value[0]);

const sideGroups = computed<SideNavGroup[]>(() => {
  const groups: SideNavGroup[] = [];
  for (const item of sections.value) {
    const last = groups.at(-1);
    const navItem = {
      id: item.id,
      label: item.label,
      icon: item.icon,
      active: section.value === item.id,
    };
    const sameLabeled = Boolean(item.group && last?.label === item.group);
    const sameUnlabeled = Boolean(!item.group && last && !last.label);
    if (last && (sameLabeled || sameUnlabeled)) {
      last.items.push(navItem);
      continue;
    }
    groups.push({ label: item.group, items: [navItem] });
  }
  return groups;
});

/** 与聊天输入框同一套选项：智能体 + 小队 */
const agentOptions = computed(() => [
  ...(store.data?.agents ?? []).map((agent) => ({
    value: `agent:${agent.id}`,
    label: agent.name || agent.id,
    avatar: agent.avatar,
  })),
  ...(store.data?.squads ?? []).map((squad) => ({
    value: `squad:${squad.id}`,
    label: t("chat.squadLabel", { name: squad.name }),
    avatar: store.data?.agents.find((agent) => agent.id === squad.leaderAgentId)?.avatar ?? "◉",
  })),
]);

const languageOptions = computed(() => [
  { value: "system", label: t("settings.lang.system") },
  { value: "zh-CN", label: t("settings.lang.zhCN") },
  { value: "zh-TW", label: t("settings.lang.zhTW") },
  { value: "en", label: t("settings.lang.en") },
  { value: "ja", label: t("settings.lang.ja") },
  { value: "ko", label: t("settings.lang.ko") },
]);

watch(
  () => store.data?.settings,
  (settings) => {
    if (!settings) return;
    hydrating = true;
    form.language = settings.language ?? "system";
    form.theme = settings.theme ?? "dark";
    form.fileIconTheme = normalizeFileIconTheme(settings.fileIconTheme);
    form.gitExecutable = settings.gitExecutable ?? "git";
    form.isolationRoot = settings.isolationRoot ?? "~/.multara";
    form.autoUpdate = settings.autoUpdate !== "false";
    form.updateProxy = settings.updateProxy ?? "";
    setUpdateProxy(form.updateProxy);
    form.formatOnSave = settings.formatOnSave === "true";
    form.stickyScroll = settings.stickyScroll !== "false";
    form.runTimeoutMinutes = Number(settings.runTimeoutMinutes ?? 30);
    form.permissionMode = settings.permissionMode === "prompt" || settings.permissionMode === "ask" ? "prompt" : "bypass";
    form.webhookPort = Number(settings.webhookPort ?? 43127);
    form.branchGenMode = normalizeGenMode(settings.branchGenMode);
    form.branchAgentId = settings.branchAgentId ?? "";
    form.branchPrompt = displayPrompt(settings.branchPrompt, DEFAULT_BRANCH_PROMPT);
    form.commitGenMode = normalizeGenMode(settings.commitGenMode);
    form.commitAgentId = settings.commitAgentId ?? "";
    form.commitPrompt = displayPrompt(settings.commitPrompt, DEFAULT_COMMIT_PROMPT);
    form.genApiUrl = settings.genApiUrl ?? "";
    form.genApiModel = settings.genApiModel ?? "";
    form.genApiKey = settings.genApiKey ?? "";
    Object.assign(explorerForm, parseExplorerSettings(settings.explorerSettings));
    filesExcludeText.value = filesExcludeToText(explorerForm.filesExclude);
    applyTheme();
    applyLanguageSetting(form.language);
    void Promise.resolve().then(() => {
      hydrating = false;
    });
  },
  { immediate: true },
);

watch(() => form.language, (value) => {
  applyLanguageSetting(value);
  scheduleSave();
});
watch(() => form.theme, () => {
  applyTheme();
  scheduleSave();
});
watch(() => form.fileIconTheme, (value) => {
  setFileIconTheme(value);
  scheduleSave();
});
watch(
  () => [
    form.gitExecutable,
    form.isolationRoot,
    form.autoUpdate,
    form.updateProxy,
    form.formatOnSave,
    form.stickyScroll,
    form.runTimeoutMinutes,
    form.permissionMode,
    form.webhookPort,
    form.branchGenMode,
    form.branchAgentId,
    form.branchPrompt,
    form.commitGenMode,
    form.commitAgentId,
    form.commitPrompt,
    form.genApiUrl,
    form.genApiModel,
    form.genApiKey,
    filesExcludeText.value,
    JSON.stringify(explorerForm),
  ],
  () => scheduleSave(),
);

// Re-render section labels when locale changes
watch(locale, () => undefined);

function scheduleSave() {
  if (hydrating) return;
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    void persist();
  }, SAVE_DEBOUNCE_MS);
}

function settingsPayload() {
  return {
    language: form.language,
    theme: form.theme,
    fileIconTheme: form.fileIconTheme,
    gitExecutable: form.gitExecutable,
    isolationRoot: form.isolationRoot,
    autoUpdate: String(form.autoUpdate),
    updateProxy: form.updateProxy.trim(),
    formatOnSave: String(form.formatOnSave),
    stickyScroll: String(form.stickyScroll),
    runTimeoutMinutes: String(form.runTimeoutMinutes),
    permissionMode: form.permissionMode,
    webhookPort: String(form.webhookPort),
    branchGenMode: normalizeGenMode(form.branchGenMode),
    branchAgentId: form.branchAgentId.trim(),
    branchPrompt: persistPrompt(form.branchPrompt, DEFAULT_BRANCH_PROMPT),
    commitGenMode: normalizeGenMode(form.commitGenMode),
    commitAgentId: form.commitAgentId.trim(),
    commitPrompt: persistPrompt(form.commitPrompt, DEFAULT_COMMIT_PROMPT),
    genApiUrl: form.genApiUrl.trim(),
    genApiModel: form.genApiModel.trim(),
    genApiKey: form.genApiKey.trim(),
    explorerSettings: serializeExplorerSettings({
      ...explorerForm,
      filesExclude: filesExcludeFromText(filesExcludeText.value),
    }),
  };
}

async function persist() {
  message.value = "";
  try {
    await store.saveSettings(settingsPayload());
    applyTheme();
  } catch (cause) {
    message.value = t("settings.saveError", {
      error: cause instanceof Error ? cause.message : String(cause),
    });
  }
}

function onSideSelect(item: SideNavItem) {
  if (!SECTION_IDS.includes(item.id as SectionId)) return;
  openSection(item.id as SectionId);
}

function openSection(id: SectionId) {
  message.value = "";
  router.replace({ query: { section: id } });
}

function applyTheme() {
  const selected = form.theme === "system"
    ? (window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark")
    : form.theme;
  document.documentElement.dataset.theme = selected;
}

watch(
  () => form.autoUpdate,
  (enabled) => {
    if (hydrating) return;
    updater.setAutomaticUpdates(enabled);
  },
);

watch(
  () => form.updateProxy,
  (proxy) => {
    setUpdateProxy(proxy);
  },
);

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

watch(
  () => updater.downloadProgress,
  (progress) => {
    const version = updater.lastChecked?.version;
    if (!progress || !version) return;
    message.value = t("settings.updates.downloadProgress", {
      version,
      percent: String(progress.percent),
      downloaded: formatBytes(progress.downloaded),
      total: progress.total > 0 ? formatBytes(progress.total) : "…",
    });
  },
);

watch(
  () => updater.readyVersion,
  (next) => {
    if (!next) return;
    message.value = t("settings.updates.ready", { version: next });
  },
);

async function checkUpdate() {
  message.value = t("settings.updates.checking");
  try {
    const found = await updater.checkNow();
    if (!found) {
      message.value = t("settings.updates.latest");
      return;
    }
    message.value = found.ready
      ? t("settings.updates.ready", { version: found.version })
      : t("settings.updates.downloading", { version: found.version });
  } catch (cause) {
    message.value = t("settings.updates.checkFailed", {
      error: cause instanceof Error ? cause.message : String(cause),
    });
  }
}

async function applyUpdate() {
  message.value = t("settings.updates.installing");
  try {
    await updater.installNow();
  } catch (cause) {
    message.value = t("settings.updates.checkFailed", {
      error: cause instanceof Error ? cause.message : String(cause),
    });
  }
}

onMounted(async () => {
  version.value = await appVersion();
});

onBeforeUnmount(() => {
  window.clearTimeout(saveTimer);
});
</script>

<template>
  <div class="settings-shell">
    <SideNav class="settings-nav" :aria-label="t('settings.navAria')" :groups="sideGroups" @select="onSideSelect" />

    <div class="settings-body">
      <div class="settings-panel">
        <header class="settings-heading">
          <div>
            <h2>{{ current.title }}</h2>
            <p>{{ current.description }}</p>
          </div>
          <div v-if="message" class="settings-save" role="status">{{ message }}</div>
        </header>

        <section v-if="section === 'general'" class="settings-card">
          <div class="settings-row">
            <div><strong>{{ t("settings.language") }}</strong><p>{{ t("settings.languageHint") }}</p></div>
            <UiSelect v-model="form.language" :options="languageOptions" />
          </div>
          <div class="settings-row">
            <div><strong>{{ t("settings.theme") }}</strong><p>{{ t("settings.themeHint") }}</p></div>
            <UiSelect
              v-model="form.theme"
              :options="[
                { value: 'system', label: t('settings.lang.system') },
                { value: 'dark', label: t('settings.theme.dark') },
                { value: 'light', label: t('settings.theme.light') },
              ]"
            />
          </div>
          <div class="settings-row">
            <div><strong>{{ t("settings.fileIconTheme") }}</strong><p>{{ t("settings.fileIconThemeHint") }}</p></div>
            <UiSelect v-model="form.fileIconTheme" :options="[...FILE_ICON_THEMES]" />
          </div>
          <div class="settings-row">
            <div><strong>{{ t("settings.runTimeout") }}</strong><p>{{ t("settings.runTimeoutHint") }}</p></div>
            <UiInput v-model.number="form.runTimeoutMinutes" type="number" min="1" max="1440" />
          </div>
          <div class="settings-row">
            <div><strong>{{ t("settings.permission") }}</strong><p>{{ t("settings.permissionHint") }}</p></div>
            <UiSelect
              v-model="form.permissionMode"
              :options="[
                { value: 'bypass', label: t('settings.permission.bypass') },
                { value: 'prompt', label: t('settings.permission.prompt') },
              ]"
            />
          </div>
          <label class="settings-row">
            <div><strong>{{ t("settings.formatOnSave") }}</strong><p>{{ t("settings.formatOnSaveHint") }}</p></div>
            <UiInput v-model="form.formatOnSave" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.stickyScroll") }}</strong><p>{{ t("settings.stickyScrollHint") }}</p></div>
            <UiInput v-model="form.stickyScroll" class="ds-switch" type="checkbox" />
          </label>
        </section>

        <section v-else-if="section === 'explorer'" class="settings-card">
          <div class="settings-row">
            <div><strong>{{ t("settings.explorer.sort") }}</strong><p>{{ t("settings.explorer.sortHint") }}</p></div>
            <UiSelect
              v-model="explorerForm.sortOrder"
              :options="[
                { value: 'default', label: t('settings.explorer.sort.default') },
                { value: 'mixed', label: t('settings.explorer.sort.mixed') },
                { value: 'filesFirst', label: t('settings.explorer.sort.filesFirst') },
                { value: 'type', label: t('settings.explorer.sort.type') },
                { value: 'modified', label: t('settings.explorer.sort.modified') },
                { value: 'foldersNestsFiles', label: t('settings.explorer.sort.foldersNestsFiles') },
              ]"
            />
          </div>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.reverse") }}</strong><p>{{ t("settings.explorer.reverseHint") }}</p></div>
            <UiInput v-model="explorerForm.sortOrderReverse" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.compact") }}</strong><p>{{ t("settings.explorer.compactHint") }}</p></div>
            <UiInput v-model="explorerForm.compactFolders" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.nesting") }}</strong><p>{{ t("settings.explorer.nestingHint") }}</p></div>
            <UiInput v-model="explorerForm.fileNestingEnabled" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.dnd") }}</strong><p>{{ t("settings.explorer.dndHint") }}</p></div>
            <UiInput v-model="explorerForm.enableDragAndDrop" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.confirmDnd") }}</strong><p>{{ t("settings.explorer.confirmDndHint") }}</p></div>
            <UiInput v-model="explorerForm.confirmDragAndDrop" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.confirmDelete") }}</strong><p>{{ t("settings.explorer.confirmDeleteHint") }}</p></div>
            <UiInput v-model="explorerForm.confirmDelete" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.undo") }}</strong><p>{{ t("settings.explorer.undoHint") }}</p></div>
            <UiInput v-model="explorerForm.enableUndo" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.excludeGitIgnore") }}</strong><p>{{ t("settings.explorer.excludeGitIgnoreHint") }}</p></div>
            <UiInput v-model="explorerForm.excludeGitIgnore" class="ds-switch" type="checkbox" />
          </label>
          <label class="settings-row">
            <div><strong>{{ t("settings.explorer.autoReveal") }}</strong><p>{{ t("settings.explorer.autoRevealHint") }}</p></div>
            <UiInput
              :model-value="explorerForm.autoReveal !== false"
              class="ds-switch"
              type="checkbox"
              @update:model-value="explorerForm.autoReveal = $event ? true : false"
            />
          </label>
          <div class="settings-row">
            <div><strong>{{ t("settings.explorer.naming") }}</strong><p>{{ t("settings.explorer.namingHint") }}</p></div>
            <UiSelect
              v-model="explorerForm.incrementalNaming"
              :options="[
                { value: 'simple', label: t('settings.explorer.naming.simple') },
                { value: 'smart', label: t('settings.explorer.naming.smart') },
                { value: 'disabled', label: t('settings.explorer.naming.disabled') },
              ]"
            />
          </div>
          <div class="settings-row settings-row-stack">
            <div>
              <strong>{{ t("settings.explorer.exclude") }}</strong>
              <p>{{ t("settings.explorer.excludeHint") }}</p>
            </div>
            <UiTextarea
              v-model="filesExcludeText"
              rows="7"
              placeholder="**/.git&#10;**/node_modules"
            />
          </div>
        </section>

        <section v-else-if="section === 'git'" class="settings-card">
          <div class="settings-row settings-row-start">
            <div><strong>{{ t("settings.git.isolationRoot") }}</strong><p>{{ t("settings.git.isolationRootHint") }}</p></div>
            <UiInput v-model="form.isolationRoot" placeholder="~/.multara" />
          </div>
        </section>

        <section v-else-if="section === 'branch'" class="settings-card settings-ai-gen">
          <SettingsAiGenBlock
            v-model:mode="form.branchGenMode"
            v-model:agent-id="form.branchAgentId"
            v-model:api-url="form.genApiUrl"
            v-model:api-model="form.genApiModel"
            v-model:api-key="form.genApiKey"
            v-model:prompt="form.branchPrompt"
            :agent-options="agentOptions"
            :default-prompt="DEFAULT_BRANCH_PROMPT"
          />
        </section>

        <section v-else-if="section === 'commit'" class="settings-card settings-ai-gen">
          <SettingsAiGenBlock
            v-model:mode="form.commitGenMode"
            v-model:agent-id="form.commitAgentId"
            v-model:api-url="form.genApiUrl"
            v-model:api-model="form.genApiModel"
            v-model:api-key="form.genApiKey"
            v-model:prompt="form.commitPrompt"
            :agent-options="agentOptions"
            :default-prompt="DEFAULT_COMMIT_PROMPT"
          />
        </section>

        <section v-else-if="section === 'automation'" class="settings-card">
          <div class="settings-row settings-row-start">
            <div>
              <strong>{{ t("settings.automation.port") }}</strong>
              <p>{{ t("settings.automation.portHint") }}</p>
            </div>
            <UiInput v-model.number="form.webhookPort" type="number" min="1024" max="65535" />
          </div>
        </section>

        <section v-else-if="section === 'updates'" class="settings-card">
          <div class="settings-row">
            <div><strong>{{ t("settings.about.version") }}</strong><p>{{ message || t("settings.about.versionHint") }}</p></div>
            <span class="settings-version">{{ version || "…" }}</span>
          </div>
          <label class="settings-row">
            <div><strong>{{ t("settings.updates.auto") }}</strong><p>{{ t("settings.updates.autoHint") }}</p></div>
            <UiInput v-model="form.autoUpdate" class="ds-switch" type="checkbox" />
          </label>
          <div class="settings-row settings-row-stack">
            <div>
              <strong>{{ t("settings.updates.proxy") }}</strong>
              <p>{{ t("settings.updates.proxyHint") }}</p>
            </div>
            <UiInput
              v-model="form.updateProxy"
              :placeholder="t('settings.updates.proxyPlaceholder')"
              spellcheck="false"
            />
          </div>
          <div class="settings-row">
            <div><strong>{{ t("settings.updates.checkNow") }}</strong><p>{{ t("settings.updates.checkNowHint") }}</p></div>
            <div class="settings-update-actions">
              <UiButton @click="checkUpdate"><Download :size="15" />{{ t("settings.updates.checkNowBtn") }}</UiButton>
              <UiButton
                v-if="updater.lastChecked?.ready || updater.readyVersion"
                @click="applyUpdate"
              >{{ t("settings.updates.installNow") }}</UiButton>
            </div>
          </div>
        </section>

        <section v-else class="settings-card">
          <div class="settings-row">
            <div><strong>{{ t("settings.about.version") }}</strong><p>{{ message || t("settings.about.versionHint") }}</p></div>
            <span class="settings-version">{{ version || "…" }}</span>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>
