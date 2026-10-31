<script setup lang="ts">
import { computed, ref } from "vue";
import { ClipboardList, LoaderCircle, X } from "@lucide/vue";
import { useAppStore } from "../../stores/app";
import { useI18n } from "../../i18n";
import RichContent from "./RichContent.vue";
import type { PlanEntry } from "./plan-state";
import { usePlanAction } from "./use-plan-action";

const props = defineProps<{
  entry: PlanEntry;
  sessionId: string;
}>();

const emit = defineEmits<{ close: [] }>();

const store = useAppStore();
const { t } = useI18n();
const { pendingAction, actionError, act } = usePlanAction(() => props.sessionId);
const copied = ref(false);

const plan = computed(() => props.entry.artifact);
const actionable = computed(() => store.planStates(props.sessionId).latestAwaitingKey === props.entry.key);

async function copyPlan() {
  await navigator.clipboard.writeText(plan.value.markdown);
  copied.value = true;
}

async function approve() {
  if (await act(props.entry, "approve")) emit("close");
}
</script>

<template>
  <UiModal close-on-backdrop @click.self="emit('close')">
    <section class="modal plan-preview-modal" role="dialog" aria-modal="true" aria-labelledby="plan-preview-title" @keydown.esc="emit('close')">
      <header class="modal-heading">
        <h2 id="plan-preview-title" class="plan-preview-title">
          <ClipboardList :size="16" />
          <span>{{ plan.title || t("chat.plan.untitled") }}</span>
        </h2>
        <UiButton size="icon" variant="ghost" :tip="t('common.close')" :aria-label="t('common.close')" @click="emit('close')">
          <X :size="15" />
        </UiButton>
      </header>
      <p class="plan-artifact-meta">
        <span>{{ t(`chat.plan.status.${entry.status}`) }}</span>
        <span>{{ t("chat.plan.revision", { n: plan.revision }) }}</span>
        <span v-if="plan.structured">{{ t("chat.plan.stepCount", { n: plan.steps.length }) }}</span>
      </p>
      <div class="plan-preview-body vs-scroll">
        <RichContent :content="plan.markdown" />
      </div>
      <UiNotice v-if="actionError" tone="error" sticky :message="actionError" @close="actionError = ''" />
      <div class="modal-actions">
        <UiButton variant="ghost" @click="copyPlan">{{ copied ? t("chat.plan.copied") : t("chat.plan.copy") }}</UiButton>
        <UiButton v-if="actionable" variant="primary" :disabled="Boolean(pendingAction)" @click="approve">
          <LoaderCircle v-if="pendingAction === 'approve'" class="process-spinner" :size="12" />
          {{ t("chat.plan.approve") }}
        </UiButton>
      </div>
    </section>
  </UiModal>
</template>
