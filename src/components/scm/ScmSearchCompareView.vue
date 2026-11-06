<script setup lang="ts">
/**
 * GitLens Search & Compare：欢迎态 / 搜索建议 / 结果树（比较 ahead·behind·文件 + 搜索结果）。
 */
import { computed, nextTick, ref, watch } from "vue";
import Codicon from "../ui/Codicon.vue";
import UiButton from "../ui/UiButton.vue";
import UiPane from "../ui/UiPane.vue";
import UiSelect from "../ui/UiSelect.vue";
import type { SelectOption } from "../ui/select-types";
import type { GitHistoryItem, GitRepositoryStatus } from "../../domain/types";
import * as backend from "../../services/backend";
import { t } from "../../i18n";
import { parseSearchQuery, SEARCH_HELP_ITEMS } from "./scm-search-compare";
import ScmHistoryCommits from "./ScmHistoryCommits.vue";
import type { ScmOpenChangesPayload, ScmOpenTarget } from "../workbench/scm-groups";

type Picker = "none" | "search" | "compare";

type SearchNode = {
  id: string;
  kind: "search";
  query: string;
  rows: GitHistoryItem[];
  loading: boolean;
  error: string;
  expanded: boolean;
};

type CompareNode = {
  id: string;
  kind: "compare";
  /** 正在比较的引用（Comparing {head} with {base}） */
  head: string;
  base: string;
  ahead: number;
  behind: number;
  files: number;
  aheadRows: GitHistoryItem[];
  behindRows: GitHistoryItem[];
  loading: boolean;
  error: string;
  expanded: boolean;
  aheadExpanded: boolean;
  behindExpanded: boolean;
};

type ResultNode = SearchNode | CompareNode;

const props = defineProps<{
  title: string;
  storageKey: string;
  sizeKey: string;
  workspaceId: string;
  repository: GitRepositoryStatus | null;
  epoch: number;
}>();

const emit = defineEmits<{
  open: [target: ScmOpenTarget];
  openChanges: [payload: ScmOpenChangesPayload];
}>();

const picker = ref<Picker>("none");
const query = ref("");
const searchInput = ref<HTMLInputElement | null>(null);
const caseSensitive = ref(false);
const leftRef = ref("");
const rightRef = ref("HEAD");
const nodes = ref<ResultNode[]>([]);
let seq = 0;
const nodeTickets = new Map<string, number>();

function bumpTicket(id: string) {
  const next = (nodeTickets.get(id) ?? 0) + 1;
  nodeTickets.set(id, next);
  return next;
}

function isCurrent(id: string, stamp: number) {
  return nodeTickets.get(id) === stamp;
}

const repoName = computed(() => props.repository?.name || "");

const refOptions = computed<SelectOption[]>(() => {
  const repo = props.repository;
  if (!repo) return [];
  const seen = new Set<string>();
  const out: SelectOption[] = [];
  const push = (value: string) => {
    const key = value.trim();
    if (!key || seen.has(key)) return;
    seen.add(key);
    out.push({ value: key, label: key });
  };
  push("HEAD");
  if (repo.branch) push(repo.branch);
  if (repo.upstream) push(repo.upstream);
  for (const name of repo.branches ?? []) push(name);
  for (const name of repo.remoteBranches ?? []) push(name);
  return out;
});

const showWelcome = computed(() => picker.value === "none" && !nodes.value.length);

function nextId(kind: string) {
  seq += 1;
  return `${kind}-${seq}`;
}

async function focusSearch() {
  picker.value = "search";
  await nextTick();
  searchInput.value?.focus();
  searchInput.value?.select();
}

function openComparePicker() {
  picker.value = "compare";
  if (!leftRef.value) {
    leftRef.value = props.repository?.upstream || props.repository?.branch || "";
  }
  if (!rightRef.value) rightRef.value = props.repository?.branch || "HEAD";
}

function cancelPicker() {
  picker.value = "none";
}

function prefill(insert: string) {
  query.value = insert;
  void focusSearch();
}

function matchesItem(item: GitHistoryItem, lower: string) {
  if (item.subject.toLowerCase().includes(lower)) return true;
  if (item.author.toLowerCase().includes(lower)) return true;
  if (item.hash.toLowerCase().startsWith(lower)) return true;
  return false;
}

function patchNode(id: string, patch: Partial<SearchNode> & Partial<CompareNode>) {
  nodes.value = nodes.value.map((item) => (item.id === id ? { ...item, ...patch } : item));
}

function dismiss(id: string) {
  bumpTicket(id);
  nodeTickets.delete(id);
  nodes.value = nodes.value.filter((item) => item.id !== id);
}

function clearAll() {
  for (const node of nodes.value) bumpTicket(node.id);
  nodeTickets.clear();
  nodes.value = [];
  picker.value = "none";
  query.value = "";
}

async function reloadSearch(node: SearchNode) {
  if (!props.workspaceId) return;
  const stamp = bumpTicket(node.id);
  patchNode(node.id, { loading: true, error: "" });
  try {
    const parsed = parseSearchQuery(node.query);
    const grep = parsed.grep || parsed.free || undefined;
    const path = parsed.file || undefined;
    const range = parsed.ref?.includes("..") ? parsed.ref : undefined;
    const page = await backend.gitLog(props.workspaceId, 0, 80, path, { grep, range });
    if (!isCurrent(node.id, stamp)) return;
    let items = page.items;
    if (parsed.sha) {
      const sha = parsed.sha.toLowerCase();
      items = items.filter((item) => item.hash.toLowerCase().startsWith(sha));
    }
    if (parsed.author) {
      const author = parsed.author.toLowerCase();
      items = items.filter((item) => item.author.toLowerCase().includes(author));
    }
    if (parsed.excludeGrep) {
      const exclude = parsed.excludeGrep.toLowerCase();
      items = items.filter((item) => !item.subject.toLowerCase().includes(exclude));
    }
    if (caseSensitive.value && grep) {
      items = items.filter((item) => item.subject.includes(grep));
    }
    if (parsed.free && !parsed.grep) {
      const lower = parsed.free.toLowerCase();
      const recent = await backend.gitLog(props.workspaceId, 0, 100);
      if (!isCurrent(node.id, stamp)) return;
      const seen = new Set(items.map((item) => item.hash));
      for (const item of recent.items) {
        if (seen.has(item.hash) || !matchesItem(item, lower)) continue;
        seen.add(item.hash);
        items.push(item);
      }
    }
    patchNode(node.id, { rows: items, loading: false });
  } catch (cause) {
    if (!isCurrent(node.id, stamp)) return;
    patchNode(node.id, {
      rows: [],
      loading: false,
      error: cause instanceof Error ? cause.message : String(cause),
    });
  }
}

async function reloadCompare(node: CompareNode) {
  if (!props.workspaceId) return;
  const stamp = bumpTicket(node.id);
  patchNode(node.id, { loading: true, error: "" });
  try {
    const [stats, aheadPage, behindPage] = await Promise.all([
      backend.gitCompareRefs(props.workspaceId, node.base, node.head),
      backend.gitLog(props.workspaceId, 0, 50, undefined, { range: `${node.base}..${node.head}` }),
      backend.gitLog(props.workspaceId, 0, 50, undefined, { range: `${node.head}..${node.base}` }),
    ]);
    if (!isCurrent(node.id, stamp)) return;
    patchNode(node.id, {
      ahead: stats.ahead,
      behind: stats.behind,
      files: stats.files,
      aheadRows: aheadPage.items,
      behindRows: behindPage.items,
      loading: false,
    });
  } catch (cause) {
    if (!isCurrent(node.id, stamp)) return;
    patchNode(node.id, {
      loading: false,
      error: cause instanceof Error ? cause.message : String(cause),
    });
  }
}

async function executeSearch(rawQuery: string) {
  const q = rawQuery.trim();
  if (!props.workspaceId || !q) return;
  const existing = nodes.value.find((item) => item.kind === "search" && item.query === q);
  if (existing?.kind === "search") {
    picker.value = "none";
    await reloadSearch(existing);
    return;
  }
  const node: SearchNode = {
    id: nextId("search"),
    kind: "search",
    query: q,
    rows: [],
    loading: true,
    error: "",
    expanded: true,
  };
  nodes.value = [node, ...nodes.value];
  picker.value = "none";
  await reloadSearch(node);
}

async function executeCompare() {
  const base = leftRef.value.trim();
  const head = rightRef.value.trim() || "HEAD";
  if (!props.workspaceId || !base || !head || base === head) return;
  const existing = nodes.value.find(
    (item) => item.kind === "compare" && item.head === head && item.base === base,
  );
  if (existing?.kind === "compare") {
    picker.value = "none";
    await reloadCompare(existing);
    return;
  }
  const node: CompareNode = {
    id: nextId("compare"),
    kind: "compare",
    head,
    base,
    ahead: 0,
    behind: 0,
    files: 0,
    aheadRows: [],
    behindRows: [],
    loading: true,
    error: "",
    expanded: true,
    aheadExpanded: false,
    behindExpanded: false,
  };
  nodes.value = [node, ...nodes.value];
  picker.value = "none";
  await reloadCompare(node);
}

async function refreshAll() {
  await Promise.all(
    nodes.value.map((node) => (node.kind === "search" ? reloadSearch(node) : reloadCompare(node))),
  );
}

function swapCompare(node: CompareNode) {
  patchNode(node.id, { head: node.base, base: node.head, aheadExpanded: false, behindExpanded: false });
  const updated = nodes.value.find((item) => item.id === node.id);
  if (updated?.kind === "compare") void reloadCompare(updated);
}

function refreshCompare(node: CompareNode) {
  void reloadCompare(node);
}

function toggleExpanded(node: ResultNode) {
  node.expanded = !node.expanded;
}

function toggleBehind(node: CompareNode) {
  node.aheadExpanded = false;
  node.behindExpanded = !node.behindExpanded;
}

function toggleAhead(node: CompareNode) {
  node.behindExpanded = false;
  node.aheadExpanded = !node.aheadExpanded;
}

function searchLabel(node: SearchNode) {
  if (node.loading) return t("scm.searchCompare.searching", { q: node.query });
  if (!node.rows.length) return t("scm.searchCompare.noSearchResults", { q: node.query });
  return t("scm.searchCompare.searchResultsFor", { n: node.rows.length, q: node.query });
}

function compareLabel(node: CompareNode) {
  return t("scm.searchCompare.comparing", { head: node.head, base: node.base });
}

function behindLabel(count: number) {
  return t("scm.searchCompare.behindCommits", { n: count });
}

function aheadLabel(count: number) {
  return t("scm.searchCompare.aheadCommits", { n: count });
}

function filesLabel(count: number) {
  if (count <= 0) return t("scm.searchCompare.noFilesChanged");
  return t("scm.searchCompare.filesChanged", { n: count });
}

watch(
  () => [props.workspaceId, props.epoch] as const,
  () => {
    if (!refOptions.value.some((item) => item.value === leftRef.value)) {
      leftRef.value = props.repository?.upstream || props.repository?.branch || "";
    }
    if (!rightRef.value) rightRef.value = props.repository?.branch || "HEAD";
    if (nodes.value.length) void refreshAll();
  },
);

watch(
  () => props.repository?.branch,
  (branch) => {
    if (!leftRef.value && branch) leftRef.value = props.repository?.upstream || branch;
    if (!rightRef.value && branch) rightRef.value = branch;
  },
  { immediate: true },
);
</script>

<template>
  <UiPane
    :title="title"
    :storage-key="storageKey"
    :size-key="sizeKey"
    :default-size="240"
    :default-expanded="false"
    :min-body="80"
    :actions-visible="true"
  >
    <template #actions>
      <UiButton variant="ghost" size="icon" :tip="t('scm.searchCompare.searchCommits')" @click="focusSearch">
        <Codicon name="search" />
      </UiButton>
      <UiButton variant="ghost" size="icon" :tip="t('scm.searchCompare.compareRefs')" @click="openComparePicker">
        <Codicon name="git-compare" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :tip="t('scm.searchCompare.clear')"
        :disabled="!nodes.length && picker === 'none'"
        @click="clearAll"
      >
        <Codicon name="clear-all" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        :tip="t('scm.inspect.refresh')"
        :disabled="!nodes.length"
        @click="refreshAll"
      >
        <Codicon name="refresh" />
      </UiButton>
    </template>

    <div class="scm-sc">
      <!-- GitLens 欢迎态 -->
      <div v-if="showWelcome" class="scm-sc-welcome">
        <div class="scm-sc-welcome-block">
          <p class="scm-sc-welcome-text">
            {{ t("scm.searchCompare.welcomeSearchBefore") }}
            <button type="button" class="scm-sc-link" @click="prefill('message:')">
              {{ t("scm.searchCompare.welcomeMessage") }}
            </button>
            {{ t("scm.searchCompare.welcomeSep1") }}
            <button type="button" class="scm-sc-link" @click="prefill('author:')">
              {{ t("scm.searchCompare.welcomeAuthor") }}
            </button>
            {{ t("scm.searchCompare.welcomeSep2") }}
            <button type="button" class="scm-sc-link" @click="prefill('commit:')">
              {{ t("scm.searchCompare.welcomeSha") }}
            </button>
            {{ t("scm.searchCompare.welcomeSep3") }}
            <button type="button" class="scm-sc-link" @click="prefill('file:')">
              {{ t("scm.searchCompare.welcomeFile") }}
            </button>
            {{ t("scm.searchCompare.welcomeSep4") }}
            <button type="button" class="scm-sc-link" @click="prefill('change:')">
              {{ t("scm.searchCompare.welcomeChange") }}
            </button>
            {{ t("scm.searchCompare.welcomeSearchAfter") }}
          </p>
          <button type="button" class="scm-sc-welcome-btn" @click="focusSearch">
            {{ t("scm.searchCompare.searchCommits") }}
          </button>
        </div>
        <div class="scm-sc-welcome-block">
          <p class="scm-sc-welcome-text">
            {{ t("scm.searchCompare.welcomeCompareBefore") }}
            <span class="scm-sc-link scm-sc-link--static">{{ t("scm.searchCompare.welcomeCompareRef") }}</span>
            {{ t("scm.searchCompare.welcomeCompareMid") }}
            <span class="scm-sc-link scm-sc-link--static">{{ t("scm.searchCompare.welcomeCompareRef") }}</span>
            {{ t("scm.searchCompare.welcomeCompareAfter") }}
          </p>
          <button type="button" class="scm-sc-welcome-btn" @click="openComparePicker">
            {{ t("scm.searchCompare.compareRefs") }}
          </button>
        </div>
      </div>

      <!-- 搜索输入 + 语法建议（对齐 GitLens QuickPick） -->
      <div v-else-if="picker === 'search'" class="scm-sc-picker">
        <div class="scm-sc-search-bar">
          <input
            ref="searchInput"
            v-model="query"
            class="scm-sc-search-input"
            type="text"
            :placeholder="t('scm.searchCompare.searchPlaceholder')"
            @keydown.enter.prevent="executeSearch(query)"
            @keydown.escape.prevent="cancelPicker"
          />
          <button
            type="button"
            class="scm-sc-search-case"
            :class="{ 'is-on': caseSensitive }"
            :title="t('scm.searchCompare.matchCase')"
            @click="caseSensitive = !caseSensitive"
          >
            Aa
          </button>
        </div>
        <div class="scm-sc-help">
          <button
            v-for="item in SEARCH_HELP_ITEMS"
            :key="item.id"
            type="button"
            class="scm-sc-help-row"
            :class="{ 'has-divider': item.dividerBefore }"
            @click="prefill(item.insert)"
          >
            <span class="scm-sc-help-label">{{ t(item.labelKey) }}</span>
            <span class="scm-sc-help-detail">{{ t(item.detailKey) }}</span>
          </button>
        </div>
      </div>

      <!-- 比较引用选择 -->
      <div v-else-if="picker === 'compare'" class="scm-sc-picker scm-sc-picker--compare">
        <p class="scm-sc-picker-hint">{{ t("scm.searchCompare.comparePickerHint") }}</p>
        <div class="scm-sc-compare-form">
          <UiSelect
            v-model="rightRef"
            class="scm-sc-ref"
            size="sm"
            creatable
            searchable
            :options="refOptions"
            :aria-label="t('scm.searchCompare.rightRef')"
          />
          <span class="scm-sc-compare-with">{{ t("scm.searchCompare.compareWith") }}</span>
          <UiSelect
            v-model="leftRef"
            class="scm-sc-ref"
            size="sm"
            creatable
            searchable
            :options="refOptions"
            :aria-label="t('scm.searchCompare.leftRef')"
          />
        </div>
        <div class="scm-sc-compare-actions">
          <button
            type="button"
            class="scm-sc-welcome-btn"
            :disabled="!leftRef || !rightRef || leftRef === rightRef"
            @click="executeCompare"
          >
            {{ t("scm.searchCompare.compareRefs") }}
          </button>
          <button type="button" class="scm-sc-link-btn" @click="cancelPicker">
            {{ t("common.cancel") }}
          </button>
        </div>
      </div>

      <!-- 结果树 -->
      <div v-if="nodes.length && picker === 'none'" class="scm-sc-tree">
        <template v-for="node in nodes" :key="node.id">
          <!-- 比较节点 -->
          <div v-if="node.kind === 'compare'" class="scm-sc-node">
            <div class="scm-sc-row scm-sc-row--parent" @click="toggleExpanded(node)">
              <Codicon
                :name="node.expanded ? 'chevron-down' : 'chevron-right'"
                class="scm-sc-twistie"
                :size="16"
              />
              <Codicon name="git-compare" class="scm-sc-icon" :size="16" />
              <span class="scm-sc-label">{{ compareLabel(node) }}</span>
              <span v-if="repoName" class="scm-sc-desc">{{ repoName }}</span>
              <span class="scm-sc-row-actions" @click.stop>
                <UiButton
                  variant="ghost"
                  size="icon"
                  :tip="t('scm.searchCompare.swap')"
                  @click="swapCompare(node)"
                >
                  <Codicon name="arrow-swap" />
                </UiButton>
                <UiButton
                  variant="ghost"
                  size="icon"
                  :tip="t('scm.inspect.refresh')"
                  @click="refreshCompare(node)"
                >
                  <Codicon name="refresh" />
                </UiButton>
                <UiButton variant="ghost" size="icon" :tip="t('scm.searchCompare.dismiss')" @click="dismiss(node.id)">
                  <Codicon name="close" />
                </UiButton>
              </span>
            </div>
            <p v-if="node.error" class="scm-sc-child scm-sc-error">{{ node.error }}</p>
            <template v-else-if="node.expanded">
              <div class="scm-sc-row scm-sc-child" @click="toggleBehind(node)">
                <Codicon
                  v-if="node.behind > 0"
                  :name="node.behindExpanded ? 'chevron-down' : 'chevron-right'"
                  class="scm-sc-twistie"
                  :size="16"
                />
                <span v-else class="scm-sc-twistie-spacer" />
                <span class="scm-sc-label">{{ behindLabel(node.behind) }}</span>
              </div>
              <ScmHistoryCommits
                v-if="node.behindExpanded"
                class="scm-sc-commits"
                :workspace-id="workspaceId"
                :commits="node.behindRows"
                :branch="repository?.branch"
                :upstream="repository?.upstream"
                @open="emit('open', $event)"
                @open-changes="emit('openChanges', $event)"
              />
              <div class="scm-sc-row scm-sc-child" @click="toggleAhead(node)">
                <Codicon
                  v-if="node.ahead > 0"
                  :name="node.aheadExpanded ? 'chevron-down' : 'chevron-right'"
                  class="scm-sc-twistie"
                  :size="16"
                />
                <span v-else class="scm-sc-twistie-spacer" />
                <span class="scm-sc-label">{{ aheadLabel(node.ahead) }}</span>
              </div>
              <ScmHistoryCommits
                v-if="node.aheadExpanded"
                class="scm-sc-commits"
                :workspace-id="workspaceId"
                :commits="node.aheadRows"
                :branch="repository?.branch"
                :upstream="repository?.upstream"
                @open="emit('open', $event)"
                @open-changes="emit('openChanges', $event)"
              />
              <div class="scm-sc-row scm-sc-child scm-sc-row--static">
                <span class="scm-sc-twistie-spacer" />
                <span class="scm-sc-label">{{ filesLabel(node.files) }}</span>
              </div>
            </template>
          </div>

          <!-- 搜索节点 -->
          <div v-else class="scm-sc-node">
            <div class="scm-sc-row scm-sc-row--parent" @click="toggleExpanded(node)">
              <Codicon
                v-if="node.rows.length"
                :name="node.expanded ? 'chevron-down' : 'chevron-right'"
                class="scm-sc-twistie"
                :size="16"
              />
              <span v-else class="scm-sc-twistie-spacer" />
              <Codicon name="search" class="scm-sc-icon" :size="16" />
              <span class="scm-sc-label">{{ searchLabel(node) }}</span>
              <span v-if="repoName" class="scm-sc-desc">{{ repoName }}</span>
              <span class="scm-sc-row-actions" @click.stop>
                <UiButton variant="ghost" size="icon" :tip="t('scm.searchCompare.dismiss')" @click="dismiss(node.id)">
                  <Codicon name="close" />
                </UiButton>
              </span>
            </div>
            <p v-if="node.error" class="scm-sc-child scm-sc-error">{{ node.error }}</p>
            <ScmHistoryCommits
              v-else-if="node.expanded && node.rows.length"
              class="scm-sc-commits"
              :workspace-id="workspaceId"
              :commits="node.rows"
              :branch="repository?.branch"
              :upstream="repository?.upstream"
              @open="emit('open', $event)"
              @open-changes="emit('openChanges', $event)"
            />
          </div>
        </template>
      </div>
    </div>
  </UiPane>
</template>
