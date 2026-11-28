<script setup lang="ts">
import { computed } from "vue";
import { Lock, Server } from "@lucide/vue";
import { t } from "../../i18n";
import { lastDenied } from "../../services/remote-invoke";
import { useRemoteStore } from "../../stores/remote";
import RemoteRequestDialog from "./RemoteRequestDialog.vue";
import { connStateLabel, roleLabel } from "./remote-labels";

const remote = useRemoteStore();

const summary = computed(() => {
  const group = remote.me?.group;
  const host = remote.currentHost?.name ?? remote.me?.hostName ?? "";
  if (!group) return `${host} · ${connStateLabel(remote.connState)}`;
  return t("remote.status.summary", { host, group: group.name, role: roleLabel(group.role), state: connStateLabel(remote.connState) });
});
</script>

<template>
  <template v-if="remote.isRemote">
    <UiTooltip :content="lastDenied?.message ?? t('remote.request.title')">
      <button type="button" class="statusbar-item" @click="remote.openRequest(lastDenied?.perm ?? '')">
        <Lock :size="12" /> {{ t("remote.request.open") }}
      </button>
    </UiTooltip>
    <span class="statusbar-item remote-status" :class="`is-${remote.connState}`">
      <Server :size="12" /> {{ summary }}
    </span>
  </template>
  <RemoteRequestDialog v-if="remote.requesting !== null" :perm="remote.requesting" @close="remote.requesting = null" />
</template>

<style scoped>
.remote-status { display: inline-flex; align-items: center; gap: 4px; }
.remote-status.is-offline, .remote-status.is-error { color: var(--color-error); }
.remote-status.is-connecting { color: var(--color-text-tertiary); }
</style>
