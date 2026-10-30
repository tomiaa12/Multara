<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import {
  parseQuickOpenQuery,
  prepareQuickOpenFiles,
  rankPreparedQuickOpenFiles,
  type QuickOpenFile,
} from "./quick-open";
import { t } from "../../i18n";

const props = defineProps<{
  files: QuickOpenFile[];
  loading: boolean;
  initialQuery?: string;
}>();

const emit = defineEmits<{
  close: [];
  open: [file: QuickOpenFile, startLine?: number, endLine?: number];
}>();

const query = ref(props.initialQuery ?? "");
const index = ref(0);
const input = ref<HTMLInputElement | null>(null);
const list = ref<HTMLElement | null>(null);

const QUICK_OPEN_DEBOUNCE_MS = 120;
const QUICK_OPEN_EMPTY_RESULTS = 100;
const filterQuery = ref(query.value);
let filterTimer = 0;
watch(query, (value) => {
  window.clearTimeout(filterTimer);
  filterTimer = window.setTimeout(() => {
    filterQuery.value = value;
  }, QUICK_OPEN_DEBOUNCE_MS);
});
onBeforeUnmount(() => window.clearTimeout(filterTimer));

function flushFilterQuery() {
  window.clearTimeout(filterTimer);
  filterQuery.value = query.value;
}

const candidates = computed(() => prepareQuickOpenFiles(props.files));
const parsed = computed(() => parseQuickOpenQuery(filterQuery.value));
const matches = computed(() => {
  if (!parsed.value.filter) return props.files.slice(0, QUICK_OPEN_EMPTY_RESULTS);
  return rankPreparedQuickOpenFiles(candidates.value, parsed.value.filter);
});

watch(matches, () => {
  index.value = 0;
});

watch(index, () => {
  void nextTick(() => {
    list.value?.querySelector(".active")?.scrollIntoView({ block: "nearest" });
  });
});

function move(delta: number) {
  const count = matches.value.length;
  if (!count) return;
  index.value = (index.value + delta + count) % count;
}

function confirm() {
  // 防抖还没生效时高亮项属于旧列表，按新结果的第一项打开。
  const stale = filterQuery.value !== query.value;
  flushFilterQuery();
  const file = matches.value[stale ? 0 : index.value];
  if (!file) return;
  emit("open", file, parsed.value.startLine, parsed.value.endLine);
}

function onFocusOut(event: FocusEvent) {
  const next = event.relatedTarget;
  const root = event.currentTarget;
  if (next instanceof Node && root instanceof Node && root.contains(next)) return;
  emit("close");
}

void nextTick(() => input.value?.focus());
</script>

<template>
  <div class="quick-pick quick-open" role="dialog" :aria-label="t('workbench.quickOpen.title')" @focusout="onFocusOut">
    <input
      ref="input"
      v-model="query"
      :placeholder="t('workbench.quickOpen.placeholder')"
      :aria-label="t('workbench.quickOpen.placeholder')"
      @keydown.escape.stop="emit('close')"
      @keydown.enter.prevent="confirm"
      @keydown.down.prevent="move(1)"
      @keydown.up.prevent="move(-1)"
    />
    <div ref="list" class="quick-open-list vs-scroll">
      <p v-if="loading">{{ t('workbench.loadingTree') }}</p>
      <button
        v-for="(file, itemIndex) in matches"
        :key="`${file.workspaceId}:${file.path}`"
        type="button"
        :class="{ active: itemIndex === index }"
        @click="emit('open', file, parsed.startLine, parsed.endLine)"
        @mouseenter="index = itemIndex"
      >
        <FileTypeIcon :name="file.label" />
        <span>{{ file.label }}</span>
        <small :title="file.detail">{{ file.detail }}</small>
      </button>
      <p v-if="!loading && !matches.length">{{ t('workbench.quickOpen.noMatch') }}</p>
    </div>
  </div>
</template>
