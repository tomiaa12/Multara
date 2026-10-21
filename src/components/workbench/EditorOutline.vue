<script setup lang="ts">
import { computed, ref } from "vue";
import type { OutlineItem } from "../../editor/outline";

const KIND_ORDER: Record<OutlineItem["kind"], number> = {
  class: 0,
  type: 1,
  function: 2,
  heading: 3,
  property: 4,
};

const KIND_LABEL: Record<OutlineItem["kind"], string> = {
  class: "类",
  type: "类型",
  function: "函数",
  heading: "标题",
  property: "属性",
};

const KIND_ICON: Record<OutlineItem["kind"], string> = {
  class: "symbol-class",
  type: "symbol-interface",
  function: "symbol-method",
  heading: "symbol-misc",
  property: "symbol-property",
};

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
  return (Object.keys(KIND_LABEL) as Array<OutlineItem["kind"]>).filter((kind) => kinds.has(kind));
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
        placeholder="筛选符号"
        spellcheck="false"
      />
      <select v-model="sort" class="editor-outline-sort" title="排序" aria-label="大纲排序">
        <option value="position">位置</option>
        <option value="name">名称</option>
        <option value="type">类型</option>
      </select>
    </div>
    <div v-if="availableKinds.length > 1" class="editor-outline-kinds">
      <button
        v-for="kind in availableKinds"
        :key="kind"
        type="button"
        class="editor-outline-kind"
        :class="{ 'is-off': hiddenKinds.has(kind) }"
        :title="KIND_LABEL[kind]"
        @click="toggleKind(kind)"
      >
        {{ KIND_LABEL[kind] }}
      </button>
    </div>
    <div class="editor-outline-list vs-scroll">
      <div v-if="!items.length" class="tree-empty">当前文件没有可识别的符号。</div>
      <div v-else-if="!visible.length" class="tree-empty">没有匹配的符号。</div>
      <button
        v-for="item in visible"
        :key="`${item.line}-${item.label}`"
        type="button"
        class="editor-outline-item"
        :class="{ 'is-active': followCursor && item.line === activeLine }"
        :title="`${item.label}:${item.line}`"
        @click="emit('reveal', item.line)"
      >
        <Codicon :name="KIND_ICON[item.kind]" :size="14" />
        <span>{{ item.label }}</span>
      </button>
    </div>
  </div>
</template>
