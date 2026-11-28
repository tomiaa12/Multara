<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { MonitorSmartphone, Unplug, X } from "@lucide/vue";
import type { RemoteDevice, RemoteGroup } from "../../domain/remote";
import { roleLabel } from "../../components/remote/remote-labels";
import { confirmDialog } from "../../components/ui/confirm-dialog";
import UiButton from "../../components/ui/UiButton.vue";
import UiInput from "../../components/ui/UiInput.vue";
import UiRelativeTime from "../../components/ui/UiRelativeTime.vue";
import UiSelect from "../../components/ui/UiSelect.vue";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";
import { formatTokens, formatUsd } from "../usage-format";

const devices = ref<RemoteDevice[]>([]);
const groups = ref<RemoteGroup[]>([]);
const error = ref("");
const groupOptions = computed(() =>
  groups.value.map((group) => ({ value: group.id, label: `${group.name} · ${roleLabel(group.role)}` })),
);

async function reload() {
  try {
    [devices.value, groups.value] = await Promise.all([remoteApi.listDevices(), remoteApi.listGroups()]);
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  }
}

async function update(device: RemoteDevice, patch: Partial<Pick<RemoteDevice, "name" | "ownerNote" | "groupId">>) {
  const next = { ...device, ...patch };
  if (!next.name.trim()) return;
  error.value = "";
  try {
    await remoteApi.updateDevice({ id: device.id, name: next.name.trim(), ownerNote: next.ownerNote.trim(), groupId: next.groupId });
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  }
  await reload();
}

function inputValue(event: Event) {
  return (event.target as HTMLInputElement).value;
}

async function remove(device: RemoteDevice) {
  if (!(await confirmDialog(t("remote.device.removeConfirm", { name: device.name }), { danger: true }))) return;
  await remoteApi.removeDevice(device.id).catch((cause) => (error.value = String(cause)));
  await reload();
}

let stop: UnlistenFn | undefined;
onMounted(async () => {
  await reload();
  stop = await listen("remote-devices-changed", () => void reload());
});
onBeforeUnmount(() => stop?.());
</script>

<template>
  <p v-if="error" class="form-error">{{ error }}</p>
  <section v-if="!devices.length" class="panel placeholder">
    <span class="ds-empty-icon"><MonitorSmartphone :size="18" /></span>
    <h2>{{ t("remote.device.empty") }}</h2>
    <p>{{ t("remote.device.emptyHint") }}</p>
  </section>
  <div v-else class="panel remote-table">
    <table class="ds-table">
      <thead>
        <tr>
          <th>{{ t("remote.device.name") }}</th>
          <th>{{ t("remote.device.ownerNote") }}</th>
          <th>{{ t("remote.device.group") }}</th>
          <th>{{ t("remote.device.state") }}</th>
          <th>{{ t("remote.device.running") }}</th>
          <th>{{ t("remote.device.usage") }}</th>
          <th>{{ t("remote.device.pairedAt") }}</th>
          <th>{{ t("common.actions") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="device in devices" :key="device.id">
          <td><UiInput :model-value="device.name" maxlength="64" :aria-label="t('remote.device.name')" @change="update(device, { name: inputValue($event) })" /></td>
          <td><UiInput :model-value="device.ownerNote" maxlength="200" :placeholder="t('remote.device.ownerNotePlaceholder')" :aria-label="t('remote.device.ownerNote')" @change="update(device, { ownerNote: inputValue($event) })" /></td>
          <td class="remote-group-cell">
            <UiSelect size="sm" :model-value="device.groupId" :options="groupOptions" :aria-label="t('remote.device.group')" @update:model-value="update(device, { groupId: String($event) })" />
          </td>
          <td class="remote-nowrap">
            <span class="remote-dot" :class="{ 'is-online': device.online }" />
            {{ device.online ? t("remote.conn.online") : t("remote.conn.offline") }}
            <small v-if="device.lastAddress" class="remote-muted">{{ device.lastAddress }}</small>
          </td>
          <td>{{ device.running }}</td>
          <td class="remote-nowrap">
            {{ formatUsd(device.costUsd) }}
            <small class="remote-muted">{{ formatTokens(device.totalTokens) }} tokens</small>
          </td>
          <td class="remote-nowrap"><UiRelativeTime :value="device.pairedAt" /></td>
          <td class="remote-actions">
            <UiButton v-if="device.online" variant="ghost" size="icon" :tip="t('remote.device.disconnect')" @click="remoteApi.disconnectDevice(device.id)"><Unplug :size="14" /></UiButton>
            <UiButton variant="ghost" size="icon" :tip="t('remote.device.remove')" @click="remove(device)"><X :size="14" /></UiButton>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.remote-table { overflow: auto; }
.remote-group-cell { min-width: 200px; }
.remote-nowrap { white-space: nowrap; }
.remote-muted { display: block; color: var(--color-text-tertiary); font-family: var(--font-family-mono); }
.remote-dot { display: inline-block; width: 8px; height: 8px; margin-right: 4px; border-radius: 50%; background: var(--color-text-disabled); }
.remote-dot.is-online { background: var(--color-success); }
.remote-actions { width: 1%; text-align: right; white-space: nowrap; }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
