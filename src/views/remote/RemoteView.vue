<script setup lang="ts">
import { computed, ref } from "vue";
import { useRoute } from "vue-router";
import SegmentedTabs from "../../components/ui/SegmentedTabs.vue";
import { t } from "../../i18n";
import { useRemoteStore } from "../../stores/remote";
import RemoteAuditPanel from "./RemoteAuditPanel.vue";
import RemoteDevicesPanel from "./RemoteDevicesPanel.vue";
import RemoteGroupsPanel from "./RemoteGroupsPanel.vue";
import RemoteHostPanel from "./RemoteHostPanel.vue";
import RemoteHostsPanel from "./RemoteHostsPanel.vue";
import RemotePendingPanel from "./RemotePendingPanel.vue";

const HOSTS_TAB = "hosts";
const HOST_TABS = ["host", "devices", "groups", "pending", "audit", HOSTS_TAB];

const remote = useRemoteStore();
const route = useRoute();

/** 远程模式下这里只管理「我连接的主机」；本机的主机模式回到本机后再管。 */
const tabs = computed(() =>
  (remote.isRemote ? [HOSTS_TAB] : HOST_TABS).map((id) => ({
    id,
    label: t(`remote.tab.${id}`),
    count: id === "pending" ? remote.pendingCount : undefined,
    pinCount: id === "pending" && remote.pendingCount > 0,
  })),
);
const requested = String(route.query.tab ?? "");
const tab = ref(tabs.value.some((item) => item.id === requested) ? requested : tabs.value[0].id);
</script>

<template>
  <div class="page remote-page">
    <div class="page-header">
      <div>
        <h1>{{ t("remote.title") }}</h1>
        <p>{{ remote.isRemote ? t("remote.subtitleRemote") : t("remote.subtitle") }}</p>
      </div>
    </div>
    <SegmentedTabs v-if="tabs.length > 1" v-model="tab" :items="tabs" :label="t('remote.title')" />
    <RemoteHostPanel v-if="tab === 'host'" />
    <RemoteDevicesPanel v-else-if="tab === 'devices'" />
    <RemoteGroupsPanel v-else-if="tab === 'groups'" />
    <RemotePendingPanel v-else-if="tab === 'pending'" />
    <RemoteAuditPanel v-else-if="tab === 'audit'" />
    <RemoteHostsPanel v-else />
  </div>
</template>

<style scoped>
.remote-page { display: flex; flex-direction: column; gap: 16px; }
</style>
