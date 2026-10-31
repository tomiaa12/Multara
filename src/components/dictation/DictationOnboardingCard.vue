<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { listAudioInputs, probeMicrophone, type MicPermissionError } from "../../dictation/dictation-errors";
import { t } from "../../i18n";

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();

const probing = ref(true);
const error = ref<MicPermissionError | null>(null);
const devices = ref<MediaDeviceInfo[]>([]);
const level = ref(0);

let stream: MediaStream | null = null;
let raf = 0;
let audioCtx: AudioContext | null = null;

function errorText(code: MicPermissionError | null) {
  if (code === "permission-denied") return t("dictation.error.permission");
  if (code === "no-device") return t("dictation.error.noDevice");
  if (code === "device-in-use") return t("dictation.error.inUse");
  if (code === "unsupported") return t("dictation.error.unsupported");
  if (code) return t("dictation.error.unknown");
  return "";
}

function stopProbe() {
  if (raf) cancelAnimationFrame(raf);
  raf = 0;
  audioCtx?.close().catch(() => undefined);
  audioCtx = null;
  for (const track of stream?.getTracks() ?? []) track.stop();
  stream = null;
}

async function runProbe() {
  probing.value = true;
  error.value = null;
  stopProbe();
  const result = await probeMicrophone();
  probing.value = false;
  if (result.error || !result.stream) {
    error.value = result.error ?? "unknown";
    return;
  }
  stream = result.stream;
  devices.value = await listAudioInputs();
  audioCtx = new AudioContext();
  const source = audioCtx.createMediaStreamSource(stream);
  const analyser = audioCtx.createAnalyser();
  analyser.fftSize = 256;
  analyser.smoothingTimeConstant = 0.75;
  source.connect(analyser);
  const data = new Uint8Array(analyser.frequencyBinCount);
  const tick = () => {
    analyser.getByteFrequencyData(data);
    let sum = 0;
    for (const value of data) sum += value;
    level.value = Math.min(1, sum / (data.length * 128));
    raf = requestAnimationFrame(tick);
  };
  tick();
}

onMounted(() => {
  void runProbe();
});
onBeforeUnmount(stopProbe);

function confirm() {
  stopProbe();
  emit("confirm");
}

function cancel() {
  stopProbe();
  emit("cancel");
}
</script>

<template>
  <UiModal @click.self="cancel">
    <div class="modal dictation-onboarding" role="dialog" aria-modal="true" :aria-label="t('dictation.onboardingTitle')">
      <h2>{{ t("dictation.onboardingTitle") }}</h2>
      <p>{{ t("dictation.onboardingBody") }}</p>
      <p class="dictation-privacy">{{ t("dictation.privacyNote") }}</p>

      <div v-if="probing" class="dictation-probe">{{ t("dictation.checkingMic") }}</div>
      <div v-else-if="error" class="dictation-probe is-error">{{ errorText(error) }}</div>
      <template v-else>
        <div class="dictation-level" aria-hidden="true">
          <span :style="{ transform: `scaleX(${Math.max(0.04, level)})` }" />
        </div>
        <ul v-if="devices.length" class="dictation-devices">
          <li v-for="device in devices" :key="device.deviceId">
            {{ device.label || t("dictation.unnamedMic") }}
          </li>
        </ul>
      </template>

      <div class="modal-actions">
        <UiButton type="button" @click="cancel">{{ t("common.cancel") }}</UiButton>
        <UiButton type="button" variant="primary" :disabled="Boolean(error) || probing" @click="confirm">
          {{ t("dictation.start") }}
        </UiButton>
      </div>
    </div>
  </UiModal>
</template>
