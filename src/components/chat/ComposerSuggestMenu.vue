<script setup lang="ts">
import { nextTick, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { Hash, Sparkles } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import { nextPopupZIndex } from "../ui/popup-z-index";

export type ComposerSuggestItem = {
  id: string;
  label: string;
  description?: string;
  /** 有头像时按智能体下拉选项渲染（AgentAvatar + ds-select-option） */
  avatar?: string | null;
  leading?: "sparkles" | "hash";
};

const props = defineProps<{
  items: ComposerSuggestItem[];
  activeIndex: number;
  /** 光标或输入区的视口矩形，用于浮层定位（不占输入框文档流） */
  anchor: { left: number; top: number; bottom: number; width?: number } | null;
  ariaLabel?: string;
}>();

const emit = defineEmits<{
  select: [index: number];
  highlight: [index: number];
}>();

/** 默认在光标下方；仅下方放不下菜单高度时才翻到上方 */
const POPOVER_GAP = 4;
const VIEWPORT_MARGIN = 12;
const MENU_MAX_WIDTH = 220;
const MENU_MAX_HEIGHT = 200;
/** 下方至少要能放下这么高，否则改向上 */
const MIN_BELOW = 120;

const root = ref<HTMLElement | null>(null);
const listEl = ref<HTMLElement | null>(null);
const placement = ref<"bottom" | "top">("bottom");
const layerZ = nextPopupZIndex();
const popoverStyle = ref<Record<string, string>>({ zIndex: String(layerZ) });

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
  const active = list?.querySelector<HTMLElement>("[data-active]");
  if (!list || !active) return;
  const listRect = list.getBoundingClientRect();
  const activeRect = active.getBoundingClientRect();
  if (activeRect.top < listRect.top) {
    list.scrollTop -= listRect.top - activeRect.top;
    return;
  }
  if (activeRect.bottom > listRect.bottom) {
    list.scrollTop += activeRect.bottom - listRect.bottom;
  }
}

async function layout() {
  await nextTick();
  updatePosition();
  await scrollActiveIntoView();
}

watch(() => [props.anchor, props.items.length] as const, () => {
  void layout();
});
watch(() => props.activeIndex, () => {
  void scrollActiveIntoView();
});

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
      <div ref="listEl" class="ds-select-list" @mousedown.prevent>
        <div
          v-for="(item, index) in items"
          :key="item.id"
          class="ds-select-option"
          :class="{ 'has-description': Boolean(item.description) }"
          role="option"
          :data-index="index"
          :data-active="index === activeIndex ? '' : undefined"
          aria-selected="false"
          @mousemove="index !== activeIndex && emit('highlight', index)"
          @click="emit('select', index)"
        >
          <AgentAvatar
            v-if="item.avatar"
            class="ds-avatar"
            :value="item.avatar"
            :name="item.label"
            :size="null"
          />
          <span v-else-if="item.leading === 'sparkles'" class="composer-suggest-leading">
            <Sparkles :size="14" />
          </span>
          <span v-else-if="item.leading === 'hash'" class="composer-suggest-leading">
            <Hash :size="14" />
          </span>
          <span class="ds-select-option-body">
            <span class="ds-select-option-label">{{ item.label }}</span>
            <span v-if="item.description" class="ds-select-option-desc">{{ item.description }}</span>
          </span>
        </div>
      </div>
    </div>
  </Teleport>
</template>
