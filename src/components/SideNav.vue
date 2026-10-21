<script setup lang="ts">
import type { Component } from "vue";
import { RouterLink } from "vue-router";
import UiCountBadge from "./ui/UiCountBadge.vue";

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

defineProps<{
  groups: SideNavGroup[];
  collapsed?: boolean;
  ariaLabel?: string;
}>();

const emit = defineEmits<{
  select: [item: SideNavItem];
}>();
</script>

<template>
  <nav class="sidebar" :aria-label="ariaLabel">
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
        :title="collapsed ? item.label : item.title"
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
  </nav>
</template>
