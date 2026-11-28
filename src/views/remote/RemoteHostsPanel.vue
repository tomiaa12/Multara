<script setup lang="ts">
import { onMounted, ref } from "vue";
import { ArrowRightLeft, KeyRound, Plug, Plus, Server, Unplug, X } from "@lucide/vue";
import type { RemoteHost } from "../../domain/remote";
import { connStateLabel, formatFingerprint } from "../../components/remote/remote-labels";
import { confirmDialog } from "../../components/ui/confirm-dialog";
import UiButton from "../../components/ui/UiButton.vue";
import UiInput from "../../components/ui/UiInput.vue";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";
import { switchHost } from "../../services/remote-target";
import { useRemoteStore } from "../../stores/remote";
import RemoteAddHostDialog from "./RemoteAddHostDialog.vue";

const remote = useRemoteStore();
const adding = ref(false);
const error = ref("");

async function act(action: () => Promise<void>) {
  error.value = "";
  await action().catch((cause) => (error.value = String(cause).replace(/^Error:\s*/, "")));
  await remote.loadHosts();
}

function inputValue(event: Event) {
  return (event.target as HTMLInputElement).value.trim();
}

function update(host: RemoteHost, patch: Partial<Pick<RemoteHost, "name" | "address" | "port">>) {
  const next = { ...host, ...patch };
  if (!next.name || !next.address || !Number(next.port)) return;
  void act(() => remoteApi.updateHost({ id: host.id, name: next.name, address: next.address, port: Number(next.port) }));
}

async function remove(host: RemoteHost) {
  if (!(await confirmDialog(t("remote.hosts.removeConfirm", { name: host.name }), { danger: true }))) return;
  await act(() => remoteApi.removeHost(host.id));
  if (host.id === remote.hostId) switchHost(null);
}

async function onPaired() {
  adding.value = false;
  await remote.loadHosts();
}

onMounted(() => void remote.loadHosts());
</script>

<template>
  <div class="remote-toolbar">
    <UiButton variant="primary" @click="adding = true"><Plus :size="16" /> {{ t("remote.hosts.add") }}</UiButton>
    <UiButton v-if="remote.isRemote" @click="remote.openRequest()"><KeyRound :size="16" /> {{ t("remote.request.open") }}</UiButton>
  </div>
  <p v-if="error" class="form-error">{{ error }}</p>
  <section v-if="!remote.hosts.length" class="panel placeholder">
    <span class="ds-empty-icon"><Server :size="18" /></span>
    <h2>{{ t("remote.hosts.empty") }}</h2>
    <p>{{ t("remote.hosts.emptyHint") }}</p>
  </section>
  <div v-else class="panel remote-table">
    <table class="ds-table">
      <thead>
        <tr>
          <th>{{ t("remote.hosts.name") }}</th>
          <th>{{ t("remote.hosts.address") }}</th>
          <th>{{ t("remote.hostMode.port") }}</th>
          <th>{{ t("remote.device.state") }}</th>
          <th>{{ t("remote.hosts.fingerprint") }}</th>
          <th>{{ t("common.actions") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="host in remote.hosts" :key="host.id">
          <td><UiInput :model-value="host.name" maxlength="64" :aria-label="t('remote.hosts.name')" @change="update(host, { name: inputValue($event) })" /></td>
          <td><UiInput :model-value="host.address" :aria-label="t('remote.hosts.address')" @change="update(host, { address: inputValue($event) })" /></td>
          <td class="remote-port"><UiInput :model-value="host.port" type="number" :aria-label="t('remote.hostMode.port')" @change="update(host, { port: Number(inputValue($event)) })" /></td>
          <td class="remote-nowrap">
            <span class="remote-dot" :class="`is-${host.status?.state ?? 'offline'}`" />
            {{ connStateLabel(host.status?.state) }}
            <small v-if="host.status?.error" class="remote-muted">{{ host.status.error }}</small>
          </td>
          <td class="remote-mono">{{ formatFingerprint(host.fingerprint) }}</td>
          <td class="remote-actions">
            <UiButton v-if="host.id !== remote.hostId" variant="ghost" size="icon" :tip="t('remote.host.switch')" @click="switchHost(host.id)"><ArrowRightLeft :size="14" /></UiButton>
            <UiButton v-if="host.status?.state === 'online'" variant="ghost" size="icon" :tip="t('remote.hosts.disconnect')" @click="act(() => remoteApi.disconnectHost(host.id))"><Unplug :size="14" /></UiButton>
            <UiButton v-else variant="ghost" size="icon" :tip="t('remote.hosts.connect')" @click="act(() => remoteApi.connectHost(host.id))"><Plug :size="14" /></UiButton>
            <UiButton variant="ghost" size="icon" :tip="t('remote.hosts.remove')" @click="remove(host)"><X :size="14" /></UiButton>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
  <RemoteAddHostDialog v-if="adding" @close="adding = false" @paired="onPaired" />
</template>

<style scoped>
.remote-toolbar { display: flex; gap: 8px; }
.remote-table { overflow: auto; }
.remote-port { width: 110px; }
.remote-nowrap { white-space: nowrap; }
.remote-mono { font-family: var(--font-family-mono); font-size: var(--font-size-sm); }
.remote-muted { display: block; color: var(--color-text-tertiary); }
.remote-dot { display: inline-block; width: 8px; height: 8px; margin-right: 4px; border-radius: 50%; background: var(--color-text-disabled); }
.remote-dot.is-online { background: var(--color-success); }
.remote-dot.is-connecting { background: var(--color-warning); }
.remote-dot.is-error { background: var(--color-error); }
.remote-actions { width: 1%; text-align: right; white-space: nowrap; }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
