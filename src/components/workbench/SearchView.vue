<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import type { Workspace } from "../../domain/types";
import { t } from "../../i18n";
import FileTypeIcon from "../FileTypeIcon.vue";
import * as backend from "../../services/backend";
import { previewSearchFiles, setPreviewFileContent } from "./preview-workspace";
import {
  emptySearchOptions,
  fileIncluded,
  replaceInDocument,
  searchDocument,
  type SearchOptions,
  type TextSearchMatch,
} from "./workspace-search";

const props = defineProps<{
  workspaces: Workspace[];
  preview: boolean;
  active: boolean;
  applyReplace: (workspaceId: string, path: string, content: string) => Promise<boolean>;
}>();

const emit = defineEmits<{
  open: [workspaceId: string, path: string, line: number];
}>();

const options = ref<SearchOptions>(emptySearchOptions());
const scopeWorkspaceId = ref("");
const replacement = ref("");
const showReplace = ref(false);
const showDetails = ref(false);
const searching = ref(false);
const error = ref("");
const truncated = ref(false);
const status = ref("");
const input = ref<HTMLInputElement | null>(null);
const matches = shallowRef<Array<TextSearchMatch & { workspaceId: string; workspaceName: string }>>([]);
const collapsed = ref(new Set<string>());
const selectedKey = ref("");
const viewAsTree = ref(true);
const TREE_INDENT = 12;
const SEARCH_DEBOUNCE_MS = 250;
/** 同一个键的新搜索会让后端放弃上一次还在遍历的搜索。 */
const SEARCH_CANCEL_KEY = "search-view";
/** 这几项是边打字边改的输入框，防抖后再搜；勾选类选项改了立即搜。 */
const TYPED_OPTIONS = ["query", "include", "exclude"] as const;
let request = 0;
let searchTimer = 0;

const groups = computed(() => {
  const byFile = new Map<string, {
    key: string;
    workspaceId: string;
    workspaceName: string;
    path: string;
    matches: Array<TextSearchMatch & { workspaceId: string }>;
  }>();
  for (const match of matches.value) {
    const key = `${match.workspaceId}\n${match.path}`;
    const group = byFile.get(key) ?? {
      key,
      workspaceId: match.workspaceId,
      workspaceName: match.workspaceName,
      path: match.path,
      matches: [],
    };
    group.matches.push(match);
    byFile.set(key, group);
  }
  const files = [...byFile.values()].sort((a, b) => a.path.localeCompare(b.path));
  const byWorkspace = new Map<string, { id: string; name: string; matchCount: number; files: typeof files }>();
  for (const file of files) {
    const folder = byWorkspace.get(file.workspaceId) ?? {
      id: file.workspaceId,
      name: file.workspaceName,
      matchCount: 0,
      files: [],
    };
    folder.files.push(file);
    folder.matchCount += file.matches.length;
    byWorkspace.set(file.workspaceId, folder);
  }
  const folders = [...byWorkspace.values()];
  if (folders.length < 2) return [{ id: "", name: "", matchCount: 0, files }];
  return folders;
});

const summary = computed(() => {
  const fileCount = groups.value.reduce((sum, folder) => sum + folder.files.length, 0);
  if (!options.value.query || searching.value) return "";
  if (!matches.value.length) return error.value ? "" : t("search.noResults");
  const key = truncated.value ? "search.summaryTruncated" : "search.summary";
  return t(key, { matches: matches.value.length, files: fileCount });
});

watch(() => props.active, (active) => {
  // 侧栏用 v-show 切换，pre flush 时节点仍是 display:none，这时 focus 不会生效。
  if (active) focusSearch();
});

watch(() => ({ ...options.value }), (next, prev) => {
  if (!TYPED_OPTIONS.some((key) => next[key] !== prev[key])) {
    void runSearch();
    return;
  }
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => void runSearch(), SEARCH_DEBOUNCE_MS);
});

onBeforeUnmount(() => window.clearTimeout(searchTimer));

function fileKey(workspaceId: string, path: string) {
  return `${workspaceId}\n${path}`;
}

function openResult(key: string, workspaceId: string, path: string, line: number) {
  selectedKey.value = key;
  emit("open", workspaceId, path, line);
}

function toggle(key: string) {
  const next = new Set(collapsed.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  collapsed.value = next;
}

function fileName(path: string) {
  return path.split("/").pop() || path;
}

function fileDir(path: string) {
  const index = path.lastIndexOf("/");
  return index < 0 ? "" : path.slice(0, index);
}

type ResultFile = (typeof groups.value)[number]["files"][number];
type ResultRow =
  | { kind: "dir"; key: string; name: string; depth: number; count: number }
  | { kind: "file"; file: ResultFile; depth: number };

function resultRows(files: ResultFile[], prefix = "", depth = 0, scope = ""): ResultRow[] {
  if (!viewAsTree.value) return files.map((file) => ({ kind: "file", file, depth: 0 }));
  const dirs = new Map<string, ResultFile[]>();
  const here: ResultFile[] = [];
  for (const file of files) {
    const rest = prefix ? file.path.slice(prefix.length + 1) : file.path;
    const slash = rest.indexOf("/");
    if (slash < 0) here.push(file);
    else {
      const name = rest.slice(0, slash);
      if (!dirs.has(name)) dirs.set(name, []);
      dirs.get(name)?.push(file);
    }
  }
  const rows: ResultRow[] = [];
  for (const name of [...dirs.keys()].sort((a, b) => a.localeCompare(b))) {
    const nested = dirs.get(name) ?? [];
    const pathKey = prefix ? `${prefix}/${name}` : name;
    const key = scope ? `${scope}\n${pathKey}` : pathKey;
    const count = nested.reduce((sum, file) => sum + file.matches.length, 0);
    rows.push({ kind: "dir", key, name, depth, count });
    if (!collapsed.value.has(key)) rows.push(...resultRows(nested, pathKey, depth + 1, scope));
  }
  for (const file of here) rows.push({ kind: "file", file, depth });
  return rows;
}

const folderRows = computed(() => new Map(
  groups.value.map((folder) => [folder.id, resultRows(folder.files, "", 0, folder.id)]),
));

function rowPad(depth: number) {
  return `${8 + depth * TREE_INDENT}px`;
}

function allCollapsed() {
  for (const folder of groups.value) {
    for (const file of folder.files) {
      if (!collapsed.value.has(file.key)) return false;
    }
  }
  return matches.value.length > 0;
}

function toggleAll() {
  if (allCollapsed()) {
    collapsed.value = new Set();
    return;
  }
  const next = new Set<string>();
  for (const folder of groups.value) {
    if (folder.id) next.add(folder.id);
    for (const file of folder.files) next.add(file.key);
    for (const row of resultRows(folder.files, "", 0, folder.id)) {
      if (row.kind === "dir") next.add(row.key);
    }
  }
  collapsed.value = next;
}

function clearSearch() {
  const hadPattern = Boolean(options.value.query || replacement.value);
  options.value = { ...emptySearchOptions(), useExcludeSettings: options.value.useExcludeSettings };
  replacement.value = "";
  matches.value = [];
  truncated.value = false;
  status.value = "";
  error.value = "";
  if (!hadPattern) {
    scopeWorkspaceId.value = "";
  }
  request += 1;
  searching.value = false;
}

function cancelSearch() {
  window.clearTimeout(searchTimer);
  request += 1;
  searching.value = false;
}

function parts(match: TextSearchMatch) {
  const start = Math.max(0, match.previewColumn - 1);
  const end = Math.min(match.preview.length, start + match.length);
  return {
    before: match.preview.slice(0, start),
    hit: match.preview.slice(start, end),
    after: match.preview.slice(end),
  };
}

async function runSearch() {
  window.clearTimeout(searchTimer);
  const current = ++request;
  const query = options.value;
  error.value = "";
  status.value = "";
  if (!query.query.trim()) {
    matches.value = [];
    truncated.value = false;
    searching.value = false;
    return;
  }
  searching.value = true;
  try {
    const next: typeof matches.value = [];
    let hitLimit = false;
    let message = "";
    const workspaces = scopeWorkspaceId.value
      ? props.workspaces.filter((workspace) => workspace.id === scopeWorkspaceId.value)
      : props.workspaces;
    for (const workspace of workspaces) {
      if (props.preview) {
        for (const file of previewSearchFiles(workspace.id)) {
          const found = searchDocument(file.path, file.content, query, 2000 - next.length);
          if (found.error) message = found.error;
          hitLimit = hitLimit || found.truncated;
          for (const match of found.matches) {
            next.push({ ...match, workspaceId: workspace.id, workspaceName: workspace.name });
          }
          if (next.length >= 2000) break;
        }
      } else {
        const found = await backend.searchWorkspace(workspace.id, query, SEARCH_CANCEL_KEY);
        if (current !== request) return;
        if (found.error) message = found.error;
        hitLimit = hitLimit || found.truncated;
        for (const match of found.matches) {
          next.push({ ...match, workspaceId: workspace.id, workspaceName: workspace.name });
        }
      }
      if (next.length >= 2000) break;
    }
    if (current !== request) return;
    matches.value = next;
    truncated.value = hitLimit;
    error.value = message;
  } catch (caught) {
    if (current !== request) return;
    matches.value = [];
    error.value = caught instanceof Error ? caught.message : t("search.failed");
  } finally {
    if (current === request) searching.value = false;
  }
}

async function replaceAll() {
  const files = new Map<string, { workspaceId: string; path: string }>();
  for (const match of matches.value) files.set(`${match.workspaceId}\n${match.path}`, match);
  let replaced = 0;
  let skipped = 0;
  for (const file of files.values()) {
    const content = await readFile(file.workspaceId, file.path);
    if (content == null || !fileIncluded(file.path, options.value)) {
      skipped += 1;
      continue;
    }
    let next = content;
    try {
      next = replaceInDocument(content, options.value, replacement.value);
    } catch (caught) {
      error.value = caught instanceof Error ? caught.message : t("search.replaceFailed");
      return;
    }
    if (next === content) continue;
    const applied = await props.applyReplace(file.workspaceId, file.path, next);
    if (!applied) skipped += 1;
    else {
      if (props.preview) setPreviewFileContent(file.path, next);
      replaced += 1;
    }
  }
  status.value = skipped
    ? t("search.replaceDoneSkipped", { replaced, skipped })
    : t("search.replaceDone", { count: replaced });
  await runSearch();
}

function focusSearch() {
  nextTick(() => {
    input.value?.focus();
    input.value?.select();
  });
}

/** 打开搜索并把范围限制到某个项目里的文件夹。folder 为空表示整个项目。 */
function findIn(workspaceId: string, include: string) {
  scopeWorkspaceId.value = workspaceId;
  options.value = { ...options.value, include };
  showDetails.value = Boolean(include);
  focusSearch();
}

defineExpose({ findIn, focusSearch });

async function readFile(workspaceId: string, path: string) {
  if (props.preview) {
    return previewSearchFiles(workspaceId).find((file) => file.path === path)?.content ?? "";
  }
  try {
    return (await backend.readWorkspaceFile(workspaceId, path)).content;
  } catch (caught) {
    error.value = caught instanceof Error ? caught.message : t("search.readFailed");
    return null;
  }
}
</script>

<template>
  <section class="search-view">
    <div class="search-toolbar">
      <button type="button" :title="searching ? t('search.cancel') : t('search.refresh')" :disabled="!searching && !options.query" @click="searching ? cancelSearch() : runSearch()">
        <Codicon :name="searching ? 'search-stop' : 'refresh'" :size="16" />
      </button>
      <button type="button" :title="t('search.clearResults')" :disabled="!options.query && !replacement && !matches.length" @click="clearSearch">
        <Codicon name="clear-all" :size="16" />
      </button>
      <button
        type="button"
        :title="viewAsTree ? t('search.viewAsList') : t('search.viewAsTree')"
        @click="viewAsTree = !viewAsTree"
      >
        <Codicon :name="viewAsTree ? 'list-tree' : 'list-flat'" :size="16" />
      </button>
      <button type="button" :title="allCollapsed() ? t('search.expandAll') : t('search.collapseAll')" :disabled="!matches.length" @click="toggleAll">
        <Codicon :name="allCollapsed() ? 'expand-all' : 'collapse-all'" :size="16" />
      </button>
    </div>
    <div class="search-widget">
      <div class="search-row">
        <button
          type="button"
          class="search-toggle"
          :title="showReplace ? t('search.hideReplace') : t('search.replaceToggle')"
          :aria-expanded="showReplace"
          @click="showReplace = !showReplace"
        >
          <Codicon :name="showReplace ? 'chevron-down' : 'chevron-right'" :size="14" />
        </button>
        <label class="search-input">
          <input
            ref="input"
            v-model="options.query"
            :placeholder="t('search.placeholder')"
            :aria-label="t('search.placeholder')"
            spellcheck="false"
          />
          <span class="search-input-actions">
            <button type="button" :title="t('search.caseSensitive')" :class="{ active: options.caseSensitive }" @click="options.caseSensitive = !options.caseSensitive">Aa</button>
            <button type="button" :title="t('search.wholeWord')" :class="{ active: options.wholeWord }" @click="options.wholeWord = !options.wholeWord">ab</button>
            <button type="button" :title="t('search.regex')" :class="{ active: options.regex }" @click="options.regex = !options.regex">.*</button>
          </span>
        </label>
      </div>
      <div v-if="showReplace" class="search-row">
        <span class="search-toggle" />
        <label class="search-input">
          <input v-model="replacement" :placeholder="t('search.replacePlaceholder')" :aria-label="t('search.replacePlaceholder')" spellcheck="false" />
          <span class="search-input-actions">
            <button type="button" :title="t('search.preserveCase')" :class="{ active: options.preserveCase }" @click="options.preserveCase = !options.preserveCase">AB</button>
          </span>
        </label>
        <button type="button" class="search-outside" :title="t('search.replaceAll')" :disabled="!matches.length" @click="replaceAll">
          <Codicon name="replace-all" :size="16" />
        </button>
      </div>
      <div class="search-details-bar">
        <span>{{ showDetails ? "" : t('search.includeFiles') }}</span>
        <button type="button" :title="t('search.details')" :aria-expanded="showDetails" @click="showDetails = !showDetails">
          <Codicon name="ellipsis" :size="16" />
        </button>
      </div>
      <div v-if="showDetails" class="search-details">
        <label class="search-file-pattern">
          <span>{{ t('search.includeFiles') }}</span>
          <input v-model="options.include" :placeholder="t('search.includePlaceholder')" :aria-label="t('search.includeFiles')" spellcheck="false" />
        </label>
        <label class="search-file-pattern">
          <span>{{ t('search.excludeFiles') }}</span>
          <span class="search-pattern-input">
            <input v-model="options.exclude" :placeholder="t('search.excludePlaceholder')" :aria-label="t('search.excludeFiles')" spellcheck="false" />
            <button
              type="button"
              :title="t('search.useExclude')"
              :class="{ active: options.useExcludeSettings }"
              @click="options.useExcludeSettings = !options.useExcludeSettings"
            >
              <Codicon name="book" :size="14" />
            </button>
          </span>
        </label>
      </div>
    </div>
    <p v-if="searching" class="search-status">{{ t('search.searching') }}</p>
    <p v-else-if="error" class="search-status is-error">{{ error }}</p>
    <p v-else-if="summary || status" class="search-status">{{ status || summary }}</p>
    <div class="search-results vs-scroll">
      <section v-for="folder in groups" :key="folder.id || 'single'" class="search-folder" :class="{ 'is-grouped': folder.name }">
        <button
          v-if="folder.name"
          type="button"
          class="search-folder-name"
          @click="toggle(folder.id)"
        >
          <Codicon :name="collapsed.has(folder.id) ? 'chevron-right' : 'chevron-down'" :size="14" />
          <FileTypeIcon :name="folder.name" directory :open="!collapsed.has(folder.id)" :size="16" />
          <span>{{ folder.name }}</span>
          <em>{{ folder.matchCount }}</em>
        </button>
        <template v-if="!collapsed.has(folder.id)">
          <template v-for="row in folderRows.get(folder.id)" :key="row.kind === 'dir' ? row.key : row.file.key">
            <button
              v-if="row.kind === 'dir'"
              type="button"
              class="search-file-name"
              :style="{ paddingLeft: rowPad(row.depth) }"
              @click="toggle(row.key)"
            >
              <Codicon :name="collapsed.has(row.key) ? 'chevron-right' : 'chevron-down'" :size="14" />
              <FileTypeIcon :name="row.name" directory :open="!collapsed.has(row.key)" :size="16" />
              <span>{{ row.name }}</span>
              <em>{{ row.count }}</em>
            </button>
            <div v-else class="search-file">
              <button
                type="button"
                class="search-file-name"
                :class="{ selected: selectedKey === row.file.key }"
                :style="{ paddingLeft: rowPad(row.depth) }"
                @click="openResult(row.file.key, row.file.workspaceId, row.file.path, row.file.matches[0]?.line ?? 1)"
              >
                <i @click.stop="toggle(row.file.key)">
                  <Codicon :name="collapsed.has(row.file.key) ? 'chevron-right' : 'chevron-down'" :size="14" />
                </i>
                <FileTypeIcon :name="fileName(row.file.path)" :size="16" />
                <span>{{ fileName(row.file.path) }}</span>
                <small v-if="!viewAsTree && fileDir(row.file.path)">{{ fileDir(row.file.path) }}</small>
                <em>{{ row.file.matches.length }}</em>
              </button>
              <button
                v-for="(match, index) in collapsed.has(row.file.key) ? [] : row.file.matches"
                :key="`${match.line}:${match.column}:${index}`"
                type="button"
                class="search-match"
                :style="{ paddingLeft: rowPad(row.depth + 2) }"
                :class="{ selected: selectedKey === `${row.file.key}:${match.line}:${match.column}:${index}` }"
                @click="openResult(`${row.file.key}:${match.line}:${match.column}:${index}`, row.file.workspaceId, row.file.path, match.line)"
              >
                <span class="search-match-line">{{ match.line }}</span>
                <span class="search-match-text">
                  <span>{{ parts(match).before }}</span><mark>{{ parts(match).hit }}</mark><span>{{ parts(match).after }}</span>
                </span>
              </button>
            </div>
          </template>
        </template>
      </section>
    </div>
  </section>
</template>
