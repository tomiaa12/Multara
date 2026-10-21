<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { ArrowDown, ArrowUp, Plus, Search, SearchX, Sparkles, Store } from "@lucide/vue";
import type { CatalogItem, GlobalResource, PromptFile, ResourceSource } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import CliSourceIcon from "../components/CliSourceIcon.vue";
import SideNav, { type SideNavGroup, type SideNavItem } from "../components/SideNav.vue";
import ContextMenu, { type ContextMenuItem } from "../components/ui/ContextMenu.vue";
import type { DirectoryColumn } from "../components/ui/directory-table";
import { cliLabel, cliSkillsDisplay } from "../domain/cli-catalog";
import { highlightParts } from "./resource-text";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import * as backend from "../services/backend";
import CreateSkillDialog from "./CreateSkillDialog.vue";
import CatalogBrowse from "./CatalogBrowse.vue";
import { ALL_SOURCES, compareResourceSourceLabels } from "./resource-source-nav";

type Usage = "all" | "used" | "unused";
type SortField = "updated" | "name";

const store = useAppStore();
const router = useRouter();
const creating = ref(false);
const browsing = ref(false);
const converting = ref(false);
const prompts = ref<PromptFile[]>([]);
const sources = ref<ResourceSource[]>([]);
const query = ref("");
const source = ref(ALL_SOURCES);
const usage = ref<Usage>("all");
const sortField = ref<SortField>("updated");
const sortDesc = ref(true);
const expanded = ref(new Set<string>());
const menu = ref<{ x: number; y: number; skill: GlobalResource } | null>(null);

const skills = computed(() => store.data?.resources.filter((item) => item.kind === "skill") ?? []);

function usersOf(skill: GlobalResource) {
  return store.data?.agents.filter((agent) => agent.skillIds.includes(skill.id)) ?? [];
}

function sourceName(owner: string) {
  const fromSkill = skills.value.find((item) => item.owner === owner)?.sourceLabel;
  if (fromSkill) return fromSkill;
  const labeled = cliLabel(owner);
  if (labeled !== owner) return labeled;
  return store.data?.runtimes.find((runtime) => runtime.id === owner)?.name ?? owner;
}

const sourceTabs = computed(() => {
  const counts = new Map<string, number>();
  for (const skill of skills.value) {
    const owner = skill.owner || "multara";
    counts.set(owner, (counts.get(owner) ?? 0) + 1);
  }
  const items = [...counts.entries()]
    .map(([id, count]) => ({ id, label: sourceName(id), count }))
    .sort(compareResourceSourceLabels);
  return [{ id: ALL_SOURCES, label: t("common.all"), count: skills.value.length }, ...items];
});

watch(sourceTabs, (tabs) => {
  if (!tabs.some((tab) => tab.id === source.value)) source.value = ALL_SOURCES;
});

const sourceNavGroups = computed<SideNavGroup[]>(() => [{
  label: t("skills.source"),
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

const visibleSkills = computed(() => {
  const text = query.value.trim().toLowerCase();
  const rows = skills.value.filter((skill) => {
    const used = usersOf(skill).length > 0;
    if (source.value !== ALL_SOURCES && (skill.owner || "multara") !== source.value) return false;
    if (usage.value === "used" && !used) return false;
    if (usage.value === "unused" && used) return false;
    if (!text) return true;
    return skill.name.toLowerCase().includes(text)
      || skill.description.toLowerCase().includes(text)
      || sourceName(skill.owner).toLowerCase().includes(text);
  });
  const direction = sortDesc.value ? -1 : 1;
  return [...rows].sort((left, right) => {
    const leftOwner = left.owner || "multara";
    const rightOwner = right.owner || "multara";
    const bySource = compareResourceSourceLabels(
      { id: leftOwner, label: sourceName(leftOwner) },
      { id: rightOwner, label: sourceName(rightOwner) },
    );
    if (bySource !== 0) return bySource;
    if (sortField.value === "name") return left.name.localeCompare(right.name) * direction;
    return (left.updatedAt || "").localeCompare(right.updatedAt || "") * direction;
  });
});

function toggleSort(field: SortField) {
  if (sortField.value === field) sortDesc.value = !sortDesc.value;
  else {
    sortField.value = field;
    sortDesc.value = field === "updated";
  }
}

function onDirectorySort(key: string) {
  if (key === "name" || key === "updated") toggleSort(key);
}

const directoryColumns = computed<DirectoryColumn[]>(() => [
  { key: "source", label: t("common.source"), size: "112px" },
  { key: "name", label: t("common.name"), size: "minmax(0, 1fr)", sortable: true },
  { key: "users", label: t("skills.col.users"), size: "120px" },
  { key: "updated", label: t("common.updatedAt"), size: "88px", sortable: true },
  { key: "actions", label: t("skills.col.actions"), size: "minmax(300px, 1.1fr)", actions: true },
]);

function canExpand(skill: GlobalResource) {
  const text = skill.description || "";
  return expanded.value.has(skill.id) || text.includes("\n") || text.length > 72;
}

function toggleExpand(id: string) {
  const next = new Set(expanded.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  expanded.value = next;
}

function openSkill(skill: GlobalResource) {
  void router.push(`/skills/${encodeURIComponent(skill.id)}`);
}

function onCreated(id: string) {
  creating.value = false;
  void router.push(`/skills/${encodeURIComponent(id)}`);
}

async function installCatalog(item: CatalogItem, owner = "multara") {
  browsing.value = false;
  try {
    const created = await store.importSkillFromUrl(item.install, owner);
    void router.push(`/skills/${encodeURIComponent(created.id)}`);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

async function copyPath(skill: GlobalResource) {
  try {
    await navigator.clipboard.writeText(skill.sourcePath);
  } catch {
    window.prompt(t("common.path"), skill.sourcePath);
  }
}

async function reveal(skill: GlobalResource) {
  try {
    await backend.revealResource(skill.id);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

async function removeSkill(skill: GlobalResource) {
  if (!window.confirm(t("skills.deleteConfirm", { name: skill.name }))) return;
  try {
    await store.deleteSkill(skill.id);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

function transferOptions(skill: GlobalResource) {
  return sources.value
    .filter((item) => item.owner !== (skill.owner || "multara"))
    .map((item) => ({
      value: item.owner,
      label: item.label,
      meta: item.scope === "workspace" ? t("common.workspaceScope") : cliSkillsDisplay(item.owner),
    }));
}

async function transferSkill(skill: GlobalResource, targetOwner: unknown, copy: boolean) {
  if (targetOwner == null || targetOwner === "") return;
  try {
    await store.transferResource(skill.id, String(targetOwner), copy);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

function openMenu(event: MouseEvent, skill: GlobalResource) {
  event.preventDefault();
  menu.value = { x: event.clientX, y: event.clientY, skill };
}

const menuItems = computed<ContextMenuItem[]>(() => {
  const skill = menu.value?.skill;
  if (!skill) return [];
  return [
    { id: "open", label: t("common.open"), icon: "go-to-file" },
    { id: "copy", label: t("common.copyPath"), icon: "copy" },
    { id: "reveal", label: t("common.revealInFinder"), icon: "folder-opened" },
    { type: "sep" },
    { id: "delete", label: t("common.delete"), icon: "trash" },
  ];
});

async function onMenu(id: string) {
  const skill = menu.value?.skill;
  menu.value = null;
  if (!skill) return;
  if (id === "open") openSkill(skill);
  if (id === "copy") await copyPath(skill);
  if (id === "reveal") await reveal(skill);
  if (id === "delete") await removeSkill(skill);
}

async function openConvert() {
  converting.value = true;
  try {
    prompts.value = await backend.listPromptFiles();
  } catch (cause) {
    converting.value = false;
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

async function convert(path: string) {
  try {
    const created = await store.convertPromptToSkill(path);
    converting.value = false;
    void router.push(`/skills/${encodeURIComponent(created.id)}`);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

onMounted(() => {
  void backend.listResourceSources("skill").then((rows) => {
    sources.value = rows;
  });
});

const sortLabel = computed(() => (sortField.value === "updated" ? t("common.updatedAt") : t("common.name")));
const emptyHint = computed(() => {
  const text = query.value.trim();
  if (text) return t("skills.noMatchQuery", { query: text });
  if (source.value !== ALL_SOURCES) return t("skills.noMatchSource");
  return t("skills.noMatchFilter");
});
</script>

<template>
  <div class="settings-shell">
    <SideNav
      class="settings-nav"
      :aria-label="t('skills.filterBySource')"
      :groups="sourceNavGroups"
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
          <h1>Skills</h1>
          <span class="ds-badge">{{ skills.length }}</span>
        </div>
        <p>
          {{ t("skills.subtitleLong") }}
          <a
            href="https://code.visualstudio.com/docs/copilot/customization/agent-skills"
            target="_blank"
            rel="noreferrer"
            @click.prevent="backend.openExternalUrl(($event.currentTarget as HTMLAnchorElement).href)"
          >{{ t("skills.learnMore") }}</a>
        </p>
      </div>
      <div class="header-actions">
        <UiButton @click="browsing = true"><Store :size="14" /> {{ t("skills.browse") }}</UiButton>
        <UiButton @click="openConvert">{{ t("skills.convertPrompt") }}</UiButton>
        <UiButton variant="primary" @click="creating = true"><Plus :size="14" /> {{ t("skills.create") }}</UiButton>
      </div>
    </div>

    <div class="page-toolbar skill-toolbar">
      <label class="ds-search collection-search">
        <Search :size="14" />
        <UiInput v-model="query" :placeholder="t('skills.search')" />
      </label>
      <div class="header-actions">
        <UiSelect
          v-model="usage"
          :aria-label="t('skills.filterByUsage')"
          :options="[
            { value: 'all', label: t('common.all') },
            { value: 'used', label: t('skills.used') },
            { value: 'unused', label: t('skills.unused') },
          ]"
        />
        <UiButton :title="sortDesc ? t('common.desc') : t('common.asc')" @click="toggleSort(sortField)">
          {{ sortLabel }}
          <ArrowDown v-if="sortDesc" :size="14" />
          <ArrowUp v-else :size="14" />
        </UiButton>
      </div>
    </div>

    <UiDirectoryTable class="skill-directory" :columns="directoryColumns" @sort="onDirectorySort">
      <div v-if="!skills.length" class="ds-empty">
        <span class="ds-empty-icon"><Sparkles :size="18" /></span>
        <h3>{{ t("skills.empty") }}</h3>
        <p>{{ t("skills.emptyHint") }}</p>
      </div>
      <div v-else-if="!visibleSkills.length" class="ds-empty">
        <span class="ds-empty-icon"><SearchX :size="18" /></span>
        <h3>{{ t("skills.noMatch") }}</h3>
        <p>{{ emptyHint }}</p>
      </div>
      <template v-else>
        <div
          v-for="skill in visibleSkills"
          :key="skill.id"
          class="agent-directory-row"
          @contextmenu="openMenu($event, skill)"
        >
          <span class="skill-source">
            <CliSourceIcon :owner="skill.owner || 'multara'" :size="16" />
            <span>{{ skill.sourceLabel || sourceName(skill.owner) }}</span>
          </span>
          <span class="agent-directory-identity">
            <span>
              <strong>
                <template v-for="(part, index) in highlightParts(skill.name, query)" :key="index">
                  <mark v-if="index === 1 && part">{{ part }}</mark>
                  <template v-else>{{ part }}</template>
                </template>
              </strong>
              <span class="skill-desc-row">
                <small class="skill-desc" :class="{ 'is-open': expanded.has(skill.id) }">{{ skill.description || t("common.noDescription") }}</small>
                <span v-if="canExpand(skill)" class="skill-desc-actions">
                  <button type="button" class="skill-desc-toggle" @click="toggleExpand(skill.id)">
                    {{ expanded.has(skill.id) ? t("skills.collapse") : t("skills.expand") }}
                  </button>
                </span>
              </span>
            </span>
          </span>
          <span class="skill-used-by">
            <template v-if="usersOf(skill).length">
              <AgentAvatar
                v-for="agent in usersOf(skill).slice(0, 4)"
                :key="agent.id"
                :value="agent.avatar"
                :name="agent.name"
                :size="22"
              />
              <small v-if="usersOf(skill).length > 4">+{{ usersOf(skill).length - 4 }}</small>
            </template>
          </span>
          <UiRelativeTime :value="skill.updatedAt" />
          <span class="ui-directory-actions">
            <UiButton variant="ghost" size="small" @click="openSkill(skill)">{{ t("skills.view") }}</UiButton>
            <UiSelect
              size="sm"
              variant="ghost"
              :model-value="undefined"
              :placeholder="t('skills.moveTo')"
              :options="transferOptions(skill)"
              :searchable="transferOptions(skill).length >= 8"
              :aria-label="t('skills.moveTo')"
              @change="transferSkill(skill, $event, false)"
            >
              <template #option-leading="{ option }">
                <CliSourceIcon v-if="option" :owner="String(option.value)" :size="16" />
              </template>
            </UiSelect>
            <UiSelect
              size="sm"
              variant="ghost"
              :model-value="undefined"
              :placeholder="t('skills.copyTo')"
              :options="transferOptions(skill)"
              :searchable="transferOptions(skill).length >= 8"
              :aria-label="t('skills.copyTo')"
              @change="transferSkill(skill, $event, true)"
            >
              <template #option-leading="{ option }">
                <CliSourceIcon v-if="option" :owner="String(option.value)" :size="16" />
              </template>
            </UiSelect>
            <UiButton variant="danger-ghost" size="small" @click="removeSkill(skill)">{{ t("common.delete") }}</UiButton>
          </span>
        </div>
      </template>
    </UiDirectoryTable>
    </div>
    </div>
  </div>

  <CreateSkillDialog
    v-if="creating"
    @close="creating = false"
    @created="onCreated"
  />
  <CatalogBrowse v-if="browsing" kind="skill" @close="browsing = false" @install="installCatalog" />
  <div v-if="converting" class="modal-backdrop" @click.self="converting = false">
    <div class="modal domain-modal">
      <header class="modal-heading">
        <h2>{{ t("skills.convertTitle") }}</h2>
        <UiButton @click="converting = false">{{ t("common.close") }}</UiButton>
      </header>
      <p v-if="!prompts.length">{{ t("skills.convertEmpty") }}</p>
      <button
        v-for="item in prompts"
        :key="item.path"
        type="button"
        class="skill-create-method"
        @click="convert(item.path)"
      >
        <span>
          <strong>{{ item.name }}</strong>
          <small>{{ item.sourceLabel }} · {{ item.description || item.path }}</small>
        </span>
      </button>
    </div>
  </div>
  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="menuItems"
    @select="onMenu"
    @close="menu = null"
  />
</template>
