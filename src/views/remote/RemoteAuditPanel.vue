<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { ScrollText } from "@lucide/vue";
import type { RemoteAuditEntry, RemoteDevice, RemoteGroup } from "../../domain/remote";
import { auditActionLabel, auditOutcomeLabel } from "../../components/remote/remote-labels";
import UiButton from "../../components/ui/UiButton.vue";
import UiRelativeTime from "../../components/ui/UiRelativeTime.vue";
import UiSelect from "../../components/ui/UiSelect.vue";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";

const PAGE_SIZE = 100;
const OUTCOMES = ["ok", "denied", "pending", "error"];

const entries = ref<RemoteAuditEntry[]>([]);
const devices = ref<RemoteDevice[]>([]);
const groups = ref<RemoteGroup[]>([]);
const filters = reactive({ deviceId: "", groupId: "", outcome: "" });
const hasMore = ref(false);
const loading = ref(false);
const error = ref("");

const deviceOptions = computed(() => devices.value.map((device) => ({ value: device.id, label: device.name })));
const groupOptions = computed(() => groups.value.map((group) => ({ value: group.id, label: group.name })));
const outcomeOptions = OUTCOMES.map((outcome) => ({ value: outcome, label: auditOutcomeLabel(outcome) }));

async function load(append: boolean) {
  loading.value = true;
  error.value = "";
  try {
    const page = await remoteApi.queryAudit({
      deviceId: filters.deviceId || null,
      groupId: filters.groupId || null,
      outcome: filters.outcome || null,
      beforeId: append ? entries.value.at(-1)?.id : null,
      limit: PAGE_SIZE,
    });
    entries.value = append ? [...entries.value, ...page] : page;
    hasMore.value = page.length === PAGE_SIZE;
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  } finally {
    loading.value = false;
  }
}

function resource(entry: RemoteAuditEntry) {
  return [entry.resourceKind, entry.resourceId].filter(Boolean).join(":");
}

watch(filters, () => void load(false));
onMounted(async () => {
  [devices.value, groups.value] = await Promise.all([remoteApi.listDevices(), remoteApi.listGroups()]).catch(() => [[], []]);
  await load(false);
});
</script>

<template>
  <div class="remote-audit-filters">
    <UiSelect v-model="filters.deviceId" size="sm" clearable :options="deviceOptions" :placeholder="t('remote.audit.allDevices')" />
    <UiSelect v-model="filters.groupId" size="sm" clearable :options="groupOptions" :placeholder="t('remote.audit.allGroups')" />
    <UiSelect v-model="filters.outcome" size="sm" clearable :options="outcomeOptions" :placeholder="t('remote.audit.allOutcomes')" />
  </div>
  <p v-if="error" class="form-error">{{ error }}</p>
  <section v-if="!entries.length && !loading" class="panel placeholder">
    <span class="ds-empty-icon"><ScrollText :size="18" /></span>
    <h2>{{ t("remote.audit.empty") }}</h2>
  </section>
  <div v-else class="panel remote-table">
    <table class="ds-table">
      <thead>
        <tr>
          <th>{{ t("remote.audit.time") }}</th>
          <th>{{ t("remote.audit.device") }}</th>
          <th>{{ t("remote.audit.action") }}</th>
          <th>{{ t("remote.audit.resource") }}</th>
          <th>{{ t("remote.audit.outcome") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="entry in entries" :key="entry.id">
          <td class="remote-nowrap"><UiRelativeTime :value="entry.at" /></td>
          <td>{{ entry.deviceName }}</td>
          <td>
            {{ auditActionLabel(entry.action) }}
            <code v-if="entry.cmd" class="remote-mono">{{ entry.cmd }}</code>
          </td>
          <td class="remote-mono">{{ resource(entry) }}</td>
          <td class="remote-nowrap" :class="`is-${entry.outcome}`">{{ auditOutcomeLabel(entry.outcome) }}</td>
        </tr>
      </tbody>
    </table>
    <UiButton v-if="hasMore" size="small" class="remote-more" :disabled="loading" @click="load(true)">{{ t("remote.audit.more") }}</UiButton>
  </div>
</template>

<style scoped>
.remote-audit-filters { display: flex; flex-wrap: wrap; gap: 8px; }
.remote-audit-filters > * { width: 200px; }
.remote-table { overflow: auto; }
.remote-nowrap { white-space: nowrap; }
.remote-mono { font-family: var(--font-family-mono); font-size: var(--font-size-sm); overflow-wrap: anywhere; }
.is-denied, .is-error { color: var(--color-error); }
.is-pending { color: var(--color-warning); }
.remote-more { margin-top: 8px; }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
