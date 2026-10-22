<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "../../i18n";
import { reshuffleHintText } from "../../stores/reask";

const props = defineProps<{
  afterCount: number;
}>();

const emit = defineEmits<{
  confirm: [rollback: boolean];
  cancel: [];
}>();

const { t } = useI18n();
const choice = ref<"keep" | "rollback">("keep");
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('cancel')">
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="reshuffle-title">
      <h2 id="reshuffle-title">{{ t('diff.reshuffleTitle') }}</h2>
      <p>{{ reshuffleHintText(afterCount) }}</p>
      <p>{{ t('diff.reshuffleFilePrompt') }}</p>
      <label class="check-row">
        <input v-model="choice" type="radio" name="reshuffle-files" value="rollback">
        <span>{{ t('diff.reshuffleRollback') }}</span>
      </label>
      <label class="check-row">
        <input v-model="choice" type="radio" name="reshuffle-files" value="keep">
        <span>{{ t('diff.reshuffleKeep') }}</span>
      </label>
      <div class="modal-actions">
        <UiButton @click="emit('cancel')">{{ t('common.cancel') }}</UiButton>
        <UiButton variant="primary" @click="emit('confirm', choice === 'rollback')">{{ t('diff.reshuffleConfirmSend') }}</UiButton>
      </div>
    </div>
  </div>
</template>
