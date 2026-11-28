<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { Copy, KeyRound, X } from "@lucide/vue";
import type { RemoteHostStatus, RemotePairingCode } from "../../domain/remote";
import { formatFingerprint } from "../../components/remote/remote-labels";
import UiButton from "../../components/ui/UiButton.vue";
import UiInput from "../../components/ui/UiInput.vue";
import UiSwitch from "../../components/ui/UiSwitch.vue";
import { showToast } from "../../components/ui/toast";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";

const SECOND_MS = 1000;
const SECONDS_PER_MINUTE = 60;

const status = ref<RemoteHostStatus | null>(null);
const port = ref("");
const busy = ref(false);
const error = ref("");
const pairing = ref<RemotePairingCode | null>(null);
const expiresAt = ref(0);
const now = ref(Date.now());
let timer = 0;

const remaining = computed(() => Math.max(0, Math.round((expiresAt.value - now.value) / SECOND_MS)));
const remainingLabel = computed(
  () => `${Math.floor(remaining.value / SECONDS_PER_MINUTE)}:${String(remaining.value % SECONDS_PER_MINUTE).padStart(2, "0")}`,
);

function apply(next: RemoteHostStatus) {
  status.value = next;
  port.value = String(next.port);
}

async function run(action: () => Promise<RemoteHostStatus>) {
  busy.value = true;
  error.value = "";
  try {
    apply(await action());
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  } finally {
    busy.value = false;
  }
}

function setEnabled(enabled: boolean) {
  if (!enabled) clearPairing();
  return run(() => remoteApi.setHostEnabled(enabled));
}

function savePort() {
  const value = Number(port.value);
  if (!Number.isInteger(value) || value === status.value?.port) return;
  void run(() => remoteApi.setHostPort(value));
}

function clearPairing() {
  window.clearInterval(timer);
  pairing.value = null;
}

async function createCode() {
  error.value = "";
  try {
    pairing.value = await remoteApi.createPairingCode();
    expiresAt.value = Date.now() + pairing.value.expiresInSecs * SECOND_MS;
    now.value = Date.now();
    window.clearInterval(timer);
    timer = window.setInterval(() => {
      now.value = Date.now();
      if (remaining.value <= 0) clearPairing();
    }, SECOND_MS);
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  }
}

async function cancelCode() {
  clearPairing();
  await remoteApi.cancelPairingCode().catch(() => undefined);
}

async function copy(text: string) {
  await navigator.clipboard?.writeText(text).catch(() => undefined);
  showToast(t("remote.copied"), "success");
}

onMounted(async () => {
  apply(await remoteApi.hostStatus());
});
onBeforeUnmount(() => window.clearInterval(timer));
</script>

<template>
  <section v-if="status" class="panel remote-host">
    <div class="remote-row">
      <div class="grow">
        <strong>{{ t("remote.hostMode.title") }}</strong>
        <small>{{ status.running ? t("remote.hostMode.running") : t("remote.hostMode.off") }}</small>
      </div>
      <UiSwitch :model-value="status.enabled" :busy="busy" :label="t('remote.hostMode.title')" @update:model-value="setEnabled" />
    </div>
    <p class="remote-hint">{{ t("remote.hostMode.firewallHint") }}</p>
    <p v-if="status.error || error" class="form-error">{{ status.error || error }}</p>
    <dl class="remote-facts">
      <dt>{{ t("remote.hostMode.port") }}</dt>
      <dd>
        <UiInput v-model="port" class="remote-port" inputmode="numeric" :aria-label="t('remote.hostMode.port')" @change="savePort" @keydown.enter="savePort" />
      </dd>
      <dt>{{ t("remote.hostMode.addresses") }}</dt>
      <dd>
        <code v-for="address in status.addresses" :key="address">{{ address }}:{{ status.port }}</code>
        <span v-if="!status.addresses.length" class="remote-hint">{{ t("remote.hostMode.noAddress") }}</span>
      </dd>
      <dt>{{ t("remote.hostMode.fingerprint") }}</dt>
      <dd><code class="remote-fp">{{ formatFingerprint(status.fingerprint) }}</code></dd>
    </dl>
  </section>

  <section v-if="status?.running" class="panel remote-host">
    <div class="remote-row">
      <div class="grow">
        <strong>{{ t("remote.pairing.title") }}</strong>
        <small>{{ t("remote.pairing.hint") }}</small>
      </div>
      <UiButton v-if="!pairing" variant="primary" @click="createCode"><KeyRound :size="16" /> {{ t("remote.pairing.create") }}</UiButton>
      <UiButton v-else variant="ghost" size="icon" :tip="t('common.cancel')" @click="cancelCode"><X :size="16" /></UiButton>
    </div>
    <div v-if="pairing" class="remote-pairing">
      <!-- 二维码 SVG 由本机后端生成 -->
      <div class="remote-qr" v-html="pairing.qrSvg" />
      <div class="remote-pairing-info">
        <code class="remote-code">{{ pairing.code }}</code>
        <span class="remote-hint">{{ t("remote.pairing.expiresIn", { time: remainingLabel }) }}</span>
        <UiButton size="small" @click="copy(pairing.link)"><Copy :size="14" /> {{ t("remote.pairing.copyLink") }}</UiButton>
      </div>
    </div>
  </section>
</template>

<style scoped>
.remote-host { display: grid; gap: 12px; }
.remote-row { display: flex; align-items: center; gap: 12px; }
.grow { display: grid; flex: 1; min-width: 0; gap: 2px; }
.grow small, .remote-hint { color: var(--color-text-tertiary); font-size: var(--font-size-sm); }
.remote-hint { margin: 0; line-height: 1.6; }
.remote-facts { display: grid; grid-template-columns: max-content 1fr; gap: 8px 16px; align-items: center; margin: 0; }
.remote-facts dt { color: var(--color-text-secondary); font-size: var(--font-size-sm); }
.remote-facts dd { display: flex; flex-wrap: wrap; gap: 8px; margin: 0; }
.remote-facts code, .remote-code { font-family: var(--font-family-mono); }
.remote-fp { overflow-wrap: anywhere; }
.remote-port { width: 120px; }
.remote-pairing { display: flex; gap: 20px; align-items: center; }
.remote-qr { flex: none; width: 168px; height: 168px; padding: 8px; border-radius: var(--radius-lg); background: #fff; }
.remote-qr :deep(svg) { width: 100%; height: 100%; }
.remote-pairing-info { display: grid; gap: 10px; justify-items: start; }
.remote-code { font-size: 28px; letter-spacing: 4px; color: var(--color-text-strong); }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
