<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";
import type { ExplorerDialogRequest } from "./use-explorer";

const props = defineProps<{ request: ExplorerDialogRequest }>();
const emit = defineEmits<{ resolve: [result: { confirmed: boolean; checked: boolean }] }>();

const checked = ref(false);
const primaryButton = ref<{ $el?: HTMLElement } | null>(null);

function finish(confirmed: boolean) {
  emit("resolve", { confirmed, checked: checked.value });
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    finish(false);
  }
}

onMounted(() => {
  void nextTick(() => primaryButton.value?.$el?.focus());
});
</script>

<template>
  <UiModal @click.self="finish(false)" @keydown="onKeydown">
    <div class="modal explorer-dialog" role="alertdialog" aria-modal="true" :aria-label="props.request.message">
      <div class="explorer-dialog-body">
        <Codicon
          :name="props.request.type === 'warning' ? 'warning' : 'info'"
          class="explorer-dialog-icon"
          :class="`is-${props.request.type ?? 'info'}`"
        />
        <div>
          <h2>{{ props.request.message }}</h2>
          <div v-if="props.request.detail" class="explorer-dialog-detail vs-scroll">{{ props.request.detail }}</div>
          <label v-if="props.request.checkbox" class="explorer-dialog-check">
            <input v-model="checked" type="checkbox" />
            <span>{{ props.request.checkbox }}</span>
          </label>
        </div>
      </div>
      <div class="modal-actions">
        <UiButton @click="finish(false)">{{ props.request.cancel ?? "取消" }}</UiButton>
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
.explorer-dialog {
  max-width: 460px;
}

.explorer-dialog-body {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}

.explorer-dialog-body h2 {
  margin: 0 0 6px;
  font-size: 14px;
  line-height: 1.45;
  word-break: break-word;
}

.explorer-dialog-icon {
  flex: none;
  margin-top: 2px;
  font-size: 20px;
}

.explorer-dialog-icon.is-warning {
  color: var(--color-warning);
}

.explorer-dialog-icon.is-info {
  color: var(--color-info);
}

.explorer-dialog-detail {
  margin: 0 0 8px;
  white-space: pre-wrap;
  color: var(--color-text-secondary);
  font-size: 12px;
  max-height: 220px;
  overflow: auto;
  word-break: break-all;
}

.explorer-dialog-check {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  cursor: pointer;
}
</style>
