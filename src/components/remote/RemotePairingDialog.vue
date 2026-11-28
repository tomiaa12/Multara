<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { AlertTriangle } from "@lucide/vue";
import type { RemoteGroup, RemotePendingPairing } from "../../domain/remote";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";
import UiButton from "../ui/UiButton.vue";
import UiInput from "../ui/UiInput.vue";
import UiModal from "../ui/UiModal.vue";
import UiSelect from "../ui/UiSelect.vue";
import { roleLabel } from "./remote-labels";

const pending = ref<RemotePendingPairing[]>([]);
const groups = ref<RemoteGroup[]>([]);
const busy = ref(false);
const error = ref("");
const form = reactive({ name: "", ownerNote: "", groupId: "" });
const current = computed(() => pending.value[0] ?? null);
const groupOptions = computed(() =>
  groups.value.map((group) => ({ value: group.id, label: `${group.name} · ${roleLabel(group.role)}` })),
);

async function reload() {
  const [items, list] = await Promise.all([
    remoteApi.pendingPairings().catch(() => []),
    remoteApi.listGroups().catch(() => groups.value),
  ]);
  const previous = current.value?.id;
  pending.value = items;
  groups.value = list;
  if (current.value && current.value.id !== previous) {
    form.name = current.value.deviceName;
    form.ownerNote = "";
    form.groupId = list.find((group) => group.isDefault)?.id ?? list[0]?.id ?? "";
    error.value = "";
  }
}

async function decide(accept: boolean) {
  const item = current.value;
  if (!item) return;
  busy.value = true;
  error.value = "";
  try {
    await remoteApi.confirmPairing({ id: item.id, accept, name: form.name.trim() || item.deviceName, ownerNote: form.ownerNote.trim(), groupId: form.groupId });
    await reload();
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
    await reload();
  } finally {
    busy.value = false;
  }
}

const stops: UnlistenFn[] = [];
onMounted(async () => {
  await reload();
  stops.push(await listen("remote-pairing-request", () => void reload()));
  stops.push(await listen("remote-pairing-changed", () => void reload()));
});
onBeforeUnmount(() => stops.forEach((stop) => stop()));
</script>

<template>
  <UiModal v-if="current" :close-on-esc="false">
    <form class="modal domain-modal" @submit.prevent="decide(true)">
      <header class="modal-heading">
        <h2>{{ t("remote.pairing.confirmTitle") }}</h2>
      </header>
      <div class="modal-body">
        <p class="remote-meta">
          {{ t("remote.pairing.from", { address: current.address }) }}
          <br />
          {{ t("remote.pairing.fingerprint") }} <code>{{ current.fingerprintShort }}</code>
        </p>
        <label>{{ t("remote.device.name") }}<UiInput v-model="form.name" maxlength="64" /></label>
        <label>{{ t("remote.device.ownerNote") }}<UiInput v-model="form.ownerNote" :placeholder="t('remote.device.ownerNotePlaceholder')" maxlength="200" /></label>
        <label>{{ t("remote.device.group") }}<UiSelect v-model="form.groupId" :options="groupOptions" /></label>
        <p class="remote-risk"><AlertTriangle :size="14" /> {{ t("remote.pairing.risk") }}</p>
        <p v-if="error" class="form-error">{{ error }}</p>
      </div>
      <div class="modal-actions">
        <UiButton type="button" variant="danger" :disabled="busy" @click="decide(false)">{{ t("remote.pairing.reject") }}</UiButton>
        <UiButton type="submit" variant="primary" :disabled="busy || !form.groupId">{{ t("remote.pairing.accept") }}</UiButton>
      </div>
    </form>
  </UiModal>
</template>

<style scoped>
.remote-meta { margin: 0; color: var(--color-text-secondary); font-size: var(--font-size-sm); line-height: 1.6; }
.remote-meta code { font-family: var(--font-family-mono); color: var(--color-text-strong); }
.remote-risk { display: flex; gap: 6px; margin: 0; color: var(--color-warning); font-size: var(--font-size-sm); line-height: 1.6; }
.remote-risk svg { flex: none; margin-top: 3px; }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
