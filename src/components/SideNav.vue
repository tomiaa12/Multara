<script setup lang="ts">
import type { Component } from "vue";
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { t } from "../i18n";
import {
  SETTINGS_NAV_COLLAPSE_THRESHOLD,
  SETTINGS_NAV_COLLAPSED_KEY,
  SETTINGS_NAV_COLLAPSED_WIDTH,
  SETTINGS_NAV_DEFAULT_WIDTH,
  SETTINGS_NAV_EXPAND_THRESHOLD,
  SETTINGS_NAV_MAX_WIDTH,
  SETTINGS_NAV_MIN_WIDTH,
} from "./side-nav";
import UiCountBadge from "./ui/UiCountBadge.vue";
import { readStorage, writeStorage } from "../utils/safe-storage";

export type SideNavItem = {
  id: string;
  label: string;
  icon?: Component;
  to?: string;
  active?: boolean;
  badge?: string | number;
  title?: string;
  children?: SideNavItem[];
};

export type SideNavGroup = {
  label?: string | null;
  items: SideNavItem[];
};

const props = defineProps<{
  groups: SideNavGroup[];
  collapsed?: boolean;
  ariaLabel?: string;
  /** 传入后由本组件拖宽/折叠并持久化；不传则把 resize 交给父级（主侧栏） */
  widthKey?: string;
  defaultWidth?: number;
  minWidth?: number;
  maxWidth?: number;
}>();

const emit = defineEmits<{
  select: [item: SideNavItem];
  resizeStart: [event: PointerEvent];
}>();

const minW = computed(() => props.minWidth ?? SETTINGS_NAV_MIN_WIDTH);
const maxW = computed(() => props.maxWidth ?? SETTINGS_NAV_MAX_WIDTH);
const defaultW = computed(() => props.defaultWidth ?? SETTINGS_NAV_DEFAULT_WIDTH);

function clampWidth(width: number) {
  return Math.min(maxW.value, Math.max(minW.value, Math.round(width)));
}

function readStoredWidth() {
  if (!props.widthKey) return defaultW.value;
  const raw = Number(readStorage(props.widthKey));
  if (!Number.isFinite(raw)) return defaultW.value;
  return clampWidth(raw);
}

function readStoredCollapsed() {
  if (!props.widthKey) return false;
  return readStorage(SETTINGS_NAV_COLLAPSED_KEY) === "true";
}

const ownWidth = ref(readStoredWidth());
const ownCollapsed = ref(readStoredCollapsed());
const resizing = ref(false);

/** 主侧栏用 props.collapsed；二级侧栏用自管折叠 */
const iconCollapsed = computed(() => Boolean(props.collapsed || (props.widthKey && ownCollapsed.value)));

const rootStyle = computed(() => {
  if (!props.widthKey) return undefined;
  const px = ownCollapsed.value ? SETTINGS_NAV_COLLAPSED_WIDTH : ownWidth.value;
  return { "--settings-nav-width": `${px}px` };
});

function persistOwn() {
  if (!props.widthKey) return;
  writeStorage(props.widthKey, String(ownWidth.value));
  writeStorage(SETTINGS_NAV_COLLAPSED_KEY, String(ownCollapsed.value));
}

function beginOwnResize(event: PointerEvent) {
  const handle = event.currentTarget;
  if (!(handle instanceof HTMLElement) || event.button !== 0) return;
  event.preventDefault();
  handle.setPointerCapture(event.pointerId);
  resizing.value = true;
  document.body.classList.add("is-resizing-panels");
  const startX = event.clientX;
  const originCollapsed = ownCollapsed.value;
  const startWidth = originCollapsed ? SETTINGS_NAV_COLLAPSED_WIDTH : ownWidth.value;

  const move = (moveEvent: PointerEvent) => {
    const next = startWidth + (moveEvent.clientX - startX);
    if (ownCollapsed.value) {
      if (next < SETTINGS_NAV_EXPAND_THRESHOLD) return;
      ownCollapsed.value = false;
      ownWidth.value = clampWidth(Math.max(next, minW.value));
      return;
    }
    if (next < SETTINGS_NAV_COLLAPSE_THRESHOLD) {
      ownCollapsed.value = true;
      return;
    }
    ownWidth.value = clampWidth(next);
  };

  const finish = () => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", finish);
    handle.removeEventListener("pointercancel", finish);
    if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId);
    document.body.classList.remove("is-resizing-panels");
    resizing.value = false;
    persistOwn();
  };

  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", finish);
  handle.addEventListener("pointercancel", finish);
}

function onResizePointerDown(event: PointerEvent) {
  if (props.widthKey) {
    beginOwnResize(event);
    return;
  }
  emit("resizeStart", event);
}
</script>

<template>
  <nav
    class="sidebar"
    :class="{
      'is-sidebar-collapsed': iconCollapsed,
      'is-sidebar-resizing': resizing,
    }"
    :aria-label="ariaLabel"
    :style="rootStyle"
  >
    <div v-for="(group, index) in groups" :key="group.label ?? index" class="nav-group">
      <span v-if="group.label" class="nav-label">{{ group.label }}</span>
      <component
        :is="item.to ? RouterLink : 'button'"
        v-for="item in group.items"
        :key="item.id"
        :to="item.to"
        :type="item.to ? undefined : 'button'"
        class="nav-item"
        :class="{ 'router-link-active': item.active }"
        :aria-current="item.active ? 'page' : undefined"
        :title="iconCollapsed ? item.label : item.title"
        @click="emit('select', item)"
      >
        <slot name="leading" :item="item">
          <component :is="item.icon" v-if="item.icon" :size="16" />
        </slot>
        <span class="nav-item-label">{{ item.label }}</span>
        <UiCountBadge
          v-if="item.badge != null && item.badge !== ''"
          class="nav-badge"
          tone="primary"
          :label="item.badge"
        />
      </component>
    </div>
    <div v-if="$slots.footer" class="nav-group sidebar-footer">
      <slot name="footer" />
    </div>
    <div
      class="panel-resize-handle sidebar-resize-handle"
      role="separator"
      :aria-label="t('app.resizeSidebar')"
      aria-orientation="vertical"
      tabindex="0"
      @pointerdown="onResizePointerDown"
    />
  </nav>
</template>
