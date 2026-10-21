<script setup lang="ts">
import { t } from "../../i18n";

defineProps<{
  title: string;
  message: string;
  saving?: boolean;
}>();

const emit = defineEmits<{
  save: [];
  discard: [];
  cancel: [];
}>();
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('cancel')">
    <div class="modal unsaved-modal" role="dialog" aria-modal="true">
      <h2>{{ title }}</h2>
      <p>{{ message }}</p>
      <div class="modal-actions">
        <UiButton :disabled="saving" @click="emit('cancel')">{{ t("dialog.unsavedCancel") }}</UiButton>
        <UiButton :disabled="saving" @click="emit('discard')">{{ t("dialog.unsavedDiscard") }}</UiButton>
        <UiButton variant="primary" :disabled="saving" @click="emit('save')">
          {{ saving ? t("common.saving") : t("dialog.unsavedSave") }}
        </UiButton>
      </div>
    </div>
  </div>
</template>
