<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { Network, Pencil, Plus, RefreshCw, Search, SearchX, Store, X } from "@lucide/vue";
import type { CatalogItem, GlobalResource, McpInput, McpProbe, ResourceSource } from "../domain/types";
import CliSourceIcon from "../components/CliSourceIcon.vue";
import SideNav, { type SideNavGroup, type SideNavItem } from "../components/SideNav.vue";
import { SETTINGS_NAV_WIDTH_KEY } from "../components/side-nav";
import ContextMenu, { type ContextMenuItem } from "../components/ui/ContextMenu.vue";
import type { DirectoryColumn } from "../components/ui/directory-table";
import * as backend from "../services/backend";
import { cliMcpDisplay } from "../domain/cli-catalog";
import { t } from "../i18n";
import { mcpBrandFallbackColor, mcpBrandIconUrl, mcpBrandInitial } from "./mcp-brand-icon";
import { argsToText, detectedNeedsAuth, emptyMcpInput, jsonToMcpInputs, mcpInputToJson, parseArgsText } from "./mcp-json";
import { mcpProbeErrorKey } from "./mcp-probe-error";
import { mcpToolsCaption } from "./mcp-tools-caption";
import { highlightParts } from "./resource-text";
import { useAppStore } from "../stores/app";
import { useMcpToolsStore } from "../stores/mcp-tools";
import CatalogBrowse from "./CatalogBrowse.vue";
import { ALL_SOURCES, compareResourceSourceLabels } from "./resource-source-nav";

const store = useAppStore();
const mcpTools = useMcpToolsStore();
const browsing = ref(false);
const query = ref("");
const source = ref(ALL_SOURCES);
const editor = ref(false);
const busy = ref(false);
const error = ref("");
const form = reactive<McpInput>(emptyMcpInput());
const argsText = ref("");
const mode = ref<"ui" | "json">("ui");
const jsonText = ref("");
const authorizing = ref("");
const authorizeError = ref("");
const refreshing = ref(false);
const brokenIcons = ref(new Set<string>());
const toolsDialog = ref<{ item: GlobalResource; probe: McpProbe } | null>(null);
const probeDialog = ref<GlobalResource | null>(null);
const inputValues = reactive<Record<string, string>>({});
const inputIds = ref<string[]>([]);
const sources = ref<ResourceSource[]>([]);
const menu = ref<{ x: number; y: number; item: GlobalResource } | null>(null);
const snapshot = ref("");

const servers = computed(() => store.data?.resources.filter((item) => item.kind === "mcp") ?? []);
const visible = computed(() => {
  const text = query.value.trim().toLowerCase();
  const rows = servers.value.filter((item) => {
    if (source.value !== ALL_SOURCES && item.owner !== source.value) return false;
    if (!text) return true;
    return item.name.toLowerCase().includes(text)
      || item.summary.toLowerCase().includes(text)
      || item.sourceLabel.toLowerCase().includes(text);
  });
  return [...rows].sort((left, right) => {
    const bySource = compareResourceSourceLabels(
      { id: left.owner || "multara", label: left.sourceLabel || left.owner || "multara" },
      { id: right.owner || "multara", label: right.sourceLabel || right.owner || "multara" },
    );
    if (bySource !== 0) return bySource;
    return left.name.localeCompare(right.name, "zh");
  });
});
const sourceTabs = computed(() => {
  const counts = new Map<string, number>();
  for (const item of servers.value) counts.set(item.owner, (counts.get(item.owner) ?? 0) + 1);
  const items = [...counts.entries()]
    .map(([id, count]) => ({ id, label: servers.value.find((item) => item.owner === id)?.sourceLabel ?? id, count }))
    .sort(compareResourceSourceLabels);
  return [{ id: ALL_SOURCES, label: t("common.all"), count: servers.value.length }, ...items];
});
const ownerOptions = computed(() =>
  sources.value.map((item) => ({
    value: item.owner,
    label: item.label,
    description: item.scope === "workspace" ? t("common.workspaceScope") : undefined,
  })),
);

const directoryColumns = computed<DirectoryColumn[]>(() => [
  { key: "source", label: t("mcp.col.source"), size: "112px" },
  { key: "name", label: t("mcp.col.name"), size: "minmax(0, 1.2fr)" },
  { key: "type", label: t("mcp.col.type"), size: "80px" },
  { key: "status", label: t("mcp.col.status"), size: "96px" },
  { key: "enabled", label: t("mcp.col.enabled"), size: "48px" },
  { key: "actions", label: t("mcp.col.actions"), size: "minmax(320px, 1.1fr)", actions: true },
]);

watch(sourceTabs, (tabs) => {
  if (!tabs.some((tab) => tab.id === source.value)) source.value = ALL_SOURCES;
});

const sourceNavGroups = computed<SideNavGroup[]>(() => [{
  label: t("mcp.col.source"),
  items: sourceTabs.value.map((tab) => ({
    id: tab.id,
    label: tab.label,
    badge: tab.count,
    active: source.value === tab.id,
  })),
}]);

function onSourceSelect(item: SideNavItem) {
  source.value = item.id;
}

function statusLabel(item: GlobalResource) {
  const live = mcpTools.probes[item.id];
  if (live?.status === "running") return t("mcp.status.running");
  if (live?.status === "needs_auth" || item.configStatus === "needs_auth") return t("mcp.status.needsAuth");
  if (live?.status === "error") return t("mcp.status.error");
  if (item.configStatus === "needs_input") return t("mcp.status.needsInput");
  if (item.configStatus === "disabled") return t("mcp.status.disabled");
  if (item.configStatus === "incomplete") return t("mcp.status.incomplete");
  return t("mcp.status.ready");
}

/** Cursor 等来源的 HTTP MCP 常不写 oauth 字段；探测到 401/403 后也要能授权。 */
function canAuthorize(item: GlobalResource) {
  if (item.transport !== "http" && item.transport !== "sse") return false;
  if (item.needsAuth || item.configStatus === "needs_auth") return true;
  return mcpTools.probes[item.id]?.status === "needs_auth";
}

function syncJsonFromForm() {
  jsonText.value = mcpInputToJson({ ...form, needsAuth: detectedNeedsAuth(form) }, argsText.value);
}

function applyJsonToForm() {
  const [first] = jsonToMcpInputs(jsonText.value);
  Object.assign(form, { ...first, name: first.name || form.name, previousName: form.previousName, owner: form.owner });
  argsText.value = argsToText(first.args);
}

function setMode(next: "ui" | "json") {
  error.value = "";
  try {
    if (mode.value === "ui" && next === "json") syncJsonFromForm();
    if (mode.value === "json" && next === "ui") applyJsonToForm();
    mode.value = next;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
}

async function loadSources() {
  sources.value = await backend.listResourceSources("mcp");
}

async function openCreate() {
  await loadSources();
  Object.assign(form, emptyMcpInput(sources.value[0]?.owner ?? "multara"));
  argsText.value = "";
  mode.value = "ui";
  jsonText.value = "";
  error.value = "";
  inputIds.value = [];
  Object.keys(inputValues).forEach((key) => delete inputValues[key]);
  snapshot.value = "";
  editor.value = true;
}

async function openEdit(item: GlobalResource) {
  error.value = "";
  try {
    await loadSources();
    const current = await backend.getMcp(item.name, item.owner);
    Object.assign(form, { ...current, previousName: item.name, args: [...current.args] });
    argsText.value = argsToText(current.args);
    mode.value = "ui";
    jsonText.value = "";
    const inputs = await backend.getMcpInputs(item.name, item.owner);
    inputIds.value = inputs.ids;
    Object.keys(inputValues).forEach((key) => delete inputValues[key]);
    for (const [key, value] of Object.entries(inputs.values)) inputValues[key] = value;
    snapshot.value = JSON.stringify({ form: { ...form }, argsText: argsText.value });
    editor.value = true;
  } catch (cause) {
    alertError(cause);
  }
}

function discard() {
  if (!snapshot.value) {
    editor.value = false;
    return;
  }
  const saved = JSON.parse(snapshot.value) as { form: McpInput; argsText: string };
  Object.assign(form, saved.form);
  argsText.value = saved.argsText;
}

async function save() {
  busy.value = true;
  error.value = "";
  try {
    const parsed = mode.value === "json" ? jsonToMcpInputs(jsonText.value).map((item) => ({
      ...item,
      name: item.name.trim() || form.name.trim(),
      owner: form.owner,
    })) : [{
      ...form,
      needsAuth: detectedNeedsAuth(form),
      args: parseArgsText(argsText.value),
    }];
    for (const [index, item] of parsed.entries()) {
      await store.saveMcp({
        ...item,
        previousName: index === 0 ? form.previousName : "",
      });
    }
    if (form.previousName || form.name) {
      await backend.saveMcpInputs(form.name.trim() || form.previousName || "", { ...inputValues });
    }
    editor.value = false;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

async function remove(item: GlobalResource) {
  if (!window.confirm(t("mcp.deleteConfirm", { name: item.name }))) return;
  try {
    await store.deleteMcp(item.name, item.owner);
  } catch (cause) {
    alertError(cause);
  }
}

function transferOptions(item: GlobalResource) {
  // 与 Skills 一致：只迁到全局运行时配置，不出现工作区/项目路径
  return sources.value
    .filter((entry) => entry.scope !== "workspace")
    .filter((entry) => entry.owner !== (item.owner || "multara"))
    .map((entry) => ({
      value: entry.owner,
      label: entry.label,
      meta: cliMcpDisplay(entry.owner),
    }));
}

async function transferMcp(item: GlobalResource, targetOwner: unknown, copy: boolean) {
  if (targetOwner == null || targetOwner === "") return;
  try {
    await store.transferResource(item.id, String(targetOwner), copy);
  } catch (cause) {
    alertError(cause);
  }
}

function authorizeLabel(item: GlobalResource) {
  if (authorizing.value === item.id) return t("mcp.authorizing");
  if (item.configStatus === "ready") return t("mcp.reauthorize");
  return t("mcp.authorize");
}

function alertError(cause: unknown) {
  window.alert(friendlyError(cause instanceof Error ? cause.message : String(cause)));
}

function friendlyError(raw: string): string {
  const key = mcpProbeErrorKey(raw);
  return key ? t(key) : (raw.trim() || t("mcp.error.generic"));
}

/** 探测详情：优先 stderr/启动日志；没有时回退原始 error（友好文案之外仍要能看到原因）。 */
function probeDetailLog(probe: McpProbe): string {
  const log = probe.log?.trim() ?? "";
  const raw = probe.error?.trim() ?? "";
  if (!log) return raw;
  if (!raw || log.includes(raw)) return log;
  return `${log}\n${raw}`;
}

const activeProbe = computed(() => {
  const id = probeDialog.value?.id;
  return id ? mcpTools.probes[id] : undefined;
});
const activeProbeLog = computed(() => (activeProbe.value ? probeDetailLog(activeProbe.value) : ""));

function alertProbeFailure(probe: McpProbe) {
  if (!probe.error) return;
  window.alert(friendlyError(probe.error));
}

async function authorize(item: GlobalResource) {
  authorizing.value = item.id;
  authorizeError.value = "";
  try {
    await store.authorizeMcp(item.name, item.owner);
  } catch (cause) {
    // 授权失败文案以后端为准（如 Figma 拒绝动态注册），不要把 403 收成「需要授权」循环提示
    const raw = cause instanceof Error ? cause.message : String(cause);
    authorizeError.value = raw.trim() || t("mcp.error.generic");
  } finally {
    authorizing.value = "";
  }
}

async function toggle(item: GlobalResource, event: Event) {
  const enabled = (event.target as HTMLInputElement).checked;
  try {
    await store.setMcpEnabled(item.name, enabled, item.owner);
  } catch (cause) {
    alertError(cause);
  }
}

function openProbe(item: GlobalResource) {
  probeDialog.value = item;
}

async function probe(item: GlobalResource) {
  if (!item.enabled) {
    window.alert(t("mcp.enableFirst"));
    return;
  }
  try {
    const result = await mcpTools.probeOne(item);
    if (!result) return;
    if (result.error && !result.tools.length) {
      alertProbeFailure(result);
    }
    if (toolsDialog.value?.item.id === item.id) {
      toolsDialog.value = { item, probe: result };
    }
  } catch (cause) {
    alertError(cause);
  }
}

function brandIcon(item: GlobalResource): string | undefined {
  if (brokenIcons.value.has(item.id)) return undefined;
  return mcpBrandIconUrl({ name: item.name, summary: item.summary });
}

function markIconBroken(id: string) {
  const next = new Set(brokenIcons.value);
  next.add(id);
  brokenIcons.value = next;
}

function toolsCaption(item: GlobalResource): string {
  const probe = mcpTools.probes[item.id];
  return mcpToolsCaption({
    probe,
    // 已有缓存时后台刷新不把文案改成「加载中」
    loading: mcpTools.isLoading(item.id) && !probe,
    enabled: item.enabled,
  });
}

async function openTools(item: GlobalResource) {
  if (!item.enabled) {
    window.alert(t("mcp.enableThenTools"));
    return;
  }
  const cached = mcpTools.probes[item.id];
  if (cached && (cached.ok || cached.tools.length || cached.error)) {
    if (cached.error && !cached.tools.length) {
      alertProbeFailure(cached);
      return;
    }
    toolsDialog.value = { item, probe: cached };
    return;
  }
  try {
    const probeResult = await mcpTools.probeOne(item);
    if (!probeResult) return;
    if (probeResult.error && !probeResult.tools.length) {
      alertProbeFailure(probeResult);
      return;
    }
    toolsDialog.value = { item, probe: probeResult };
  } catch (cause) {
    alertError(cause);
  }
}

onMounted(() => {
  void loadSources();
  void mcpTools.hydrate().then(() => {
    mcpTools.refreshInBackground(servers.value);
  });
});

// 资源数组整体替换时内容往往没变；只在服务器配置真的变化时才排后台刷新（会起 stdio 进程）
watch(
  () => servers.value
    .map((item) => [item.id, item.enabled, item.transport, item.summary, item.configStatus].join("\u0000"))
    .join("\n"),
  () => {
    if (!mcpTools.ready) return;
    mcpTools.refreshInBackground(servers.value);
  },
);

async function refreshList() {
  refreshing.value = true;
  try {
    await store.refreshResources();
    mcpTools.refreshInBackground(servers.value, true);
  } catch (cause) {
    alertError(cause);
  } finally {
    refreshing.value = false;
  }
}

async function openConfig(item: GlobalResource) {
  try {
    await backend.revealResource(item.id);
  } catch (cause) {
    alertError(cause);
  }
}

async function copyPath(item: GlobalResource) {
  try {
    await navigator.clipboard.writeText(item.sourcePath);
  } catch {
    window.prompt(t("common.path"), item.sourcePath);
  }
}

async function installCatalog(item: CatalogItem, owner = "multara") {
  browsing.value = false;
  try {
    await store.installMcp(item.name, item.install, owner);
  } catch (cause) {
    alertError(cause);
  }
}

function openMenu(event: MouseEvent, item: GlobalResource) {
  event.preventDefault();
  menu.value = { x: event.clientX, y: event.clientY, item };
}

const menuItems = computed<ContextMenuItem[]>(() => {
  const item = menu.value?.item;
  if (!item) return [];
  return [
    { id: "probe", label: t("mcp.probeDetails") },
    { id: "config", label: t("mcp.openConfig") },
    { id: "copy", label: t("common.copyPath") },
    { type: "sep" },
    { id: "edit", label: t("common.edit") },
    { id: "auth", label: t("mcp.authorize"), disabled: !canAuthorize(item) },
    { id: "signout", label: t("mcp.signOut") },
    { id: "delete", label: t("common.delete") },
  ];
});

async function onMenu(id: string) {
  const item = menu.value?.item;
  menu.value = null;
  if (!item) return;
  if (id === "probe") openProbe(item);
  if (id === "config") await openConfig(item);
  if (id === "copy") await copyPath(item);
  if (id === "edit") await openEdit(item);
  if (id === "auth") await authorize(item);
  if (id === "signout") {
    try {
      await store.signOutMcp(item.name);
    } catch (cause) {
      alertError(cause);
    }
  }
  if (id === "delete") await remove(item);
}

async function authorizeForm() {
  if (!form.previousName) return;
  await authorize({
    id: `${form.owner}:${form.previousName}`,
    name: form.previousName,
    owner: form.owner || "multara",
    kind: "mcp",
    scope: "global",
    sourceLabel: "",
    description: "",
    sourcePath: "",
    enabled: true,
    updatedAt: "",
    transport: form.transport,
    summary: "",
    needsAuth: true,
    configStatus: "needs_auth",
  });
}
</script>

<template>
  <div class="settings-shell">
    <SideNav
      class="settings-nav"
      :aria-label="t('mcp.filterBySource')"
      :groups="sourceNavGroups"
      :width-key="SETTINGS_NAV_WIDTH_KEY"
      @select="onSourceSelect"
    >
      <template #leading="{ item }">
        <CliSourceIcon :key="item.id" :owner="item.id === ALL_SOURCES ? 'all' : item.id" :size="16" />
      </template>
    </SideNav>

    <div class="settings-body">
    <div class="page">
    <div class="page-header">
      <div>
        <div class="skill-title-line">
          <h1>{{ t("mcp.title") }}</h1>
          <span class="ds-badge">{{ servers.length }}</span>
        </div>
        <p>
          {{ t("mcp.subtitleLong") }}
          <a
            href="https://modelcontextprotocol.io"
            target="_blank"
            rel="noreferrer"
            @click.prevent="backend.openExternalUrl(($event.currentTarget as HTMLAnchorElement).href)"
          >{{ t("mcp.learnMore") }}</a>
        </p>
      </div>
      <div class="header-actions">
        <UiButton :disabled="refreshing" @click="refreshList">
          <RefreshCw :size="14" /> {{ t("common.refresh") }}
        </UiButton>
        <UiButton @click="browsing = true"><Store :size="14" /> {{ t("mcp.browse") }}</UiButton>
        <UiButton variant="primary" @click="openCreate"><Plus :size="16" /> {{ t("mcp.create") }}</UiButton>
      </div>
    </div>

    <div class="page-toolbar skill-toolbar">
      <label class="ds-search collection-search">
        <Search :size="14" />
        <UiInput v-model="query" :placeholder="t('mcp.search')" />
      </label>
      <span class="collection-summary">{{ t("mcp.enabledCount", { enabled: servers.filter((item) => item.enabled).length, total: servers.length }) }}</span>
    </div>

    <UiNotice
      v-if="authorizeError"
      tone="error"
      sticky
      :message="authorizeError"
      @close="authorizeError = ''"
    />

    <UiDirectoryTable class="mcp-directory" :columns="directoryColumns">
      <div v-if="!servers.length" class="ds-empty">
        <span class="ds-empty-icon"><Network :size="18" /></span>
        <h3>{{ t("mcp.empty") }}</h3>
        <p>{{ t("mcp.emptyHint") }}</p>
      </div>
      <div v-else-if="!visible.length" class="ds-empty">
        <span class="ds-empty-icon"><SearchX :size="18" /></span>
        <h3>{{ t("mcp.noMatch") }}</h3>
        <p>{{ t("mcp.noMatchHint", { query }) }}</p>
      </div>
      <template v-else>
        <div
          v-for="item in visible"
          :key="item.id"
          class="agent-directory-row"
          :class="{ disabled: !item.enabled }"
          @contextmenu="openMenu($event, item)"
        >
          <span class="skill-source">
            <CliSourceIcon :owner="item.owner || 'multara'" :size="16" />
            <span>{{ item.sourceLabel }}</span>
          </span>
          <span class="agent-directory-identity">
            <img
              v-if="brandIcon(item)"
              class="mcp-brand-icon"
              :src="brandIcon(item)"
              alt=""
              loading="lazy"
              referrerpolicy="no-referrer"
              @error="markIconBroken(item.id)"
            />
            <span
              v-else
              class="mcp-brand-icon mcp-brand-fallback"
              :style="{ background: mcpBrandFallbackColor(item.name) }"
              aria-hidden="true"
            >{{ mcpBrandInitial(item.name) }}</span>
            <span>
              <strong>
                <template v-for="(part, index) in highlightParts(item.name, query)" :key="index">
                  <mark v-if="index === 1 && part">{{ part }}</mark>
                  <template v-else>{{ part }}</template>
                </template>
              </strong>
              <button
                type="button"
                class="mcp-tools-caption"
                :disabled="!item.enabled || mcpTools.isLoading(item.id)"
                @click="openTools(item)"
              >{{ toolsCaption(item) }}</button>
            </span>
          </span>
          <span><span class="ds-badge">{{ item.transport === "http" ? t("mcp.transport.http") : item.transport === "sse" ? t("mcp.transport.sse") : t("mcp.transport.stdio") }}</span></span>
          <span class="agent-directory-status" :data-status="statusLabel(item)">{{ statusLabel(item) }}</span>
          <span>
            <input class="ds-switch" type="checkbox" :checked="item.enabled" :aria-label="t('mcp.enable', { name: item.name })" @change="toggle(item, $event)" />
          </span>
          <span class="ui-directory-actions">
            <UiButton
              v-if="canAuthorize(item)"
              variant="ghost"
              size="small"
              :disabled="authorizing === item.id"
              @click="authorize(item)"
            >{{ authorizeLabel(item) }}</UiButton>
            <UiButton variant="ghost" size="small" @click="openProbe(item)">{{ t("mcp.details") }}</UiButton>
            <UiButton variant="ghost" size="small" @click="openEdit(item)"><Pencil :size="14" /> {{ t("common.edit") }}</UiButton>
            <UiSelect
              size="sm"
              variant="ghost"
              :model-value="undefined"
              :placeholder="t('mcp.moveTo')"
              :options="transferOptions(item)"
              :searchable="transferOptions(item).length >= 8"
              :aria-label="t('mcp.moveTo')"
              @change="transferMcp(item, $event, false)"
            >
              <template #option-leading="{ option }">
                <CliSourceIcon v-if="option" :owner="String(option.value)" :size="16" />
              </template>
            </UiSelect>
            <UiSelect
              size="sm"
              variant="ghost"
              :model-value="undefined"
              :placeholder="t('mcp.copyTo')"
              :options="transferOptions(item)"
              :searchable="transferOptions(item).length >= 8"
              :aria-label="t('mcp.copyTo')"
              @change="transferMcp(item, $event, true)"
            >
              <template #option-leading="{ option }">
                <CliSourceIcon v-if="option" :owner="String(option.value)" :size="16" />
              </template>
            </UiSelect>
            <UiButton variant="danger-ghost" size="small" @click="remove(item)">{{ t("common.delete") }}</UiButton>
          </span>
        </div>
      </template>
    </UiDirectoryTable>
    </div>
    </div>
  </div>

  <UiModal v-if="editor" @click.self="editor = false">
    <form class="modal domain-modal" @submit.prevent="save">
      <header class="modal-heading">
        <h2>{{ form.previousName ? t("mcp.edit") : t("mcp.new") }}</h2>
        <div class="mcp-mode-toggle ds-segmented" role="tablist">
          <button type="button" :class="{ active: mode === 'ui' }" @click="setMode('ui')">{{ t("mcp.mode.ui") }}</button>
          <button type="button" :class="{ active: mode === 'json' }" @click="setMode('json')">{{ t("mcp.mode.json") }}</button>
        </div>
        <UiButton variant="ghost" size="icon" :tip="t('common.close')" @click="editor = false"><X :size="16" /></UiButton>
      </header>
      <template v-if="mode === 'ui'">
        <label>{{ t("mcp.saveTo") }}
          <UiSelect v-model="form.owner" :disabled="Boolean(form.previousName)" :options="ownerOptions" />
        </label>
        <label>{{ t("mcp.field.name") }}<UiInput v-model="form.name" required :placeholder="t('mcp.placeholder.name')" /></label>
        <label>{{ t("mcp.field.type") }}
          <UiSelect
            v-model="form.transport"
            :options="[
              { value: 'stdio', label: t('mcp.transport.stdio'), description: t('mcp.stdioDesc') },
              { value: 'http', label: t('mcp.transport.http'), description: t('mcp.httpDesc') },
              { value: 'sse', label: 'SSE', description: t('mcp.transport.sseDesc') },
            ]"
          />
        </label>
        <template v-if="form.transport === 'stdio'">
          <label>{{ t("mcp.field.command") }}<UiInput v-model="form.command" required :placeholder="t('mcp.placeholder.command')" /></label>
          <label>{{ t("mcp.argsLabel") }}<UiTextarea v-model="argsText" rows="4" placeholder="-y&#10;@modelcontextprotocol/server-filesystem" /></label>
          <label>{{ t("mcp.field.env") }}<UiTextarea v-model="form.envText" rows="3" placeholder="API_KEY=${input:token}" /></label>
          <label>{{ t("mcp.cwd") }}<UiInput v-model="form.cwd" placeholder="${workspaceFolder}" /></label>
          <label>{{ t("mcp.envFile") }}<UiInput v-model="form.envFile" placeholder="${workspaceFolder}/.env" /></label>
        </template>
        <template v-else>
          <label>{{ t("mcp.field.url") }}<UiInput v-model="form.url" required placeholder="https://example.com/mcp" /></label>
          <label>{{ t("mcp.field.headers") }}<UiTextarea v-model="form.headersText" rows="3" placeholder="Authorization: Bearer ${input:token}" /></label>
        </template>
        <label v-for="id in inputIds" :key="id">{{ t("mcp.inputValue", { id }) }}
          <UiInput v-model="inputValues[id]" :placeholder="t('mcp.inputPlaceholder', { id })" />
        </label>
        <label class="check-row">
          <input v-model="form.enabled" class="ds-switch" type="checkbox" />
          <span>{{ t("common.enabled") }}</span>
        </label>
      </template>
      <template v-else>
        <label>{{ t("mcp.field.name") }}<UiInput v-model="form.name" :placeholder="t('mcp.placeholder.name')" /></label>
        <p>{{ t("mcp.jsonHint") }} {{ t("mcp.jsonExtraHint") }}</p>
        <UiTextarea v-model="jsonText" class="mcp-json-source" rows="16" spellcheck="false" />
      </template>
      <p v-if="error" class="form-error">{{ error }}</p>
      <div class="modal-actions">
        <UiButton
          v-if="form.previousName && (form.transport === 'http' || form.transport === 'sse') && detectedNeedsAuth(form)"
          type="button"
          :disabled="authorizing === `${form.owner}:${form.previousName}`"
          @click="authorizeForm"
        >{{ authorizing === `${form.owner}:${form.previousName}` ? t("mcp.authorizing") : t("mcp.authorize") }}</UiButton>
        <UiButton type="button" @click="discard">{{ t("mcp.discardChanges") }}</UiButton>
        <UiButton type="button" @click="editor = false">{{ t("common.cancel") }}</UiButton>
        <UiButton variant="primary" type="submit" :class="{ 'is-loading': busy }" :disabled="busy">{{ busy ? t("common.saving") : t("common.save") }}</UiButton>
      </div>
    </form>
  </UiModal>
  <UiModal v-if="probeDialog" @click.self="probeDialog = null">
    <div class="modal domain-modal mcp-probe-modal" role="dialog" aria-modal="true">
      <header class="modal-heading">
        <div class="mcp-tools-modal-title">
          <img
            v-if="brandIcon(probeDialog)"
            class="mcp-brand-icon"
            :src="brandIcon(probeDialog)"
            alt=""
            loading="lazy"
            referrerpolicy="no-referrer"
            @error="markIconBroken(probeDialog.id)"
          />
          <span
            v-else
            class="mcp-brand-icon mcp-brand-fallback"
            :style="{ background: mcpBrandFallbackColor(probeDialog.name) }"
            aria-hidden="true"
          >{{ mcpBrandInitial(probeDialog.name) }}</span>
          <h2>{{ t("mcp.probeTitle") }}</h2>
        </div>
        <UiButton variant="ghost" size="icon" :tip="t('common.close')" @click="probeDialog = null"><X :size="16" /></UiButton>
      </header>
      <template v-if="activeProbe">
        <p v-if="activeProbe.error" class="form-error">{{ friendlyError(activeProbe.error) }}</p>
        <p v-else class="mcp-probe-summary">
          {{ activeProbe.serverName }} {{ activeProbe.serverVersion }} · {{ activeProbe.durationMs }}ms
        </p>
        <p v-if="activeProbe.missingInputs?.length">{{ t("mcp.missingInputs", { list: activeProbe.missingInputs.join("、") }) }}</p>
        <p v-if="activeProbe.resources.length">{{ t("mcp.resources", { list: activeProbe.resources.map((entry) => entry.name || entry.uri).join("、") }) }}</p>
        <p v-if="activeProbe.prompts.length">{{ t("mcp.prompts", { list: activeProbe.prompts.map((entry) => entry.name).join("、") }) }}</p>
        <pre v-if="activeProbeLog" class="mcp-log">{{ activeProbeLog }}</pre>
        <p v-else class="mcp-tools-empty">{{ t("mcp.noLogs") }}</p>
      </template>
      <p v-else class="mcp-tools-empty">{{ t("mcp.notProbed") }}</p>
      <div class="modal-actions">
        <UiButton
          v-if="canAuthorize(probeDialog)"
          :disabled="authorizing === probeDialog.id"
          @click="authorize(probeDialog)"
        >{{ authorizing === probeDialog.id ? t("mcp.authorizing") : authorizeLabel(probeDialog) }}</UiButton>
        <UiButton
          variant="primary"
          :disabled="mcpTools.isLoading(probeDialog.id) || !probeDialog.enabled"
          @click="probe(probeDialog)"
        >{{ mcpTools.isLoading(probeDialog.id) ? t("mcp.probing") : t("mcp.probe") }}</UiButton>
        <UiButton @click="probeDialog = null">{{ t("common.close") }}</UiButton>
      </div>
    </div>
  </UiModal>
  <UiModal v-if="toolsDialog" @click.self="toolsDialog = null">
    <div class="modal domain-modal mcp-tools-modal" role="dialog" aria-modal="true">
      <header class="modal-heading">
        <div class="mcp-tools-modal-title">
          <img
            v-if="brandIcon(toolsDialog.item)"
            class="mcp-brand-icon"
            :src="brandIcon(toolsDialog.item)"
            alt=""
            loading="lazy"
            referrerpolicy="no-referrer"
            @error="markIconBroken(toolsDialog.item.id)"
          />
          <span
            v-else
            class="mcp-brand-icon mcp-brand-fallback"
            :style="{ background: mcpBrandFallbackColor(toolsDialog.item.name) }"
            aria-hidden="true"
          >{{ mcpBrandInitial(toolsDialog.item.name) }}</span>
          <h2>{{ toolsDialog.probe.error ? t("mcp.toolsFailedTitle") : t("mcp.toolsTitle") }}</h2>
        </div>
        <UiButton variant="ghost" size="icon" :tip="t('common.close')" @click="toolsDialog = null"><X :size="16" /></UiButton>
      </header>
      <p v-if="toolsDialog.probe.error" class="form-error">{{ friendlyError(toolsDialog.probe.error) }}</p>
      <ul v-else-if="toolsDialog.probe.tools.length" class="mcp-tools-list">
        <li v-for="tool in toolsDialog.probe.tools" :key="tool.name">
          <strong>{{ tool.title || tool.name }}</strong>
          <small v-if="tool.title && tool.title !== tool.name">{{ tool.name }}</small>
          <p v-if="tool.description">{{ tool.description }}</p>
        </li>
      </ul>
      <p v-else class="mcp-tools-empty">{{ t("mcp.noTools") }}</p>
      <div class="modal-actions">
        <UiButton
          v-if="canAuthorize(toolsDialog.item)"
          :disabled="authorizing === toolsDialog.item.id"
          @click="authorize(toolsDialog.item)"
        >{{ authorizing === toolsDialog.item.id ? t("mcp.authorizing") : authorizeLabel(toolsDialog.item) }}</UiButton>
        <UiButton variant="primary" @click="toolsDialog = null">{{ t("common.close") }}</UiButton>
      </div>
    </div>
  </UiModal>
  <CatalogBrowse v-if="browsing" kind="mcp" @close="browsing = false" @install="installCatalog" />
  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="menuItems"
    @select="onMenu"
    @close="menu = null"
  />
</template>
