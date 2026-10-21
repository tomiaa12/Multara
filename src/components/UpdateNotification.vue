<script setup lang="ts">
import { computed } from "vue";
import { RefreshCw, X } from "@lucide/vue";
import { t } from "../i18n";
import { useUpdaterStore } from "../stores/updater";

const updater = useUpdaterStore();

const visible = computed(
  () => Boolean(updater.readyVersion) && !updater.dismissed && !updater.installing,
);

async function restartNow() {
  try {
    await updater.installNow();
  } catch (cause) {
    console.error(cause);
  }
}
</script>

<template>
  <div v-if="visible" class="update-toast ds-toast" data-tone="success" role="status">
    <RefreshCw :size="16" />
    <div>
      <strong>{{ t("updates.readyTitle") }}</strong>
      <small>{{ t("updates.readyBody", { version: updater.readyVersion }) }}</small>
      <div class="update-toast-actions">
        <UiButton size="small" variant="ghost" @click="restartNow">
          {{ t("updates.restartNow") }}
        </UiButton>
      </div>
    </div>
    <button
      type="button"
      class="update-toast-close"
      :title="t('common.close')"
      :aria-label="t('common.close')"
      @click="updater.dismiss()"
    >
      <X :size="14" />
    </button>
  </div>
</template>
