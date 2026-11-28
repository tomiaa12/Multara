<script setup lang="ts">
// 智能体状态格：有智能体在跑时显示「头像 + 轮播动作文案」，多个同时跑时显示堆叠头像 + 「N 个智能体处理中」；
// 没在跑时：头像 +「名称 3 小时前」（单人+idleLabel），或多人头像 + 相对时间；仅 showName 时为头像+名称。
import { computed, onUnmounted, ref, watch } from "vue";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import AgentAvatar from "./AgentAvatar.vue";
import AvatarStack, { type AvatarStackItem } from "./ui/AvatarStack.vue";
import {
  collectLiveRuns,
  liveCapacity,
  liveRunMessage,
  resolveRunLiveWorkStatus,
  retryItemFromRun,
} from "./agent-live-status";
import { latestRetrySignal, liveWorkParts } from "./chat/timeline-display";

const props = withDefaults(defineProps<{
  /** 这格对应的会话：需求下所有会话 / 任务自己的会话 */
  sessionIds: string[];
  /** 没有在跑的智能体时展示的人 */
  fallback?: AvatarStackItem[];
  /** 空闲相对时间；单人头像时会拼成「名称 + 时间」 */
  idleLabel?: string;
  /** 无人在跑且无 idleLabel 时，单人头像旁显示名字 */
  showName?: boolean;
  max?: number;
  size?: number;
  dialogTitle?: string;
}>(), {
  fallback: () => [],
  idleLabel: "",
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

const nowMs = ref(Date.now());
let ticker = 0;
watch(() => liveRuns.value.length, (count) => {
  window.clearInterval(ticker);
  if (!count) return;
  nowMs.value = Date.now();
  ticker = window.setInterval(() => { nowMs.value = Date.now(); }, 5000);
}, { immediate: true });
onUnmounted(() => window.clearInterval(ticker));

const liveLabel = computed(() => {
  const runs = liveRuns.value;
  if (runs.length > 1) return t("agentLive.multi", { n: runs.length });
  const run = runs[0];
  if (!run) return "";
  const state = store.runStates[run.runId];
  const messageTimeline = liveRunMessage(store.messages, run)?.timeline ?? [];
  const timeline = messageTimeline.length
    ? messageTimeline
    : state?.timelineItem ? [state.timelineItem] : [];
  // 与对话内工作行共用：绑定运行时的名额满了才是排队中
  const status = resolveRunLiveWorkStatus(run, store.runStates, liveCapacity(store.data));
  const idleSeconds = state?.lastEventAtMs
    ? Math.max(0, (nowMs.value - state.lastEventAtMs) / 1000)
    : undefined;
  const retryItem = retryItemFromRun(store.runStates, run.runId);
  return liveWorkParts(timeline, 0, {
    language: store.data?.settings.language,
    status,
    idleSeconds,
    retrying: retryItem ? latestRetrySignal([retryItem]) : null,
  })[0]?.label ?? "";
});

const idleText = computed(() => props.idleLabel.trim());
const idleCaption = computed(() => {
  const time = idleText.value;
  const name = props.fallback.length === 1 ? props.fallback[0]?.name?.trim() : "";
  if (name && time) return `${name} ${time}`;
  if (time) return time;
  if (props.showName && name) return name;
  return "";
});
const single = computed(() => (
  !idleCaption.value && props.showName && props.fallback.length === 1 ? props.fallback[0] : null
));
</script>

<template>
  <span v-if="liveRuns.length" class="agent-live-status is-live" aria-live="polite">
    <AvatarStack :items="liveItems" :max="max" :size="size" :dialog-title="dialogTitle" />
    <UiShimmerText class="agent-live-label" :text="liveLabel" rotate />
  </span>
  <span v-else-if="fallback.length && idleCaption" class="agent-live-status">
    <AvatarStack :items="fallback" :max="max" :size="size" :dialog-title="dialogTitle" />
    <span class="agent-live-label">{{ idleCaption }}</span>
  </span>
  <span v-else-if="single" class="agent-live-status">
    <AgentAvatar :value="single.image || ''" :name="single.name" :size="size" />
    <span class="agent-live-label">{{ single.name }}</span>
  </span>
  <AvatarStack v-else :items="fallback" :max="max" :size="size" :dialog-title="dialogTitle" />
</template>
