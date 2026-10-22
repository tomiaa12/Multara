<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { ISearchOptions, SearchAddon } from "@xterm/addon-search";
import type { IDisposable } from "@xterm/xterm";
import { t } from "../../i18n";
import { isLightTheme } from "./terminal-theme";

const props = defineProps<{
  search: SearchAddon;
  initial: string;
}>();

const emit = defineEmits<{
  close: [];
}>();

const input = ref<HTMLInputElement | null>(null);
const query = ref(props.initial);
const caseSensitive = ref(false);
const wholeWord = ref(false);
const regex = ref(false);
const results = ref<{ index: number; count: number } | null>(null);
let resultsListener: IDisposable | undefined;

const summary = computed(() => {
  if (!query.value) return "";
  const found = results.value;
  if (!found || found.count === 0) return t("terminal.find.noResults");
  return found.index < 0
    ? t("terminal.find.countOnly", { count: found.count })
    : t("terminal.find.position", { index: found.index + 1, count: found.count });
});

const summaryEmpty = computed(() => summary.value === t("terminal.find.noResults"));

function options(incremental: boolean): ISearchOptions {
  const light = isLightTheme();
  return {
    caseSensitive: caseSensitive.value,
    wholeWord: wholeWord.value,
    regex: regex.value,
    incremental,
    decorations: {
      matchBackground: light ? "#a8ac94" : "#515c6a",
      matchBorder: light ? "#a8ac94" : "#515c6a",
      matchOverviewRuler: "#d18616",
      activeMatchBackground: light ? "#f5c400" : "#a8ac94",
      activeMatchBorder: light ? "#f5c400" : "#a8ac94",
      activeMatchColorOverviewRuler: "#d18616",
    },
  };
}

function findNext(incremental = false) {
  if (!query.value) {
    props.search.clearDecorations();
    results.value = null;
    return;
  }
  try {
    props.search.findNext(query.value, options(incremental));
  } catch {
    results.value = { index: -1, count: 0 };
  }
}

function findPrevious() {
  if (!query.value) return;
  try {
    props.search.findPrevious(query.value, options(false));
  } catch {
    results.value = { index: -1, count: 0 };
  }
}

function close() {
  props.search.clearDecorations();
  emit("close");
}

function onKey(event: KeyboardEvent) {
  if (event.altKey && !event.ctrlKey && !event.metaKey) {
    const flag = { KeyC: "caseSensitive", KeyW: "wholeWord", KeyR: "regex" }[event.code] as
      | "caseSensitive"
      | "wholeWord"
      | "regex"
      | undefined;
    if (flag) {
      event.preventDefault();
      toggle(flag);
      return;
    }
  }
  if (event.key === "Escape") {
    event.preventDefault();
    close();
  } else if (event.key === "Enter") {
    event.preventDefault();
    if (event.shiftKey) findPrevious();
    else findNext();
  } else if (event.key === "F3") {
    event.preventDefault();
    if (event.shiftKey) findPrevious();
    else findNext();
  }
}

function toggle(name: "caseSensitive" | "wholeWord" | "regex") {
  const flag = { caseSensitive, wholeWord, regex }[name];
  flag.value = !flag.value;
  findNext(true);
  input.value?.focus();
}

watch(query, () => findNext(true));

onMounted(async () => {
  resultsListener = props.search.onDidChangeResults((event) => {
    results.value = { index: event.resultIndex, count: event.resultCount };
  });
  await nextTick();
  input.value?.focus();
  input.value?.select();
  if (query.value) findNext(true);
});

onBeforeUnmount(() => {
  resultsListener?.dispose();
});

defineExpose({
  focus(value?: string) {
    if (value) query.value = value;
    input.value?.focus();
    input.value?.select();
  },
});
</script>

<template>
  <div class="terminal-find" role="search" @keydown="onKey">
    <div class="terminal-find-field">
      <input ref="input" v-model="query" type="text" :placeholder="t('terminal.find.placeholder')" :aria-label="t('terminal.find.aria')" spellcheck="false" />
      <button type="button" :class="{ on: caseSensitive }" :title="t('terminal.find.caseSensitive')" @click="toggle('caseSensitive')">
        <Codicon name="case-sensitive" :size="14" />
      </button>
      <button type="button" :class="{ on: wholeWord }" :title="t('terminal.find.wholeWord')" @click="toggle('wholeWord')">
        <Codicon name="whole-word" :size="14" />
      </button>
      <button type="button" :class="{ on: regex }" :title="t('terminal.find.regex')" @click="toggle('regex')">
        <Codicon name="regex" :size="14" />
      </button>
    </div>
    <span class="terminal-find-count" :class="{ empty: summaryEmpty }">{{ summary }}</span>
    <button type="button" :title="t('terminal.find.previous')" :disabled="!query" @click="findPrevious">
      <Codicon name="arrow-up" :size="14" />
    </button>
    <button type="button" :title="t('terminal.find.next')" :disabled="!query" @click="findNext()">
      <Codicon name="arrow-down" :size="14" />
    </button>
    <button type="button" :title="t('terminal.find.close')" @click="close">
      <Codicon name="close" :size="14" />
    </button>
  </div>
</template>
