<script setup lang="ts">
import { computed } from "vue";
import type { ChatMessage } from "../../domain/types";
import AgentAvatar from "../AgentAvatar.vue";
import { liveAssistantText } from "../../domain/assistant-text";
import { assignmentVisibleText, isLeaderAssignment } from "./message-speaker";
import type { SpeakerInfo } from "./message-speaker";
import FailureBubble from "./FailureBubble.vue";
import MessageFooter from "./MessageFooter.vue";
import ProcessFold from "./ProcessFold.vue";
import RichContent from "./RichContent.vue";
import { useAppStore } from "../../stores/app";
import { CONTINUE_ASSIGNMENT } from "../../domain/continue-task";
import { extractQuestions } from "./question-block";

const props = defineProps<{
  message: ChatMessage;
  speaker?: SpeakerInfo | null;
  duplicateCount?: number;
  running?: boolean;
  allowContinue?: boolean;
  questionState?: "pending" | "answered" | "skipped" | null;
}>();
const emit = defineEmits<{
  "pick-action": [prompt: string];
}>();
const store = useAppStore();
const liveText = computed(() => liveAssistantText(props.message.timeline));
const rawContent = computed(() =>
  props.running ? liveText.value || props.message.content : props.message.content || liveText.value,
);
const hidesAssignment = computed(() => isLeaderAssignment(props.message, store.data?.squads ?? []));
const bodyRaw = computed(() =>
  props.message.failureReason ? liveText.value : rawContent.value,
);
const extracted = computed(() => extractQuestions(bodyRaw.value));
const bodyText = computed(() => {
  if (!hidesAssignment.value) return extracted.value.body;
  return assignmentVisibleText(extracted.value.body);
});
/** 失败消息没有正文时，复制内容取失败摘要 + 技术详情 */
const footerText = computed(() => {
  if (bodyText.value.trim() || !props.message.failureReason) return bodyText.value;
  const summary = props.message.content || "这次运行没有成功完成。";
  return `${summary}\n\n${props.message.failureReason}`;
});
const questions = computed(() => extracted.value.questions);
const questionStateLabel = computed(() => {
  if (props.questionState === "answered") return "已回答";
  if (props.questionState === "skipped") return "已跳过";
  if (props.questionState === "pending") return "在输入框上方回答";
  return "";
});
const showAssignmentOriginal = computed(() =>
  hidesAssignment.value && bodyRaw.value.trim() !== bodyText.value.trim(),
);
</script>

<template>
  <article class="chat-turn assistant-turn" :class="{ internal: speaker?.internal }">
    <div v-if="speaker" class="speaker-row">
      <AgentAvatar
        class="assistant-avatar"
        :class="speaker.roleKind"
        :value="speaker.avatar"
        :name="speaker.name"
        :size="22"
      />
      <div class="speaker-meta">
        <b>{{ speaker.name }}</b>
        <span class="speaker-pill" :class="speaker.roleKind">{{ speaker.roleLabel }}</span>
        <span v-if="message.failureReason" class="speaker-pill fail">失败</span>
      </div>
    </div>
    <p v-if="speaker?.internal && speaker.audienceName" class="to-member">
      <em>@{{ speaker.audienceName }}</em>
      <template v-if="speaker.audienceRole">（{{ speaker.audienceRole }}）</template>
    </p>
    <div class="assistant-main">
      <ProcessFold
        :items="message.timeline"
        :running="running"
        :elapsed-ms="message.elapsedMs"
      />
      <RichContent v-if="bodyText.trim()" :content="bodyText" />
      <p v-if="running && extracted.streaming" class="question-summary-pending">正在整理要问你的问题…</p>
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
        {{ message.content || "运行已结束，但没有返回可展示的文本。" }}
      </div>
      <details v-if="showAssignmentOriginal" class="assignment-original">
        <summary>查看派活原文</summary>
        <pre>{{ bodyRaw }}</pre>
      </details>
      <MessageFooter
        v-if="!running && (bodyText.trim() || message.failureReason)"
        :content="footerText"
        :created-at="message.createdAt"
        :elapsed-ms="message.timeline.length ? null : message.elapsedMs"
        :message-id="message.id"
        :feedback="message.feedback"
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
