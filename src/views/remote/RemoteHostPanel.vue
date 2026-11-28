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
import RemoteConnectionLog from "./RemoteConnectionLog.vue";
import RemoteHostNetworks from "./RemoteHostNetworks.vue";

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
  <div v-if="status" class="remote-host-stack">
    <section class="settings-card">
      <div class="settings-row">
        <div>
          <strong>{{ t("remote.hostMode.title") }}</strong>
          <p>{{ t("remote.hostMode.firewallHint") }}</p>
        </div>
        <UiSwitch :model-value="status.enabled" :busy="busy" :label="t('remote.hostMode.title')" @update:model-value="setEnabled" />
      </div>
      <p v-if="status.error || error" class="settings-hint form-error">{{ status.error || error }}</p>
      <div class="settings-row">
        <div><strong>{{ t("remote.hostMode.port") }}</strong></div>
        <UiInput v-model="port" class="remote-port" inputmode="numeric" :aria-label="t('remote.hostMode.port')" @change="savePort" @keydown.enter="savePort" />
      </div>
      <RemoteHostNetworks :status="status" @update="apply" @error="error = $event" />
      <div class="settings-row settings-row-start">
        <div><strong>{{ t("remote.hostMode.fingerprint") }}</strong></div>
        <code class="remote-fp">{{ formatFingerprint(status.fingerprint) }}</code>
      </div>
    </section>

    <section v-if="status.running" class="settings-card">
      <div class="settings-row">
        <div>
          <strong>{{ t("remote.pairing.title") }}</strong>
          <p>{{ t("remote.pairing.hint") }}</p>
        </div>
        <UiButton v-if="!pairing" variant="primary" @click="createCode"><KeyRound :size="16" /> {{ t("remote.pairing.create") }}</UiButton>
        <UiButton v-else variant="ghost" size="icon" :tip="t('common.cancel')" @click="cancelCode"><X :size="16" /></UiButton>
      </div>
      <div v-if="pairing" class="settings-row settings-row-stack">
        <!-- 二维码 SVG 由本机后端生成 -->
        <div class="remote-pairing">
          <div class="remote-qr" v-html="pairing.qrSvg" />
          <div class="remote-pairing-info">
            <code class="remote-code">{{ pairing.code }}</code>
            <span class="remote-muted">{{ t("remote.pairing.expiresIn", { time: remainingLabel }) }}</span>
            <UiButton size="small" @click="copy(pairing.link)"><Copy :size="14" /> {{ t("remote.pairing.copyLink") }}</UiButton>
          </div>
        </div>
      </div>
    </section>

    <RemoteConnectionLog v-if="status.running" />
  </div>
</template>

<style scoped>
.remote-host-stack { display: grid; gap: var(--spacing-4); }
.remote-port { width: 120px; }
.remote-code, .remote-fp { font-family: var(--font-family-mono); }
.remote-fp { overflow-wrap: anywhere; text-align: right; }
.remote-muted { color: var(--color-text-tertiary); font-size: var(--font-size-sm); }
.remote-pairing { display: flex; gap: 20px; align-items: center; }
.remote-qr { flex: none; width: 168px; height: 168px; padding: 8px; border-radius: var(--radius-lg); background: #fff; }
.remote-qr :deep(svg) { width: 100%; height: 100%; }
.remote-pairing-info { display: grid; gap: 10px; justify-items: start; }
.remote-code { font-size: 28px; letter-spacing: 4px; color: var(--color-text-strong); }
.form-error { color: var(--color-error); }
</style>
