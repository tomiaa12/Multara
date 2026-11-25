<script setup lang="ts">
// 底栏右下角：没人跑时是「0 个运行中」；有人跑时是堆叠头像 + 扫光状态，整段有最大宽度。
import { computed } from "vue";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import AvatarStack, { type AvatarStackItem } from "./ui/AvatarStack.vue";
import {
  collectAllLiveRuns,
  liveCapacity,
  liveRunMessage,
  resolveRunLiveWorkStatus,
} from "./agent-live-status";
import { liveWorkParts } from "./chat/timeline-display";

const store = useAppStore();

const liveRuns = computed(() => collectAllLiveRuns(store.runStates));
const liveItems = computed<AvatarStackItem[]>(() =>
  liveRuns.value.map((run) => {
    const agent = store.data?.agents.find((item) => item.id === run.agentId);
    return {
      key: run.agentId,
      name: agent?.name ?? t("inbox.agentFallback"),
      image: agent?.avatar ?? "",
    };
  }),
);

const liveLabel = computed(() => {
  const runs = liveRuns.value;
  if (runs.length > 1) return t("agentLive.multi", { n: runs.length });
  const run = runs[0];
  if (!run) return "";
  const timeline = liveRunMessage(store.messages, run)?.timeline ?? [];
  const status = resolveRunLiveWorkStatus(run, store.runStates, liveCapacity(store.data));
  if (status === "running" && timeline.length === 0) return t("chat.thinking");
  return liveWorkParts(timeline, 0, {
    language: store.data?.settings.language,
    status,
  })[0]?.label ?? t("chat.thinking");
});
</script>

<template>
  <span class="running-status" :class="{ 'is-live': liveRuns.length }" aria-live="polite">
    <template v-if="liveRuns.length">
      <AvatarStack
        :items="liveItems"
        :max="3"
        :size="14"
        :dialog-title="t('dashboard.col.workers')"
      />
      <UiShimmerText class="running-status-label" :text="liveLabel" rotate />
    </template>
    <template v-else>
      <i class="ds-dot" />
      {{ t("app.runningCount", { n: 0 }) }}
    </template>
  </span>
</template>

<style scoped>
.running-status {
  display: inline-flex;
  align-items: center;
  gap: var(--spacing-1-5);
  max-width: 220px;
  min-width: 0;
  padding-left: var(--spacing-2);
  color: var(--color-text-tertiary);
}
.running-status.is-live {
  color: var(--color-text-secondary);
}
.running-status :deep(.ds-avatar-stack) {
  flex: none;
}
.running-status :deep(.ds-avatar-stack > * + * .agent-avatar),
.running-status :deep(.ds-avatar-stack > * + * .ds-avatar),
.running-status :deep(.ds-avatar-stack > * + *.ds-avatar-stack-more) {
  box-shadow: 0 0 0 2px var(--color-bg-secondary);
}
.running-status-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.running-status.is-live :deep(.ds-text-shimmer) {
  --ds-shimmer-base: var(--color-text-secondary);
}
</style>
