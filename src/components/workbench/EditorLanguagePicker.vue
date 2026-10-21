<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import {
  AUTO_DETECT_LANGUAGE,
  filterSelectableLanguages,
  languageIconName,
  matchesAutoDetect,
  type EditorLanguageOption,
} from "../../monaco-language";

const props = defineProps<{
  languages: EditorLanguageOption[];
  currentId: string | null;
  hasOverride: boolean;
  anchor: DOMRect;
}>();

const emit = defineEmits<{
  select: [id: string];
  close: [];
}>();

const query = ref("");
const highlight = ref(0);
const panelEl = ref<HTMLElement | null>(null);
const searchEl = ref<HTMLInputElement | null>(null);
const left = ref(8);
const top = ref(8);

const filtered = computed(() => filterSelectableLanguages(props.languages, query.value));
const showAuto = computed(() => matchesAutoDetect(query.value));
const items = computed(() => {
  const rows: EditorLanguageOption[] = [];
  if (showAuto.value) {
    rows.push({ id: AUTO_DETECT_LANGUAGE, label: "自动检测" });
  }
  rows.push(...filtered.value);
  return rows;
});

const style = computed(() => ({
  left: `${left.value}px`,
  top: `${top.value}px`,
}));

function place() {
  const el = panelEl.value;
  if (!el) return;
  const width = el.offsetWidth;
  const height = el.offsetHeight;
  left.value = Math.max(8, Math.min(props.anchor.right - width, window.innerWidth - width - 8));
  top.value = Math.max(8, props.anchor.top - height - 4);
}

function select(id: string) {
  emit("select", id);
}

function move(delta: number) {
  const count = items.value.length;
  if (!count) return;
  highlight.value = (highlight.value + delta + count) % count;
  const active = panelEl.value?.querySelector<HTMLElement>("[data-active='true']");
  active?.scrollIntoView({ block: "nearest" });
}

function onDoc(event: MouseEvent) {
  const target = event.target as Node | null;
  if (panelEl.value?.contains(target)) return;
  if (target instanceof Element && target.closest(".statusbar-item")) return;
  emit("close");
}

function onKey(event: KeyboardEvent) {
  if (event.isComposing) return;
  if (event.key === "Escape") {
    event.preventDefault();
    emit("close");
    return;
  }
  if (event.key === "ArrowDown") {
    event.preventDefault();
    move(1);
    return;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    move(-1);
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    const item = items.value[highlight.value];
    if (item) select(item.id);
  }
}

function resetHighlight() {
  highlight.value = 0;
}

function onSearchBlur() {
  window.setTimeout(() => {
    if (searchEl.value && document.activeElement === searchEl.value) return;
    emit("close");
  }, 0);
}

function focusSearch() {
  searchEl.value?.focus();
}

onMounted(() => {
  resetHighlight();
  nextTick(() => {
    place();
    focusSearch();
    requestAnimationFrame(focusSearch);
  });
  document.addEventListener("mousedown", onDoc);
  document.addEventListener("keydown", onKey);
});

onBeforeUnmount(() => {
  document.removeEventListener("mousedown", onDoc);
  document.removeEventListener("keydown", onKey);
});

watch(() => [props.anchor.right, props.anchor.top], () => nextTick(place));
watch(query, resetHighlight);
</script>

<template>
  <div
    ref="panelEl"
    class="editor-language-picker context-menu"
    :style="style"
    role="listbox"
    aria-label="选择语言模式"
  >
    <input
      ref="searchEl"
      v-model="query"
      class="editor-language-search"
      type="search"
      placeholder="选择语言模式"
      autofocus
      @blur="onSearchBlur"
    />
    <div class="editor-language-list">
      <button
        v-for="(item, index) in items"
        :key="item.id"
        type="button"
        role="option"
        :data-active="index === highlight"
        :aria-selected="index === highlight"
        :class="{ 'is-active': index === highlight }"
        @mouseenter="highlight = index"
        @mousedown.prevent="select(item.id)"
      >
        <FileTypeIcon :name="languageIconName(item)" />
        <span class="context-menu-label">{{ item.label }}</span>
      </button>
    </div>
  </div>
</template>
