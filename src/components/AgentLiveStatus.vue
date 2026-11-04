<script setup lang="ts">
// 智能体状态格：有智能体在跑时显示「头像 + 轮播动作文案」，多个同时跑时显示堆叠头像 + 「N 个智能体处理中」；
// 没在跑时回退到传入的 fallback（堆叠头像，或单个头像 + 名字）。需求列表、任务卡片共用。
import { computed } from "vue";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import AgentAvatar from "./AgentAvatar.vue";
import AvatarStack, { type AvatarStackItem } from "./ui/AvatarStack.vue";
import { collectLiveRuns, liveRunMessage } from "./agent-live-status";
import { liveWorkParts } from "./chat/timeline-display";

const props = withDefaults(defineProps<{
  /** 这格对应的会话：需求下所有会话 / 任务自己的会话 */
  sessionIds: string[];
  /** 没有在跑的智能体时展示的人 */
  fallback?: AvatarStackItem[];
  /** fallback 只有一个人时，头像后面带名字 */
  showName?: boolean;
  max?: number;
  size?: number;
  dialogTitle?: string;
}>(), {
  fallback: () => [],
  showName: false,
  max: 3,
  size: 18,
  dialogTitle: "",
});

const store = useAppStore();

const liveRuns = computed(() => collectLiveRuns(store.runStates, props.sessionIds));
const liveItems = computed<AvatarStackItem[]>(() =>
  liveRuns.value.map((run) => {
    const agent = store.data?.agents.find((item) => item.id === run.agentId);
    return {
      key: run.agentId,
      name: agent?.name ?? t("inbox.agentFallback"),
      image: agent?.avatar ?? "M",
    };
  }),
);

const liveLabel = computed(() => {
  const runs = liveRuns.value;
  if (runs.length > 1) return t("agentLive.multi", { n: runs.length });
  const run = runs[0];
  if (!run) return "";
  const timeline = liveRunMessage(store.messages, run)?.timeline ?? [];
  // 只要阶段文案；已过时间在列表里没有意义
  return liveWorkParts(timeline, 0, { language: store.data?.settings.language, status: run.status })[0]?.label ?? "";
});

const single = computed(() => (props.showName && props.fallback.length === 1 ? props.fallback[0] : null));
</script>

<template>
  <span v-if="liveRuns.length" class="agent-live-status is-live" aria-live="polite">
    <AvatarStack :items="liveItems" :max="max" :size="size" :dialog-title="dialogTitle" />
    <UiShimmerText class="agent-live-label" :text="liveLabel" rotate />
  </span>
  <span v-else-if="single" class="agent-live-status">
    <AgentAvatar :value="single.image || ''" :name="single.name" :size="size" />
    <span class="agent-live-label">{{ single.name }}</span>
  </span>
  <AvatarStack v-else :items="fallback" :max="max" :size="size" :dialog-title="dialogTitle" />
</template>
