<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Download, FolderTree, GitBranch, Info, MessageSquare, Mic, Shield, SlidersHorizontal, Workflow } from "@lucide/vue";
import {
  DEFAULT_EXPLORER_SETTINGS,
  filesExcludeFromText,
  filesExcludeToText,
  parseExplorerSettings,
  serializeExplorerSettings,
  type ExplorerSettings,
} from "../components/workbench/explorer/explorer-settings";
import { appVersion, checkGitExecutable } from "../services/backend";
import { setUpdateProxy } from "../services/update";
import { useAppStore } from "../stores/app";
import { useDictationStore } from "../stores/dictation";
import { useUpdaterStore } from "../stores/updater";
import {
  defaultBranchPrompt,
  defaultCommitPrompt,
  displayPrompt,
  GEN_MODE_AGENT,
  normalizeGenMode,
  persistPrompt,
} from "../domain/ai-helper-defaults";
import { DEFAULT_FILE_ICON_THEME, FILE_ICON_THEMES, normalizeFileIconTheme, setFileIconTheme } from "../file-icons";
import { applyLanguageSetting, locale, t } from "../i18n";
import { applyThemeSetting } from "../theme";
import SideNav, { type SideNavGroup, type SideNavItem } from "../components/SideNav.vue";
import { SETTINGS_NAV_WIDTH_KEY } from "../components/side-nav";
import SegmentedTabs from "../components/ui/SegmentedTabs.vue";
import SettingsAiGenBlock from "./SettingsAiGenBlock.vue";
import { integerSettingEntry } from "./settings-number";

const SAVE_DEBOUNCE_MS = 300;
const RUN_TIMEOUT_MIN_MINUTES = 0;
const RUN_TIMEOUT_MAX_MINUTES = 1440;
const WEBHOOK_PORT_MIN = 1024;
const WEBHOOK_PORT_MAX = 65535;
const defaultBranch = computed(() => defaultBranchPrompt());
const defaultCommit = computed(() => defaultCommitPrompt());

const SECTION_IDS = ["general", "dictation", "explorer", "git", "branch", "commit", "automation", "updates", "about"] as const;
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
  { id: "dictation", icon: Mic, groupKey: null, labelKey: "settings.section.dictation", titleKey: "settings.section.dictation", descKey: "settings.section.dictationDesc" },
  { id: "explorer", icon: FolderTree, groupKey: null, labelKey: "settings.section.explorer", titleKey: "settings.section.explorer", descKey: "settings.section.explorerDesc" },
  { id: "git", icon: GitBranch, groupKey: "settings.group.git", labelKey: "settings.section.git", titleKey: "settings.section.gitTitle", descKey: "settings.section.gitDesc" },
  { id: "branch", icon: GitBranch, groupKey: "settings.group.git", labelKey: "settings.section.branch", titleKey: "settings.section.branch", descKey: "settings.section.branchDesc" },
  { id: "commit", icon: MessageSquare, groupKey: "settings.group.git", labelKey: "settings.section.commit", titleKey: "settings.section.commitTitle", descKey: "settings.section.commitDesc" },
  { id: "automation", icon: Workflow, groupKey: null, labelKey: "settings.section.automation", titleKey: "settings.section.automationTitle", descKey: "settings.section.automationDesc" },
  { id: "updates", icon: Shield, groupKey: null, labelKey: "settings.section.updates", titleKey: "settings.section.updates", descKey: "settings.section.updatesDesc" },
  { id: "about", icon: Info, groupKey: null, labelKey: "settings.section.about", titleKey: "settings.section.aboutTitle", descKey: "settings.section.aboutDesc" },
];
const dictationStore = useDictationStore();
const dictationReasonKey = computed(() => {
  const reason = dictationStore.unsupportedReason;
  return reason === "devBuild" || reason === "noBundle" || reason === "missingUsageDescription"
    ? (`settings.dictation.reason.${reason}` as const)
    : null;
});

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
  branchPrefix: "feat",
  autoUpdate: true,
  updateProxy: "",
  formatOnSave: false,
  stickyScroll: true,
  wordWrap: false,
  runTimeoutMinutes: 0,
  permissionMode: "bypass",
  dictationEnabled: true,
  dictationLanguage: "auto",
  webhookPort: 43127,
  branchGenMode: GEN_MODE_AGENT,
  branchAgentId: "",
  branchPrompt: defaultBranchPrompt(),
  commitGenMode: GEN_MODE_AGENT,
  commitAgentId: "",
  commitPrompt: defaultCommitPrompt(),
  genApiUrl: "",
  genApiModel: "",
  genApiKey: "",
});
const explorerForm = reactive<ExplorerSettings>(structuredClone(DEFAULT_EXPLORER_SETTINGS));
const filesExcludeText = ref(filesExcludeToText(DEFAULT_EXPLORER_SETTINGS.filesExclude));
const watcherExcludeText = ref(filesExcludeToText(DEFAULT_EXPLORER_SETTINGS.watcherExclude));
const message = ref("");
const version = ref("");
const gitTestMessage = ref("");
const gitTesting = ref(false);
let hydrating = true;
let saveTimer = 0;
let lastSentSettings: Record<string, string> | null = null;

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

const themeTabs = computed(() => [
  { id: "system", label: t("settings.lang.system") },
  { id: "dark", label: t("settings.theme.dark") },
  { id: "light", label: t("settings.theme.light") },
]);

const fileIconTabs = computed(() =>
  FILE_ICON_THEMES.map((item) => ({ id: item.value, label: item.label })),
);

const permissionTabs = computed(() => [
  { id: "bypass", label: t("settings.permission.bypass") },
  { id: "prompt", label: t("settings.permission.prompt") },
]);

watch(
  () => store.data?.settings,
  (settings) => {
    if (!settings) return;
    // 自己保存后的回显：表单已是最新（可能还在继续输入），不要用裁剪后的值回填
    if (lastSentSettings && Object.entries(lastSentSettings).every(([key, value]) => settings[key] === value)) return;
    hydrating = true;
    form.language = settings.language ?? "system";
    form.theme = settings.theme ?? "dark";
    form.fileIconTheme = normalizeFileIconTheme(settings.fileIconTheme);
    form.gitExecutable = settings.gitExecutable ?? "git";
    form.isolationRoot = settings.isolationRoot ?? "~/.multara";
    form.branchPrefix = settings.branchPrefix ?? "feat";
    form.autoUpdate = settings.autoUpdate !== "false";
    gitTestMessage.value = "";
    form.updateProxy = settings.updateProxy ?? "";
    setUpdateProxy(form.updateProxy);
    form.formatOnSave = settings.formatOnSave === "true";
    form.stickyScroll = settings.stickyScroll !== "false";
    form.wordWrap = settings.wordWrap === "true";
    form.runTimeoutMinutes = Number(settings.runTimeoutMinutes ?? 0);
    form.permissionMode = settings.permissionMode === "prompt" || settings.permissionMode === "ask" ? "prompt" : "bypass";
    form.dictationEnabled = settings.dictationEnabled !== "false";
    form.dictationLanguage = settings.dictationLanguage ?? "auto";
    form.webhookPort = Number(settings.webhookPort ?? 43127);
    form.branchGenMode = normalizeGenMode(settings.branchGenMode);
    form.branchAgentId = settings.branchAgentId ?? "";
    form.branchPrompt = displayPrompt(settings.branchPrompt, defaultBranch.value);
    form.commitGenMode = normalizeGenMode(settings.commitGenMode);
    form.commitAgentId = settings.commitAgentId ?? "";
    form.commitPrompt = displayPrompt(settings.commitPrompt, defaultCommit.value);
    form.genApiUrl = settings.genApiUrl ?? "";
    form.genApiModel = settings.genApiModel ?? "";
    form.genApiKey = settings.genApiKey ?? "";
    Object.assign(explorerForm, parseExplorerSettings(settings.explorerSettings));
    filesExcludeText.value = filesExcludeToText(explorerForm.filesExclude);
    watcherExcludeText.value = filesExcludeToText(explorerForm.watcherExclude);
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
    form.branchPrefix,
    form.autoUpdate,
    form.updateProxy,
    form.formatOnSave,
    form.stickyScroll,
    form.wordWrap,
    form.runTimeoutMinutes,
    form.permissionMode,
    form.dictationEnabled,
    form.dictationLanguage,
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
    watcherExcludeText.value,
    JSON.stringify(explorerForm),
  ],
  () => scheduleSave(),
);

// 语言切换后：未自定义的默认提示词跟界面语言走
watch(locale, () => {
  const settings = store.data?.settings;
  if (!settings) return;
  if (!settings.branchPrompt?.trim()) form.branchPrompt = defaultBranch.value;
  if (!settings.commitPrompt?.trim()) form.commitPrompt = defaultCommit.value;
});

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
    gitExecutable: form.gitExecutable.trim() || "git",
    isolationRoot: form.isolationRoot,
    branchPrefix: form.branchPrefix.trim() || "feat",
    autoUpdate: String(form.autoUpdate),
    updateProxy: form.updateProxy.trim(),
    formatOnSave: String(form.formatOnSave),
    stickyScroll: String(form.stickyScroll),
    wordWrap: String(form.wordWrap),
    ...integerSettingEntry("runTimeoutMinutes", form.runTimeoutMinutes, RUN_TIMEOUT_MIN_MINUTES, RUN_TIMEOUT_MAX_MINUTES),
    permissionMode: form.permissionMode,
    dictationEnabled: String(form.dictationEnabled),
    dictationLanguage: form.dictationLanguage,
    ...integerSettingEntry("webhookPort", form.webhookPort, WEBHOOK_PORT_MIN, WEBHOOK_PORT_MAX),
    branchGenMode: normalizeGenMode(form.branchGenMode),
    branchAgentId: form.branchAgentId.trim(),
    branchPrompt: persistPrompt(form.branchPrompt, defaultBranch.value),
    commitGenMode: normalizeGenMode(form.commitGenMode),
    commitAgentId: form.commitAgentId.trim(),
    commitPrompt: persistPrompt(form.commitPrompt, defaultCommit.value),
    genApiUrl: form.genApiUrl.trim(),
    genApiModel: form.genApiModel.trim(),
    genApiKey: form.genApiKey.trim(),
    explorerSettings: serializeExplorerSettings({
      ...explorerForm,
      filesExclude: filesExcludeFromText(filesExcludeText.value),
      watcherExclude: filesExcludeFromText(watcherExcludeText.value),
    }),
  };
}

async function persist() {
  message.value = "";
  saveTimer = 0;
  try {
    lastSentSettings = settingsPayload();
    await store.saveSettings(lastSentSettings);
    applyTheme();
  } catch (cause) {
    message.value = t("settings.saveError", {
      error: cause instanceof Error ? cause.message : String(cause),
    });
  }
}

async function testGitExecutable() {
  gitTesting.value = true;
  gitTestMessage.value = "";
  try {
    const versionText = await checkGitExecutable(form.gitExecutable.trim() || "git");
    gitTestMessage.value = versionText;
  } catch (cause) {
    gitTestMessage.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    gitTesting.value = false;
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
  applyThemeSetting(form.theme);
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
  if (!saveTimer) return;
  window.clearTimeout(saveTimer);
  void persist();
});
</script>

<template>
  <div class="settings-shell">
    <SideNav
      class="settings-nav"
      :aria-label="t('settings.navAria')"
      :groups="sideGroups"
      :width-key="SETTINGS_NAV_WIDTH_KEY"
      @select="onSideSelect"
    />

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
            <SegmentedTabs
              v-model="form.theme"
              :items="themeTabs"
              :label="t('settings.theme')"
            />
          </div>
          <div class="settings-row">
            <div><strong>{{ t("settings.fileIconTheme") }}</strong><p>{{ t("settings.fileIconThemeHint") }}</p></div>
            <SegmentedTabs
              v-model="form.fileIconTheme"
              :items="fileIconTabs"
              :label="t('settings.fileIconTheme')"
            />
          </div>
          <div class="settings-row">
            <div><strong>{{ t("settings.runTimeout") }}</strong><p>{{ t("settings.runTimeoutHint") }}</p></div>
            <UiInput v-model.number="form.runTimeoutMinutes" type="number" :min="RUN_TIMEOUT_MIN_MINUTES" :max="RUN_TIMEOUT_MAX_MINUTES" />
          </div>
          <div class="settings-row">
            <div><strong>{{ t("settings.permission") }}</strong><p>{{ t("settings.permissionHint") }}</p></div>
            <SegmentedTabs
              v-model="form.permissionMode"
              :items="permissionTabs"
              :label="t('settings.permission')"
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
          <label class="settings-row">
            <div><strong>{{ t("settings.wordWrap") }}</strong><p>{{ t("settings.wordWrapHint") }}</p></div>
            <UiInput v-model="form.wordWrap" class="ds-switch" type="checkbox" />
          </label>
        </section>

        <section v-else-if="section === 'dictation'" class="settings-card">
          <p v-if="!dictationStore.supported" class="settings-hint">
            {{ dictationReasonKey ? t(dictationReasonKey) : t("settings.dictation.unsupported") }}
          </p>
          <template v-else>
            <label class="settings-row">
              <div><strong>{{ t("settings.dictation.enabled") }}</strong></div>
              <UiInput v-model="form.dictationEnabled" class="ds-switch" type="checkbox" />
            </label>
            <div class="settings-row">
              <div><strong>{{ t("settings.dictation.language") }}</strong></div>
              <UiSelect
                v-model="form.dictationLanguage"
                :options="[
                  { value: 'auto', label: t('settings.dictation.lang.auto') },
                  { value: 'zh-CN', label: t('settings.lang.zhCN') },
                  { value: 'zh-TW', label: t('settings.lang.zhTW') },
                  { value: 'en-US', label: t('settings.dictation.lang.enUS') },
                  { value: 'ja-JP', label: t('settings.lang.ja') },
                  { value: 'ko-KR', label: t('settings.lang.ko') },
                ]"
              />
            </div>
            <p class="settings-hint">{{ t("dictation.privacyNote") }}</p>
          </template>
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
          <div class="settings-row settings-row-stack">
            <div>
              <strong>{{ t("settings.explorer.watcherExclude") }}</strong>
              <p>{{ t("settings.explorer.watcherExcludeHint") }}</p>
            </div>
            <UiTextarea
              v-model="watcherExcludeText"
              rows="7"
              placeholder=".git/objects/**&#10;*/.git/objects/**"
            />
          </div>
        </section>

        <section v-else-if="section === 'git'" class="settings-card">
          <div class="settings-row settings-row-start">
            <div><strong>{{ t("settings.git.executable") }}</strong><p>{{ t("settings.git.executableHint") }}</p></div>
            <div class="settings-inline">
              <UiInput
                v-model="form.gitExecutable"
                :placeholder="t('settings.git.executablePlaceholder')"
                spellcheck="false"
              />
              <UiButton type="button" :disabled="gitTesting" @click="testGitExecutable">
                {{ gitTesting ? t("settings.git.testing") : t("settings.git.test") }}
              </UiButton>
            </div>
          </div>
          <p v-if="gitTestMessage" class="settings-hint" role="status">{{ gitTestMessage }}</p>
          <div class="settings-row settings-row-start">
            <div><strong>{{ t("settings.git.isolationRoot") }}</strong><p>{{ t("settings.git.isolationRootHint") }}</p></div>
            <UiInput v-model="form.isolationRoot" placeholder="~/.multara" />
          </div>
        </section>

        <section v-else-if="section === 'branch'" class="settings-card settings-ai-gen">
          <div class="settings-row">
            <div><strong>{{ t("settings.branch.prefix") }}</strong><p>{{ t("settings.branch.prefixHint") }}</p></div>
            <UiInput v-model="form.branchPrefix" :placeholder="t('settings.branch.prefixPlaceholder')" spellcheck="false" />
          </div>
          <SettingsAiGenBlock
            v-model:mode="form.branchGenMode"
            v-model:agent-id="form.branchAgentId"
            v-model:api-url="form.genApiUrl"
            v-model:api-model="form.genApiModel"
            v-model:api-key="form.genApiKey"
            v-model:prompt="form.branchPrompt"
            :agent-options="agentOptions"
            :default-prompt="defaultBranch"
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
            :default-prompt="defaultCommit"
          />
        </section>

        <section v-else-if="section === 'automation'" class="settings-card">
          <div class="settings-row settings-row-start">
            <div>
              <strong>{{ t("settings.automation.port") }}</strong>
              <p>{{ t("settings.automation.portHint") }}</p>
            </div>
            <UiInput v-model.number="form.webhookPort" type="number" :min="WEBHOOK_PORT_MIN" :max="WEBHOOK_PORT_MAX" />
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
