<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { Info, TriangleAlert, CircleAlert, CircleCheck } from "@lucide/vue";
import { t } from "../../i18n";
import { dismissToast, getToasts, onToastsChange, type ToastItem, type ToastTone } from "./toast";

const toasts = ref<ToastItem[]>(getToasts());
let stop: (() => void) | null = null;

onMounted(() => {
  stop = onToastsChange(() => {
    toasts.value = getToasts();
  });
});
onBeforeUnmount(() => stop?.());

function icon(tone: ToastTone) {
  if (tone === "success") return CircleCheck;
  if (tone === "error") return CircleAlert;
  if (tone === "warning") return TriangleAlert;
  return Info;
}
</script>

<template>
  <div class="toast-host" aria-live="polite">
    <div
      v-for="item in toasts"
      :key="item.id"
      v-motion="'slide-up'"
      class="ds-toast toast-item"
      :data-tone="item.tone === 'info' ? undefined : item.tone"
      role="status"
    >
      <component :is="icon(item.tone)" :size="16" />
      <div><strong>{{ item.message }}</strong></div>
      <button type="button" class="toast-close" :aria-label="t('common.close')" @click="dismissToast(item.id)">×</button>
    </div>
  </div>
</template>

<style scoped>
.toast-host {
  position: fixed;
  right: 16px;
  bottom: 48px;
  z-index: var(--z-toast);
  display: grid;
  gap: var(--spacing-2);
  pointer-events: none;
}
.toast-item {
  position: relative;
  pointer-events: auto;
  padding-right: 28px;
}
.toast-item[data-tone="warning"] > svg { color: var(--color-warning); }
.toast-item[data-tone="info"] > svg,
.toast-item:not([data-tone]) > svg { color: var(--color-info); }
.toast-close {
  position: absolute;
  top: 8px;
  right: 8px;
  border: 0;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  font-size: 16px;
  line-height: 1;
}
.toast-close:hover { color: var(--color-text); }
</style>
