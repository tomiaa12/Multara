<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { ShieldQuestion } from "@lucide/vue";
import { t } from "../../i18n";
import { approvalWaits } from "../../services/remote-invoke";
import UiButton from "../ui/UiButton.vue";

const SECOND_MS = 1000;
const SECONDS_PER_MINUTE = 60;

const now = ref(Date.now());
let timer = 0;

watch(
  () => approvalWaits.length,
  (count) => {
    window.clearInterval(timer);
    if (count) timer = window.setInterval(() => (now.value = Date.now()), SECOND_MS);
  },
  { immediate: true },
);
onBeforeUnmount(() => window.clearInterval(timer));

function remaining(expiresAt: string) {
  const seconds = Math.max(0, Math.round((Date.parse(expiresAt) - now.value) / SECOND_MS));
  return `${Math.floor(seconds / SECONDS_PER_MINUTE)}:${String(seconds % SECONDS_PER_MINUTE).padStart(2, "0")}`;
}
</script>

<template>
  <div v-if="approvalWaits.length" class="remote-approval-wait" role="status">
    <div v-for="wait in approvalWaits" :key="wait.id" class="remote-approval-row">
      <ShieldQuestion :size="16" />
      <div class="grow">
        <strong>{{ t("remote.approval.waiting") }}</strong>
        <small>{{ wait.summary }}</small>
      </div>
      <span class="remote-approval-time">{{ remaining(wait.expiresAt) }}</span>
      <UiButton variant="ghost" size="small" @click="wait.cancel()">{{ t("common.cancel") }}</UiButton>
    </div>
  </div>
</template>

<style scoped>
.remote-approval-wait {
  position: fixed;
  right: 16px;
  bottom: 40px;
  z-index: 60;
  display: grid;
  gap: 8px;
  width: min(380px, 90vw);
}
.remote-approval-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border: 1px solid var(--color-warning-border);
  border-radius: var(--radius-lg);
  background: var(--color-surface-raised);
  box-shadow: var(--shadow-md);
  color: var(--color-text);
}
.grow { display: grid; flex: 1; min-width: 0; gap: 2px; }
.grow small { overflow: hidden; color: var(--color-text-tertiary); text-overflow: ellipsis; white-space: nowrap; }
.remote-approval-time { font-family: var(--font-family-mono); color: var(--color-warning); }
</style>
