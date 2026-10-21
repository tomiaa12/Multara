<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { Search, Store, X } from "@lucide/vue";
import type { CatalogItem, ResourceKind, ResourceSource } from "../domain/types";
import CliSourceIcon from "../components/CliSourceIcon.vue";
import { cliSkillsDisplay } from "../domain/cli-catalog";
import * as backend from "../services/backend";

const CATALOG_SKELETON_ROWS = 6;

const props = defineProps<{
  kind: ResourceKind;
}>();

const emit = defineEmits<{
  close: [];
  install: [item: CatalogItem, owner: string];
}>();

const query = ref("");
const owner = ref("multara");
const sources = ref<ResourceSource[]>([]);
const items = ref<CatalogItem[]>([]);
const error = ref("");
/** 首帧即 loading，先画出弹窗与骨架屏 */
const loading = ref(true);
let request = 0;
let debounce = 0;
const ownerOptions = computed(() =>
  sources.value.map((item) => {
    let meta: string | undefined;
    if (item.scope === "workspace") meta = "工作区";
    else if (props.kind === "skill") meta = cliSkillsDisplay(item.owner);
    return { value: item.owner, label: item.label, meta };
  }),
);

async function load() {
  const id = ++request;
  loading.value = true;
  error.value = "";
  try {
    const rows = await backend.searchCatalog(props.kind, query.value);
    if (id !== request) return;
    items.value = rows;
  } catch (cause) {
    if (id !== request) return;
    error.value = cause instanceof Error ? cause.message : String(cause);
    items.value = [];
  } finally {
    if (id === request) loading.value = false;
  }
}

watch(query, () => {
  window.clearTimeout(debounce);
  debounce = window.setTimeout(() => void load(), 280);
});

onMounted(() => {
  void backend.listResourceSources(props.kind).then((rows) => {
    sources.value = rows;
    if (!rows.some((item) => item.owner === owner.value) && rows[0]) owner.value = rows[0].owner;
  });
  // 等弹窗先绘制一帧，再发起商店请求
  void nextTick(() => void load());
});
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <div class="modal domain-modal catalog-dialog">
      <header class="modal-heading">
        <h2>{{ kind === "skill" ? "浏览 Skills" : "浏览 MCP" }}</h2>
        <UiButton class="icon-button" @click="emit('close')"><X :size="16" /></UiButton>
      </header>
      <label>安装到
        <UiSelect v-model="owner" :options="ownerOptions">
          <template #option-leading="{ option }">
            <CliSourceIcon v-if="option" :owner="String(option.value)" :size="16" />
          </template>
        </UiSelect>
      </label>
      <label class="ds-search collection-search">
        <Search :size="14" />
        <UiInput v-model="query" :placeholder="kind === 'skill' ? '搜索官方 Skills…' : '搜索 MCP Registry…'" />
      </label>
      <p v-if="error" class="form-error">{{ error }}</p>
      <div v-else-if="loading" class="catalog-list" aria-busy="true" aria-label="正在加载商店">
        <article
          v-for="index in CATALOG_SKELETON_ROWS"
          :key="index"
          class="catalog-row catalog-row-skeleton"
        >
          <span>
            <span class="ds-skeleton catalog-skeleton-title" />
            <span class="ds-skeleton catalog-skeleton-desc" />
          </span>
          <span class="ds-skeleton catalog-skeleton-action" />
        </article>
      </div>
      <p v-else-if="!items.length" class="empty-copy">没有匹配的项。</p>
      <div v-else class="catalog-list">
        <article v-for="item in items" :key="item.id" class="catalog-row">
          <span>
            <strong>{{ item.name }}</strong>
            <small>{{ item.description || item.source }}</small>
          </span>
          <UiButton variant="primary" size="small" @click="emit('install', item, owner)">安装</UiButton>
        </article>
      </div>
      <p class="catalog-hint">
        <Store :size="14" />
        {{ kind === "skill" ? "来源：GitHub anthropics/skills，安装会浅克隆到所选目录。" : "来源：MCP Registry，安装写入所选来源的配置文件。" }}
      </p>
    </div>
  </div>
</template>
