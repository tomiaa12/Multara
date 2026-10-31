<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronRight, ClipboardList, LoaderCircle } from "@lucide/vue";
import { useAppStore } from "../../stores/app";
import { useDiffStore } from "../../stores/diff";
import { useI18n } from "../../i18n";
import RichContent from "./RichContent.vue";
import PlanPreviewModal from "./PlanPreviewModal.vue";
import type { PlanEntry } from "./plan-state";
import { PLAN_PREVIEW_STEPS, PLAN_STATUS_TONE, undoableChangedFiles } from "./plan-card";

/** 卡片不放操作：批准 / 拒绝在输入框上方的 ComposerPlanHint 和预览弹窗，完善直接在输入框回复 */
const props = withDefaults(defineProps<{
  entry: PlanEntry;
  sessionId: string;
  /** 窄布局：只显示标题、摘要和步骤数 */
  compact?: boolean;
}>(), {
  compact: false,
});

const store = useAppStore();
const diffStore = useDiffStore();
const { t } = useI18n();

const expanded = ref(false);
const previewing = ref(false);
const undoing = ref(false);
const undoNotice = ref("");

const plan = computed(() => props.entry.artifact);
const previewSteps = computed(() => plan.value.steps.slice(0, PLAN_PREVIEW_STEPS));
const hiddenStepCount = computed(() => plan.value.steps.length - previewSteps.value.length);
const undoable = computed(() =>
  undoableChangedFiles(
    plan.value.changedFiles,
    diffStore.listSnapshotFiles(store.sessionWorkspaces(props.sessionId).map((workspace) => workspace.id)),
  ),
);

async function undoChanges() {
  if (!undoable.value.length) {
    undoNotice.value = t("chat.plan.undoUnavailable");
    return;
  }
  undoing.value = true;
  try {
    for (const file of undoable.value) await diffStore.undoAllChunks(file.workspaceId, file.filePath);
  } catch (cause) {
    undoNotice.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    undoing.value = false;
  }
}
</script>

<template>
  <section class="plan-artifact-card" :class="{ 'is-compact': compact }" :data-tone="PLAN_STATUS_TONE[entry.status]">
    <button type="button" class="plan-artifact-head" :aria-label="t('chat.plan.view')" @click="previewing = true">
      <ClipboardList :size="14" />
      <b class="plan-artifact-title">{{ plan.title || t("chat.plan.untitled") }}</b>
      <span class="plan-artifact-status">{{ t(`chat.plan.status.${entry.status}`) }}</span>
    </button>
    <p class="plan-artifact-meta">
      <span>{{ t("chat.plan.revision", { n: plan.revision }) }}</span>
      <span v-if="plan.structured">{{ t("chat.plan.stepCount", { n: plan.steps.length }) }}</span>
      <span v-else>{{ t("chat.plan.unstructured") }}</span>
      <span v-if="plan.risks.length && !compact">{{ t("chat.plan.riskCount", { n: plan.risks.length }) }}</span>
    </p>
    <p v-if="plan.summary" class="plan-artifact-summary">{{ plan.summary }}</p>
    <template v-if="!compact">
      <ol v-if="!expanded && previewSteps.length" class="plan-artifact-steps">
        <li v-for="step in previewSteps" :key="step.id">{{ step.text }}</li>
        <li v-if="hiddenStepCount > 0" class="is-more">{{ t("chat.plan.moreSteps", { n: hiddenStepCount }) }}</li>
      </ol>
      <div v-if="expanded" class="plan-artifact-markdown vs-scroll">
        <RichContent :content="plan.markdown" />
      </div>
      <button type="button" class="plan-artifact-toggle" :aria-expanded="expanded" @click="expanded = !expanded">
        <ChevronRight :size="13" />
        <span>{{ expanded ? t("chat.plan.collapse") : t("chat.plan.expand") }}</span>
      </button>
    </template>
    <p v-if="plan.changedFiles.length" class="plan-artifact-changes">
      <span>{{ t("chat.plan.changedFiles", { n: plan.changedFiles.length }) }}</span>
      <UiButton size="small" variant="ghost" :disabled="undoing" @click="undoChanges">
        <LoaderCircle v-if="undoing" class="process-spinner" :size="12" />
        {{ t("chat.plan.undoChanges") }}
      </UiButton>
    </p>
    <UiNotice v-if="undoNotice" :message="undoNotice" @close="undoNotice = ''" />
    <PlanPreviewModal v-if="previewing" :entry="entry" :session-id="sessionId" @close="previewing = false" />
  </section>
</template>
