<script setup lang="ts">
import { computed, nextTick, onMounted, onBeforeUnmount, reactive, ref, watch } from "vue";
import { Bot, ChevronRight, FolderKanban, Hash, Paperclip, Search, Sparkles } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import { nextPopupZIndex } from "../ui/popup-z-index";
import type { ComposerSuggestCategory } from "./composer-entities";
import {
  matchesCascadeQuery,
  navigateCascade,
  nextCascadeVisibleCount,
  type CascadePane,
} from "./composer-cascade";
import { t } from "../../i18n";

export type ComposerSuggestItem = {
  id: string;
  kind: ComposerSuggestCategory;
  label: string;
  description?: string;
  /** 有头像时按智能体下拉选项渲染（AgentAvatar + ds-select-option） */
  avatar?: string | null;
  leading?: "bot" | "sparkles" | "hash" | "requirement";
};

export type ComposerSuggestGroup = {
  id: string;
  label: string;
  items: ComposerSuggestItem[];
};

export type ComposerSuggestCategoryItem = {
  id: ComposerSuggestCategory | "file";
  label: string;
  groups: ComposerSuggestGroup[];
  action?: "pick-files-or-folders";
};

const props = defineProps<{
  categories: ComposerSuggestCategoryItem[];
  defaultCategory: ComposerSuggestCategory;
  query: string;
  /** 光标或输入区的视口矩形，用于浮层定位（不占输入框文档流） */
  anchor: { left: number; top: number; bottom: number; width?: number } | null;
  ariaLabel?: string;
}>();

const emit = defineEmits<{
  select: [item: ComposerSuggestItem];
  "category-action": [action: "pick-files-or-folders"];
  close: [];
}>();

/** 默认在光标下方；仅下方放不下菜单高度时才翻到上方 */
const POPOVER_GAP = 4;
const VIEWPORT_MARGIN = 12;
const MENU_MAX_WIDTH = 520;
const MENU_MAX_HEIGHT = 360;
const PAGE_SIZE = 5;
/** 下方至少要能放下这么高，否则改向上 */
const MIN_BELOW = 220;

const root = ref<HTMLElement | null>(null);
const listEl = ref<HTMLElement | null>(null);
const categoryIndex = ref(0);
const activeIndex = ref(0);
const activePane = ref<CascadePane>("categories");
const searchQuery = ref(props.query);
const visibleCounts = reactive<Record<string, number>>({});
const placement = ref<"bottom" | "top">("bottom");
const layerZ = nextPopupZIndex();
const popoverStyle = ref<Record<string, string>>({ zIndex: String(layerZ) });

const activeCategory = computed(() => props.categories[categoryIndex.value] ?? props.categories[0]);
const filteredGroups = computed(() =>
  (activeCategory.value?.groups ?? []).map((group) => ({
    ...group,
    items: group.items.filter((item) =>
      matchesCascadeQuery(item.label, item.description, searchQuery.value),
    ),
  })),
);
const visibleGroups = computed(() =>
  filteredGroups.value.map((group) => ({
    ...group,
    items: group.items.slice(0, visibleCounts[`${activeCategory.value?.id}:${group.id}`] ?? PAGE_SIZE),
  })),
);
const navigationEntries = computed(() =>
  visibleGroups.value.flatMap((group) => {
    const entries: Array<{ item?: ComposerSuggestItem; more?: ComposerSuggestGroup }> =
      group.items.map((item) => ({ item }));
    const total = filteredGroups.value.find((item) => item.id === group.id)?.items.length ?? 0;
    if (group.items.length < total) entries.push({ more: group });
    return entries;
  }),
);

function selectCategory(index: number) {
  categoryIndex.value = index;
  activeIndex.value = 0;
}

function activateCategory(index: number) {
  selectCategory(index);
  const action = props.categories[index]?.action;
  if (action) {
    emit("category-action", action);
    return;
  }
  activePane.value = "results";
}

function resetCategory() {
  const index = props.categories.findIndex((item) => item.id === props.defaultCategory);
  selectCategory(Math.max(index, 0));
  activePane.value = "categories";
}

function clamp(value: number, min: number, max: number) {
  if (value < min) return min;
  if (value > max) return max;
  return value;
}

function updatePosition() {
  const anchor = props.anchor;
  if (!anchor) return;
  const spaceBelow = window.innerHeight - anchor.bottom - POPOVER_GAP - VIEWPORT_MARGIN;
  const spaceAbove = anchor.top - POPOVER_GAP - VIEWPORT_MARGIN;
  const menuHeight = Math.min(root.value?.offsetHeight || MENU_MAX_HEIGHT, MENU_MAX_HEIGHT);
  // 优先下方：下方够用，或比上方更宽裕时向下
  const placeBelow = spaceBelow >= Math.min(menuHeight, MIN_BELOW) || spaceBelow >= spaceAbove;
  placement.value = placeBelow ? "bottom" : "top";
  const width = Math.min(MENU_MAX_WIDTH, window.innerWidth - VIEWPORT_MARGIN * 2);
  const left = clamp(anchor.left, VIEWPORT_MARGIN, window.innerWidth - width - VIEWPORT_MARGIN);
  popoverStyle.value = {
    zIndex: String(layerZ),
    left: `${left}px`,
    width: `${width}px`,
    maxHeight: `${MENU_MAX_HEIGHT}px`,
    ...(placement.value === "top"
      ? { bottom: `${window.innerHeight - anchor.top + POPOVER_GAP}px`, top: "auto" }
      : { top: `${anchor.bottom + POPOVER_GAP}px`, bottom: "auto" }),
  };
}

async function scrollActiveIntoView() {
  await nextTick();
  const list = listEl.value;
  const active = list?.querySelector<HTMLElement>(`[data-nav-index="${activeIndex.value}"]`);
  if (!list || !active) return;
  const listRect = list.getBoundingClientRect();
  const activeRect = active.getBoundingClientRect();
  const activeTop = list.scrollTop + activeRect.top - listRect.top;
  const activeBottom = activeTop + activeRect.height;
  if (activeTop < list.scrollTop) {
    list.scrollTop = activeTop;
    return;
  }
  const visibleBottom = list.scrollTop + list.clientHeight;
  if (activeBottom > visibleBottom) {
    list.scrollTop = activeBottom - list.clientHeight;
  }
}

async function layout() {
  await nextTick();
  updatePosition();
  await scrollActiveIntoView();
}

watch(() => [props.anchor, navigationEntries.value.length] as const, () => {
  void layout();
});
watch([activePane, categoryIndex, activeIndex], () => {
  void scrollActiveIntoView();
});
watch(() => props.defaultCategory, resetCategory, { immediate: true });
watch(() => props.query, (query) => {
  searchQuery.value = query;
  activeIndex.value = 0;
});
watch(searchQuery, () => {
  activeIndex.value = 0;
});

function loadMore(group: ComposerSuggestGroup) {
  const key = `${activeCategory.value?.id}:${group.id}`;
  const total = filteredGroups.value.find((item) => item.id === group.id)?.items.length ?? 0;
  visibleCounts[key] = nextCascadeVisibleCount(visibleCounts[key], total, PAGE_SIZE);
}

function handleKeydown(event: KeyboardEvent) {
  const result = navigateCascade(
    {
      pane: activePane.value,
      categoryIndex: categoryIndex.value,
      resultIndex: activeIndex.value,
    },
    event.key,
    props.categories.length,
    navigationEntries.value.length,
  );
  if (!result.handled) return false;
  event.preventDefault();
  event.stopPropagation();
  activePane.value = result.state.pane;
  categoryIndex.value = result.state.categoryIndex;
  activeIndex.value = result.state.resultIndex;
  if (result.intent === "close") emit("close");
  if (result.intent === "enter-results" && activeCategory.value?.action) {
    emit("category-action", activeCategory.value.action);
    return true;
  }
  if (result.intent === "activate-result") {
    const entry = navigationEntries.value[activeIndex.value];
    if (entry?.item) emit("select", entry.item);
    if (entry?.more) loadMore(entry.more);
  }
  return true;
}

defineExpose({ handleKeydown });

function onViewportChange() {
  updatePosition();
}

onMounted(() => {
  void layout();
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("scroll", onViewportChange, true);
});
onBeforeUnmount(() => {
  window.removeEventListener("resize", onViewportChange);
  window.removeEventListener("scroll", onViewportChange, true);
});
</script>

<template>
  <!-- 与 UiSelect 相同：Teleport + fixed 浮层，不占输入框布局 -->
  <Teleport to="body">
    <div
      v-if="anchor"
      ref="root"
      class="ds-select-popover is-floating composer-suggest-menu"
      :data-placement="placement"
      role="listbox"
      :aria-label="ariaLabel"
      :style="popoverStyle"
    >
      <nav class="composer-suggest-categories" aria-label="引用类型">
        <button
          v-for="(category, index) in categories"
          :key="category.id"
          type="button"
          :data-active="index === categoryIndex ? '' : undefined"
          @mousedown.prevent
          @mousemove="activePane = 'categories'; selectCategory(index)"
          @click="activateCategory(index)"
        >
          <Bot v-if="category.id === 'agent'" :size="14" />
          <Sparkles v-else-if="category.id === 'skill'" :size="14" />
          <Hash v-else-if="category.id === 'task'" :size="14" />
          <FolderKanban v-else-if="category.id === 'requirement'" :size="14" />
          <Paperclip v-else :size="14" />
          <span>{{ category.label }}</span>
          <ChevronRight v-if="!category.action" :size="13" />
        </button>
      </nav>
      <section class="composer-suggest-results">
        <label class="ds-select-search">
          <Search :size="14" />
          <input
            v-model="searchQuery"
            :placeholder="t('select.searchPlaceholder')"
            @focus="activePane = 'results'"
            @keydown="handleKeydown"
          />
        </label>
        <div ref="listEl" class="ds-select-list" @mousedown.prevent>
          <template v-for="group in visibleGroups" :key="group.id">
            <div v-if="group.items.length" class="ds-select-group">
              <div class="ds-select-group-label">
                <span>{{ group.label }}</span>
                <span>{{ group.items.length }}/{{ filteredGroups.find((item) => item.id === group.id)?.items.length }}</span>
              </div>
              <div
                v-for="item in group.items"
                :key="`${item.kind}:${item.id}`"
                class="ds-select-option"
                :class="{ 'has-description': Boolean(item.description) }"
                role="option"
                :data-nav-index="navigationEntries.findIndex((entry) => entry.item === item)"
                :data-active="activePane === 'results' && navigationEntries.findIndex((entry) => entry.item === item) === activeIndex ? '' : undefined"
                aria-selected="false"
                @mousemove="activePane = 'results'; activeIndex = navigationEntries.findIndex((entry) => entry.item === item)"
                @click="emit('select', item)"
              >
                <AgentAvatar
                  v-if="item.avatar"
                  class="ds-avatar"
                  :value="item.avatar"
                  :name="item.label"
                  :size="null"
                />
                <span v-else class="composer-suggest-leading">
                  <Sparkles v-if="item.leading === 'sparkles'" :size="14" />
                  <Hash v-else-if="item.leading === 'hash'" :size="14" />
                  <FolderKanban v-else-if="item.leading === 'requirement'" :size="14" />
                  <Bot v-else :size="14" />
                </span>
                <span class="ds-select-option-body">
                  <span class="ds-select-option-label">{{ item.label }}</span>
                  <span v-if="item.description" class="ds-select-option-desc">{{ item.description }}</span>
                </span>
              </div>
              <button
                v-if="group.items.length < (filteredGroups.find((item) => item.id === group.id)?.items.length ?? 0)"
                type="button"
                class="composer-suggest-more"
                :data-nav-index="navigationEntries.findIndex((entry) => entry.more?.id === group.id)"
                :data-active="activePane === 'results' && navigationEntries.findIndex((entry) => entry.more?.id === group.id) === activeIndex ? '' : undefined"
                @mousemove="activePane = 'results'; activeIndex = navigationEntries.findIndex((entry) => entry.more?.id === group.id)"
                @click="loadMore(group)"
              >
                {{ t("select.loadMore", { n: Math.min(PAGE_SIZE, (filteredGroups.find((item) => item.id === group.id)?.items.length ?? 0) - group.items.length) }) }}
              </button>
            </div>
          </template>
          <div v-if="!navigationEntries.length" class="ds-select-empty">{{ t("select.empty") }}</div>
        </div>
      </section>
    </div>
  </Teleport>
</template>
