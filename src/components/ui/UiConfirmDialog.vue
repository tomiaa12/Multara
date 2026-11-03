<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";
import type { ConfirmDialogRequest } from "./confirm-dialog";

const props = defineProps<{ request: ConfirmDialogRequest }>();
const emit = defineEmits<{ resolve: [confirmed: boolean] }>();

const primaryButton = ref<{ $el?: HTMLElement } | null>(null);

function finish(confirmed: boolean) {
  emit("resolve", confirmed);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  event.stopPropagation();
  finish(false);
}

onMounted(() => {
  void nextTick(() => primaryButton.value?.$el?.focus());
});
</script>

<template>
  <UiModal @click.self="finish(false)" @keydown="onKeydown">
    <div
      class="modal ui-confirm-dialog"
      role="alertdialog"
      aria-modal="true"
      :aria-label="props.request.message"
    >
      <div class="ui-confirm-body">
        <Codicon
          :name="props.request.type === 'warning' ? 'warning' : 'info'"
          class="ui-confirm-icon"
          :class="`is-${props.request.type ?? 'info'}`"
        />
        <div>
          <h2>{{ props.request.message }}</h2>
          <p v-if="props.request.detail" class="ui-confirm-detail vs-scroll">{{ props.request.detail }}</p>
        </div>
      </div>
      <div class="modal-actions">
        <UiButton @click="finish(false)">{{ props.request.cancel }}</UiButton>
        <UiButton
          ref="primaryButton"
          :variant="props.request.danger ? 'danger' : 'primary'"
          @click="finish(true)"
        >
          {{ props.request.primary }}
        </UiButton>
      </div>
    </div>
  </UiModal>
</template>

<style scoped>
.ui-confirm-dialog {
  max-width: 460px;
}

.ui-confirm-body {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}

.ui-confirm-body h2 {
  margin: 0 0 6px;
  font-size: 14px;
  line-height: 1.45;
  word-break: break-word;
}

.ui-confirm-icon {
  flex: none;
  margin-top: 2px;
  font-size: 20px;
}

.ui-confirm-icon.is-warning {
  color: var(--color-warning);
}

.ui-confirm-icon.is-info {
  color: var(--color-info);
}

.ui-confirm-detail {
  margin: 0;
  white-space: pre-wrap;
  color: var(--color-text-secondary);
  font-size: 12px;
  max-height: 220px;
  overflow: auto;
  word-break: break-all;
}
</style>
