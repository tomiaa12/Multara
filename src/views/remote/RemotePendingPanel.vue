<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Check, Inbox, X } from "@lucide/vue";
import type { RemoteApproval, RemoteGroup, RemotePermissionRequest } from "../../domain/remote";
import { permLabel, requestStatusLabel, roleLabel } from "../../components/remote/remote-labels";
import UiButton from "../../components/ui/UiButton.vue";
import UiRelativeTime from "../../components/ui/UiRelativeTime.vue";
import UiSelect from "../../components/ui/UiSelect.vue";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";
import { useAppStore } from "../../stores/app";

const PENDING = "pending";

const store = useAppStore();
const approvals = ref<RemoteApproval[]>([]);
const requests = ref<RemotePermissionRequest[]>([]);
const groups = ref<RemoteGroup[]>([]);
/** 每条申请选中的目标分组。 */
const moveTo = reactive<Record<string, string>>({});
const error = ref("");

const pendingRequests = computed(() => requests.value.filter((item) => item.status === PENDING));
const decidedRequests = computed(() => requests.value.filter((item) => item.status !== PENDING));
const groupOptions = computed(() =>
  groups.value.map((group) => ({ value: group.id, label: `${group.name} · ${roleLabel(group.role)}` })),
);

function projectName(id?: string | null) {
  return store.data?.projects.find((project) => project.id === id)?.name ?? id ?? "";
}

async function reload() {
  try {
    [approvals.value, requests.value, groups.value] = await Promise.all([
      remoteApi.listApprovals(true),
      remoteApi.listRequests(),
      remoteApi.listGroups(),
    ]);
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  }
}

async function act(action: () => Promise<void>) {
  error.value = "";
  await action().catch((cause) => (error.value = String(cause).replace(/^Error:\s*/, "")));
  await reload();
}

const stops: UnlistenFn[] = [];
onMounted(async () => {
  await reload();
  stops.push(await listen("remote-approvals-changed", () => void reload()));
  stops.push(await listen("remote-requests-changed", () => void reload()));
});
onBeforeUnmount(() => stops.forEach((stop) => stop()));
</script>

<template>
  <p v-if="error" class="form-error">{{ error }}</p>
  <section v-if="!approvals.length && !pendingRequests.length" class="panel placeholder">
    <span class="ds-empty-icon"><Inbox :size="18" /></span>
    <h2>{{ t("remote.pending.empty") }}</h2>
  </section>

  <section v-if="approvals.length" class="panel remote-section">
    <h2>{{ t("remote.pending.approvals") }}</h2>
    <article v-for="item in approvals" :key="item.id" class="remote-item">
      <div class="grow">
        <strong>{{ item.deviceName }}</strong>
        <code>{{ item.summary }}</code>
        <small>{{ t("remote.pending.expires") }} <UiRelativeTime :value="item.expiresAt" /></small>
      </div>
      <UiButton variant="danger" size="small" @click="act(() => remoteApi.decideApproval(item.id, false))"><X :size="14" /> {{ t("remote.pending.reject") }}</UiButton>
      <UiButton variant="primary" size="small" @click="act(() => remoteApi.decideApproval(item.id, true))"><Check :size="14" /> {{ t("remote.pending.approve") }}</UiButton>
    </article>
  </section>

  <section v-if="pendingRequests.length" class="panel remote-section">
    <h2>{{ t("remote.pending.requests") }}</h2>
    <article v-for="item in pendingRequests" :key="item.id" class="remote-item">
      <div class="grow">
        <strong>{{ item.deviceName }} · {{ permLabel(item.permission) }}</strong>
        <small v-if="item.projectId">{{ t("remote.pending.project", { name: projectName(item.projectId) }) }}</small>
        <small v-if="item.note">{{ item.note }}</small>
        <small><UiRelativeTime :value="item.createdAt" /></small>
      </div>
      <UiSelect v-model="moveTo[item.id]" size="sm" class="remote-move" :options="groupOptions" :placeholder="t('remote.pending.pickGroup')" />
      <UiButton size="small" :disabled="!moveTo[item.id]" @click="act(() => remoteApi.decideRequest(item.id, 'move', moveTo[item.id]))">{{ t("remote.pending.move") }}</UiButton>
      <UiButton v-if="item.projectId" size="small" :title="t('remote.pending.addProjectHint')" @click="act(() => remoteApi.decideRequest(item.id, 'addProject'))">{{ t("remote.pending.addProject") }}</UiButton>
      <UiButton variant="danger" size="small" @click="act(() => remoteApi.decideRequest(item.id, 'reject'))">{{ t("remote.pending.reject") }}</UiButton>
    </article>
  </section>

  <section v-if="decidedRequests.length" class="panel remote-section">
    <h2>{{ t("remote.pending.history") }}</h2>
    <article v-for="item in decidedRequests" :key="item.id" class="remote-item">
      <div class="grow">
        <span>{{ item.deviceName }} · {{ permLabel(item.permission) }}</span>
        <small>{{ requestStatusLabel(item.status) }} · <UiRelativeTime :value="item.decidedAt ?? item.createdAt" /></small>
      </div>
    </article>
  </section>
</template>

<style scoped>
.remote-section { display: grid; gap: 8px; }
.remote-section h2 { margin: 0; font-size: var(--font-size-lg); }
.remote-item { display: flex; align-items: center; gap: 8px; padding: 8px 0; border-top: 1px solid var(--color-border-light); }
.grow { display: grid; flex: 1; min-width: 0; gap: 2px; }
.grow small { color: var(--color-text-tertiary); }
.grow code { font-family: var(--font-family-mono); font-size: var(--font-size-sm); overflow-wrap: anywhere; }
.remote-move { width: 200px; }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
