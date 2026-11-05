<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import type { ChatMessage } from "../../domain/types";
import AgentAvatar from "../AgentAvatar.vue";
import AgentLiveStatus from "../AgentLiveStatus.vue";
import { liveAssistantText } from "../../domain/assistant-text";
import {
  assignmentVisibleText,
  isLeaderAssignment,
  parseAssignmentSubtask,
  type AssignmentSubtaskRef,
} from "./message-speaker";
import type { SpeakerInfo } from "./message-speaker";
import FailureBubble from "./FailureBubble.vue";
import MessageFooter from "./MessageFooter.vue";
import ProcessFold from "./ProcessFold.vue";
import RichContent from "./RichContent.vue";
import PlanArtifactCard from "./PlanArtifactCard.vue";
import SubtaskReplyEmbed from "./SubtaskReplyEmbed.vue";
import { plansInMessage } from "./plan-state";
import { useAppStore } from "../../stores/app";
import { CONTINUE_ASSIGNMENT } from "../../domain/continue-task";
import { extractQuestions } from "./question-block";
import { t } from "../../i18n";

const props = withDefaults(defineProps<{
  message: ChatMessage;
  speaker?: SpeakerInfo | null;
  duplicateCount?: number;
  running?: boolean;
  allowContinue?: boolean;
  allowFork?: boolean;
  questionState?: "pending" | "answered" | "skipped" | null;
  /** 回看场景（如收件箱）：子代理默认全折叠，只显示摘要 */
  defaultCollapseSubagents?: boolean;
  /** 窄布局：计划卡片只显示标题、摘要和步骤数 */
  compact?: boolean;
  /** 同一 exchange 连续同说话人时隐藏头像行，减少重复 */
  hideSpeaker?: boolean;
  /** 派活后第一条回复下方嵌子任务回复 */
  embedSubtask?: AssignmentSubtaskRef | null;
}>(), {
  allowFork: false,
  compact: false,
  hideSpeaker: false,
  embedSubtask: null,
});
const emit = defineEmits<{
  "pick-action": [prompt: string];
}>();
const store = useAppStore();
const liveText = computed(() => liveAssistantText(props.message.timeline));
const rawContent = computed(() =>
  props.running ? liveText.value || props.message.content : props.message.content || liveText.value,
);
const hidesAssignment = computed(() => isLeaderAssignment(props.message));
const bodyRaw = computed(() =>
  props.message.failureReason ? liveText.value : rawContent.value,
);
const extracted = computed(() => extractQuestions(bodyRaw.value));
const bodyText = computed(() => {
  if (!hidesAssignment.value) return extracted.value.body;
  // 有子任务链接时不再复述「请处理子任务…」全文
  if (parseAssignmentSubtask(extracted.value.body || bodyRaw.value)) return "";
  return assignmentVisibleText(extracted.value.body);
});
/** 失败消息没有正文时，复制内容取失败摘要 + 技术详情 */
const footerText = computed(() => {
  if (bodyText.value.trim() || !props.message.failureReason) {
    if (hidesAssignment.value && assignmentSubtask.value) {
      return assignmentSubtask.value.title;
    }
    return bodyText.value;
  }
  const summary = props.message.content || t("chat.runFailedSummary");
  return `${summary}\n\n${props.message.failureReason}`;
});
const questions = computed(() => extracted.value.questions);
const questionStateLabel = computed(() => {
  if (props.questionState === "answered") return t("chat.questionState.answered");
  if (props.questionState === "skipped") return t("chat.questionState.skipped");
  if (props.questionState === "pending") return t("chat.questionState.pending");
  return "";
});
const plans = computed(() => plansInMessage(store.planStates(props.message.sessionId), props.message.id));
const assignmentSubtask = computed(() =>
  hidesAssignment.value ? parseAssignmentSubtask(bodyRaw.value) : null,
);
const assignedTask = computed(() => {
  const id = assignmentSubtask.value?.id;
  if (!id) return null;
  return store.data?.tasks.find((item) => item.id === id) ?? null;
});
const assignedAgent = computed(() => {
  const id = assignedTask.value?.agentId || props.message.audienceId || props.speaker?.agentId;
  if (!id) return null;
  return store.data?.agents.find((item) => item.id === id) ?? null;
});
const assignedFallback = computed(() => {
  const agent = assignedAgent.value;
  if (!agent) {
    if (!props.speaker?.audienceName) return [];
    return [{
      key: props.message.audienceId || "assignee",
      name: props.speaker.audienceName,
      image: "",
    }];
  }
  return [{ key: agent.id, name: agent.name, image: agent.avatar }];
});
const assignedSessionIds = computed(() =>
  assignedTask.value?.sessionId ? [assignedTask.value.sessionId] : [],
);
const audienceLabel = computed(() =>
  props.speaker?.audienceName
    ?? assignedAgent.value?.name
    ?? null,
);
const isInternalReply = computed(() =>
  !hidesAssignment.value
  && props.message.audienceType === "agent"
  && Boolean(props.message.audienceId)
  && Boolean(props.message.agentId)
  && !props.message.failureReason,
);
const assignedName = computed(() => assignedFallback.value[0]?.name ?? audienceLabel.value ?? "");
/** 派给对方的话：优先用子任务描述，没有再退回标题。 */
const assignmentText = computed(() => {
  const description = assignedTask.value?.description?.trim();
  if (description) return description;
  if (assignmentSubtask.value) return assignmentSubtask.value.title;
  // @ 派活没有子任务：指令正文并排在 @名字 后面
  return bodyText.value.trim();
});
const showSpeaker = computed(() => Boolean(props.speaker) && !props.hideSpeaker);
const isInternal = computed(() =>
  Boolean(props.speaker?.internal) || hidesAssignment.value,
);
</script>

<template>
  <!-- 派活：A 头像 + @B + 子任务链接，与小队内部互 @ 同一套对话样式 -->
  <article
    v-if="hidesAssignment"
    class="chat-turn assistant-turn internal assignment-turn"
  >
    <div v-if="showSpeaker" class="speaker-row">
      <AgentAvatar
        class="assistant-avatar"
        :class="speaker?.roleKind"
        :value="speaker?.avatar"
        :name="speaker?.name"
        :size="22"
      />
      <div class="speaker-meta">
        <b>{{ speaker?.name }}</b>
        <span class="speaker-pill" :class="speaker?.roleKind">{{ speaker?.roleLabel }}</span>
      </div>
    </div>
    <div class="mention-line">
      <span class="at-name">@{{ assignedName }}</span>
      <span v-if="assignmentText" class="assignment-instruction">{{ assignmentText }}</span>
    </div>
    <RouterLink
      v-if="assignmentSubtask"
      class="assignment-subtask"
      :to="`/tasks/${assignmentSubtask.id}`"
    >
      <span class="assignment-subtask-kicker">{{ t("chat.assignmentSubtask") }}</span>
      <span class="assignment-subtask-title">{{ assignmentSubtask.title }}</span>
    </RouterLink>
      </article>

  <article v-else class="chat-turn assistant-turn" :class="{ internal: isInternal }">
    <div v-if="showSpeaker" class="speaker-row">
      <AgentAvatar
        class="assistant-avatar"
        :class="speaker?.roleKind"
        :value="speaker?.avatar"
        :name="speaker?.name"
        :size="22"
      />
      <div class="speaker-meta">
        <b>{{ speaker?.name }}</b>
        <span class="speaker-pill" :class="speaker?.roleKind">{{ speaker?.roleLabel }}</span>
        <span v-if="message.failureReason" class="speaker-pill fail">{{ t("chat.failPill") }}</span>
      </div>
    </div>
    <div v-if="isInternalReply" class="mention-line reply-text">
      <span v-if="audienceLabel" class="at-name">@{{ audienceLabel }}</span>
      <RichContent v-if="bodyText.trim()" :content="bodyText" />
    </div>
    <div class="assistant-main">
      <ProcessFold
        :items="message.timeline"
        :running="running"
        :elapsed-ms="message.elapsedMs"
        :run-id="message.runId"
        :default-collapse-subagents="defaultCollapseSubagents"
      />
      <RichContent v-if="bodyText.trim() && !isInternalReply" :content="bodyText" />
      <SubtaskReplyEmbed v-if="embedSubtask" :subtask="embedSubtask" />
      <PlanArtifactCard
        v-for="entry in plans"
        :key="entry.key"
        :entry="entry"
        :compact="compact"
        :session-id="message.sessionId"
      />
      <p v-if="running && extracted.streaming" class="question-summary-pending">{{ t("chat.questionPreparing") }}</p>
      <div v-else-if="questions.length && !running" class="question-summary" :class="questionState ?? ''">
        <div v-for="question in questions" :key="question.id" class="question-summary-item">
          <span class="question-summary-q">{{ question.question }}</span>
          <span v-if="question.options.length" class="question-summary-options">
            {{ question.options.map((option) => option.label).join(" / ") }}
          </span>
        </div>
        <span v-if="questionStateLabel" class="question-summary-state">{{ questionStateLabel }}</span>
      </div>
      <FailureBubble
        v-if="message.failureReason"
        :message="message"
        :duplicate-count="duplicateCount"
        :allow-continue="allowContinue && !running"
        @continue="emit('pick-action', CONTINUE_ASSIGNMENT)"
      />
      <div v-else-if="message.messageKind === 'no_response'" class="empty-answer">
        {{ message.content || t("chat.runNoText") }}
      </div>
      <MessageFooter
        v-if="!running && (bodyText.trim() || message.failureReason)"
        :content="footerText"
        :created-at="message.createdAt"
        :elapsed-ms="message.timeline.length ? null : message.elapsedMs"
        :session-id="message.sessionId"
        :message-id="message.id"
        :allow-fork="allowFork"
      />
      <div v-if="message.quickActions.length && !running" class="quick-actions">
        <UiButton
          v-for="action in message.quickActions"
          :key="action.label"
          :class="{ primary: action.primary }"
          type="button"
          @click="emit('pick-action', action.prompt)"
        >{{ action.label }}</UiButton>
      </div>
    </div>
  </article>
</template>
