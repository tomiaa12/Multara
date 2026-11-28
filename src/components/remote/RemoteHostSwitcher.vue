<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { Monitor, Server } from "@lucide/vue";
import { t } from "../../i18n";
import { switchHost } from "../../services/remote-target";
import { useRemoteStore } from "../../stores/remote";
import ContextMenu, { type ContextMenuItem } from "../ui/ContextMenu.vue";
import { connStateLabel } from "./remote-labels";

const LOCAL_ID = "local";
const MANAGE_ID = "manage";

defineProps<{ collapsed?: boolean }>();

const remote = useRemoteStore();
const router = useRouter();
const menu = ref<{ x: number; y: number } | null>(null);

const label = computed(() => remote.currentHost?.name ?? (remote.isRemote ? t("remote.host.unknown") : t("remote.host.local")));

const items = computed<ContextMenuItem[]>(() => [
  { id: LOCAL_ID, label: t("remote.host.local"), checked: !remote.isRemote },
  ...remote.hosts.map((host) => ({
    id: host.id,
    label: t("remote.host.option", { name: host.name, state: connStateLabel(host.status?.state) }),
    checked: host.id === remote.hostId,
  })),
  { type: "sep" as const },
  { id: MANAGE_ID, label: t("remote.host.manage") },
]);

function open(event: MouseEvent) {
  void remote.loadHosts();
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  menu.value = { x: rect.left, y: rect.top - 8 };
}

function select(id: string) {
  menu.value = null;
  if (id === MANAGE_ID) {
    void router.push("/remote");
    return;
  }
  switchHost(id === LOCAL_ID ? null : id);
}
</script>

<template>
  <UiTooltip v-if="remote.isRemote || remote.hosts.length" placement="right" :content="collapsed ? label : t('remote.host.switch')">
    <button type="button" class="nav-item" :class="{ 'router-link-active': remote.isRemote }" @click="open">
      <Server v-if="remote.isRemote" :size="16" />
      <Monitor v-else :size="16" />
      <span class="nav-item-label">{{ label }}</span>
    </button>
  </UiTooltip>
  <ContextMenu v-if="menu" :x="menu.x" :y="menu.y" :items="items" @select="select" @close="menu = null" />
</template>
