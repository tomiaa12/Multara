<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { X } from "@lucide/vue";
import { PROJECT_REQUEST, REMOTE_PERMS, type RemotePermissionRequest } from "../../domain/remote";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";
import { useRemoteStore } from "../../stores/remote";
import { showToast } from "../ui/toast";
import UiButton from "../ui/UiButton.vue";
import UiModal from "../ui/UiModal.vue";
import UiRelativeTime from "../ui/UiRelativeTime.vue";
import UiSelect from "../ui/UiSelect.vue";
import UiTextarea from "../ui/UiTextarea.vue";
import { permLabel, requestStatusLabel, roleLabel } from "./remote-labels";

const props = defineProps<{ perm?: string }>();
const emit = defineEmits<{ close: [] }>();

const remote = useRemoteStore();
const permission = ref(props.perm || PROJECT_REQUEST);
const note = ref("");
const busy = ref(false);
const error = ref("");
const mine = ref<RemotePermissionRequest[]>([]);

const options = computed(() => [
  { value: PROJECT_REQUEST, label: t("remote.perm.project") },
  ...REMOTE_PERMS.filter((perm) => !remote.can(perm)).map((perm) => ({ value: perm, label: permLabel(perm) })),
]);

async function loadMine() {
  if (!remote.hostId) return;
  mine.value = await remoteApi.listMyRequests(remote.hostId).catch(() => mine.value);
}

async function submit() {
  if (!remote.hostId) return;
  busy.value = true;
  error.value = "";
  try {
    await remoteApi.requestPermission(remote.hostId, { permission: permission.value, note: note.value.trim() });
    showToast(t("remote.request.sent"), "success");
    emit("close");
  } catch (cause) {
    error.value = (cause as Error)?.message ?? String(cause);
  } finally {
    busy.value = false;
  }
}

async function cancel(item: RemotePermissionRequest) {
  if (!remote.hostId) return;
  await remoteApi.cancelRequest(remote.hostId, item.id).catch(() => undefined);
  await loadMine();
}

onMounted(loadMine);
</script>

<template>
  <UiModal @close="emit('close')">
    <form class="modal domain-modal" @submit.prevent="submit">
      <header class="modal-heading">
        <h2>{{ t("remote.request.title") }}</h2>
        <UiButton type="button" variant="ghost" size="icon" :tip="t('common.close')" @click="emit('close')"><X :size="16" /></UiButton>
      </header>
      <div class="modal-body">
        <p v-if="remote.me" class="remote-hint">
          {{ t("remote.request.current", { device: remote.me.device.name, group: remote.me.group.name, role: roleLabel(remote.me.group.role) }) }}
        </p>
        <label>{{ t("remote.request.permission") }}<UiSelect v-model="permission" :options="options" /></label>
        <label>{{ t("remote.request.note") }}<UiTextarea v-model="note" :placeholder="t('remote.request.notePlaceholder')" maxlength="500" /></label>
        <p v-if="error" class="form-error">{{ error }}</p>
        <section v-if="mine.length" class="remote-mine">
          <h3>{{ t("remote.request.mine") }}</h3>
          <div v-for="item in mine" :key="item.id" class="remote-mine-row">
            <span class="grow">{{ permLabel(item.permission) }}</span>
            <span class="remote-hint">{{ requestStatusLabel(item.status) }} · <UiRelativeTime :value="item.createdAt" /></span>
            <UiButton v-if="item.status === 'pending'" type="button" variant="ghost" size="small" @click="cancel(item)">{{ t("remote.request.withdraw") }}</UiButton>
          </div>
        </section>
      </div>
      <div class="modal-actions">
        <UiButton type="button" @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton type="submit" variant="primary" :disabled="busy">{{ t("remote.request.submit") }}</UiButton>
      </div>
    </form>
  </UiModal>
</template>

<style scoped>
.remote-hint { margin: 0; color: var(--color-text-tertiary); font-size: var(--font-size-sm); }
.remote-mine { display: grid; gap: 6px; }
.remote-mine h3 { margin: 4px 0 0; font-size: var(--font-size-sm); }
.remote-mine-row { display: flex; align-items: center; gap: 8px; }
.grow { flex: 1; min-width: 0; }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
