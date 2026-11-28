<script setup lang="ts">
import { ref } from "vue";
import type { RemoteHostStatus } from "../../domain/remote";
import UiSwitch from "../../components/ui/UiSwitch.vue";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";

defineProps<{ status: RemoteHostStatus }>();
const emit = defineEmits<{ update: [status: RemoteHostStatus]; error: [message: string] }>();

const busy = ref("");
const OTHER = "other";

async function run(key: string, action: () => Promise<RemoteHostStatus>) {
  busy.value = key;
  try {
    emit("update", await action());
  } catch (cause) {
    emit("error", String(cause).replace(/^Error:\s*/, ""));
  } finally {
    busy.value = "";
  }
}
</script>

<template>
  <div class="settings-row settings-row-start">
    <div>
      <strong>{{ t("remote.hostMode.addresses") }}</strong>
      <p>{{ t("remote.network.hint") }}</p>
    </div>
  </div>
  <div v-for="network in status.networks" :key="network.cidr" class="settings-row remote-network-row">
    <div>
      <code>{{ network.address }}:{{ status.port }}</code>
      <small class="remote-network-meta">{{ network.interface }} · {{ network.cidr }}</small>
    </div>
    <UiSwitch
      :model-value="network.allowed"
      :busy="busy === network.cidr"
      :label="network.cidr"
      @update:model-value="(allowed) => run(network.cidr, () => remoteApi.setNetworkAllowed(network.cidr, allowed))"
    />
  </div>
  <p v-if="!status.networks.length" class="settings-hint">{{ t("remote.hostMode.noAddress") }}</p>
  <div class="settings-row remote-network-row">
    <div>
      <strong>{{ t("remote.network.other") }}</strong>
      <small class="remote-network-meta">{{ t("remote.network.otherHint") }}</small>
    </div>
    <UiSwitch
      :model-value="status.allowOther"
      :busy="busy === OTHER"
      :label="t('remote.network.other')"
      @update:model-value="(allowed) => run(OTHER, () => remoteApi.setAllowOther(allowed))"
    />
  </div>
</template>

<style scoped>
.remote-network-row code { font-family: var(--font-family-mono); }
.remote-network-meta { display: block; color: var(--color-text-tertiary); font-size: var(--font-size-sm); }
</style>
