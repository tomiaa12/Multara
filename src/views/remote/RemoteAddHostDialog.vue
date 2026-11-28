<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { X } from "@lucide/vue";
import { parsePairingLink } from "../../domain/remote";
import { formatFingerprint } from "../../components/remote/remote-labels";
import UiButton from "../../components/ui/UiButton.vue";
import UiInput from "../../components/ui/UiInput.vue";
import UiModal from "../../components/ui/UiModal.vue";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";

/** 与主机端 `store::DEFAULT_PORT` 一致。 */
const DEFAULT_PORT = 43130;

const emit = defineEmits<{ close: []; paired: [] }>();

const form = reactive({ link: "", address: "", port: DEFAULT_PORT, code: "", fingerprint: "", deviceName: "" });
const myFingerprint = ref("");
const busy = ref(false);
const error = ref("");

function applyLink() {
  const parsed = parsePairingLink(form.link);
  if (parsed) Object.assign(form, parsed);
}

async function submit() {
  busy.value = true;
  error.value = "";
  try {
    await remoteApi.pairHost({
      address: form.address.trim(),
      port: Number(form.port) || DEFAULT_PORT,
      code: form.code.trim(),
      fingerprint: form.fingerprint.trim(),
      deviceName: form.deviceName.trim(),
    });
    emit("paired");
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  myFingerprint.value = await remoteApi.localFingerprint().catch(() => "");
});
</script>

<template>
  <UiModal @close="emit('close')">
    <form class="modal domain-modal" @submit.prevent="submit">
      <header class="modal-heading">
        <h2>{{ t("remote.hosts.add") }}</h2>
        <UiButton type="button" variant="ghost" size="icon" :tip="t('common.close')" @click="emit('close')"><X :size="16" /></UiButton>
      </header>
      <div class="modal-body">
        <label>
          {{ t("remote.hosts.link") }}
          <UiInput v-model="form.link" placeholder="multara://pair?..." @input="applyLink" />
          <small class="remote-hint">{{ t("remote.hosts.linkHint") }}</small>
        </label>
        <div class="remote-row">
          <label class="remote-grow">{{ t("remote.hosts.address") }}<UiInput v-model.trim="form.address" required /></label>
          <label class="remote-port">{{ t("remote.hostMode.port") }}<UiInput v-model.number="form.port" type="number" required /></label>
        </div>
        <label>{{ t("remote.hosts.code") }}<UiInput v-model.trim="form.code" required autocomplete="off" /></label>
        <label>
          {{ t("remote.hosts.fingerprint") }}
          <UiInput v-model.trim="form.fingerprint" :placeholder="t('remote.hosts.fingerprintPlaceholder')" />
        </label>
        <label>{{ t("remote.hosts.deviceName") }}<UiInput v-model="form.deviceName" maxlength="64" :placeholder="t('remote.hosts.deviceNamePlaceholder')" /></label>
        <p v-if="myFingerprint" class="remote-hint">{{ t("remote.hosts.myFingerprint") }} <code>{{ formatFingerprint(myFingerprint) }}</code></p>
        <p v-if="busy" class="remote-hint">{{ t("remote.hosts.waiting") }}</p>
        <p v-if="error" class="form-error">{{ error }}</p>
      </div>
      <footer class="modal-actions">
        <UiButton type="button" @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton type="submit" variant="primary" :disabled="busy || !form.address.trim() || !form.code.trim()">{{ t("remote.hosts.pair") }}</UiButton>
      </footer>
    </form>
  </UiModal>
</template>

<style scoped>
.remote-row { display: flex; gap: 8px; }
.remote-grow { flex: 1; }
.remote-port { width: 120px; }
.remote-hint { margin: 0; color: var(--color-text-tertiary); font-size: var(--font-size-sm); }
.remote-hint code { font-family: var(--font-family-mono); }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
