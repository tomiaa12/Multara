<script setup lang="ts">
import { computed, ref } from "vue";
import type { OutlineItem } from "../../editor/outline";
import { t } from "../../i18n";

const KIND_ORDER: Record<OutlineItem["kind"], number> = {
  class: 0,
  type: 1,
  function: 2,
  heading: 3,
  property: 4,
};

const KIND_ICON: Record<OutlineItem["kind"], string> = {
  class: "symbol-class",
  type: "symbol-interface",
  function: "symbol-method",
  heading: "symbol-misc",
  property: "symbol-property",
};

function kindLabel(kind: OutlineItem["kind"]) {
  return t(`outline.kind.${kind}`);
}

const props = withDefaults(
  defineProps<{
    items: OutlineItem[];
    activeLine: number;
    followCursor?: boolean;
  }>(),
  { followCursor: true },
);

const emit = defineEmits<{
  reveal: [line: number];
}>();

const query = ref("");
const sort = ref<"position" | "name" | "type">("position");
const hiddenKinds = ref(new Set<OutlineItem["kind"]>());

const availableKinds = computed(() => {
  const kinds = new Set(props.items.map((item) => item.kind));
  return (["class", "type", "function", "heading", "property"] as Array<OutlineItem["kind"]>).filter((kind) => kinds.has(kind));
});

const visible = computed(() => {
  const needle = query.value.trim().toLowerCase();
  const filtered = props.items.filter((item) => {
    if (hiddenKinds.value.has(item.kind)) return false;
    return !needle || item.label.toLowerCase().includes(needle);
  });
  const sorted = [...filtered];
  if (sort.value === "name") sorted.sort((a, b) => a.label.localeCompare(b.label, "zh-CN"));
  if (sort.value === "type") {
    sorted.sort((a, b) => KIND_ORDER[a.kind] - KIND_ORDER[b.kind] || a.label.localeCompare(b.label, "zh-CN"));
  }
  return sorted;
});

function toggleKind(kind: OutlineItem["kind"]) {
  const next = new Set(hiddenKinds.value);
  if (next.has(kind)) next.delete(kind);
  else next.add(kind);
  hiddenKinds.value = next;
}

function collapseAll() {
  query.value = "";
}

defineExpose({ collapseAll, clearFilter: () => { query.value = ""; } });
</script>

<template>
  <div class="editor-outline">
    <div class="editor-outline-toolbar">
      <input
        v-model="query"
        class="editor-outline-filter"
        type="search"
        :placeholder="t('outline.filterPlaceholder')"
        spellcheck="false"
      />
      <UiTooltip :content="t('outline.sortTitle')">
        <select v-model="sort" class="editor-outline-sort" :aria-label="t('outline.sortAria')">
          <option value="position">{{ t("outline.sort.position") }}</option>
          <option value="name">{{ t("outline.sort.name") }}</option>
          <option value="type">{{ t("outline.sort.type") }}</option>
        </select>
      </UiTooltip>
    </div>
    <div v-if="availableKinds.length > 1" class="editor-outline-kinds">
      <template v-for="kind in availableKinds" :key="kind">
        <UiTooltip :content="kindLabel(kind)">
          <button
            type="button"
            class="editor-outline-kind"
            :class="{ 'is-off': hiddenKinds.has(kind) }"
            @click="toggleKind(kind)"
          >
            {{ kindLabel(kind) }}
          </button>
        </UiTooltip>
      </template>
    </div>
    <div class="editor-outline-list vs-scroll">
      <div v-if="!items.length" class="tree-empty">{{ t("outline.emptyFile") }}</div>
      <div v-else-if="!visible.length" class="tree-empty">{{ t("outline.noMatch") }}</div>
      <template v-for="item in visible" :key="`${item.line}-${item.label}`">
        <UiTooltip :content="`${item.label}:${item.line}`">
          <button
            type="button"
            class="editor-outline-item"
            :class="{ 'is-active': followCursor && item.line === activeLine }"
            @click="emit('reveal', item.line)"
          >
            <Codicon :name="KIND_ICON[item.kind]" :size="14" />
            <span>{{ item.label }}</span>
          </button>
        </UiTooltip>
      </template>
    </div>
  </div>
</template>
