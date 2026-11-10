<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import type { ChatMessage, TimelineItem } from "../../domain/types";
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
import { GENUI_MAX_BLOCKS, isGenUiEnabled, splitGenUi } from "./genui/block";
import GenUiBlock from "./genui/GenUiBlock.vue";
import { genUiSendingIds, genUiSubmissions, sendGenUiAction } from "./ui-outbound";
import { toolResultUis } from "./genui/tool-ui";
import McpAppFrame from "./McpAppFrame.vue";
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
  /** 界面块能否提交（预览、只读回看时为 false） */
  uiInteractive?: boolean;
}>(), {
  allowFork: false,
  compact: false,
  hideSpeaker: false,
  embedSubtask: null,
  uiInteractive: true,
});
const emit = defineEmits<{
  "pick-action": [prompt: string];
}>();
const store = useAppStore();
// 生成式界面开关：提问别名和界面块渲染都跟它走
const genUiEnabled = computed(() => isGenUiEnabled(store.data?.settings));
const liveText = computed(() => liveAssistantText(props.message.timeline));
const rawContent = computed(() =>
  props.running ? liveText.value || props.message.content : props.message.content || liveText.value,
);
const hidesAssignment = computed(() => isLeaderAssignment(props.message));
const bodyRaw = computed(() =>
  props.message.failureReason ? liveText.value : rawContent.value,
);
const extracted = computed(() => extractQuestions(bodyRaw.value, { genui: genUiEnabled.value }));
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

// 生成式界面（实验功能）：正文按 ```multara-ui 切片，界面块原地换成组件。关闭时整段仍按 Markdown 渲染。
const genUiSegments = computed(() => {
  if (!genUiEnabled.value || isInternalReply.value) return null;
  const { segments } = splitGenUi(bodyText.value);
  return segments.some((segment) => segment.kind !== "md") ? segments : null;
});
const genUiSubmitted = computed(() =>
  genUiSegments.value
    ? genUiSubmissions(store.messages[props.message.sessionId] ?? [], props.message.id)
    : new Map(),
);
/** 生成式界面开关打开时，`multara ... --ui` 的工具结果在正文下方显示成界面块 */
const toolUis = computed(() => (genUiEnabled.value ? toolResultUis(props.message.timeline) : []));
/** MCP Apps：MCP 工具调用（mcp__<server>__<tool>）；有没有界面由 McpAppFrame 自己去问 */
const mcpAppsOn = computed(() => genUiEnabled.value && store.data?.settings.mcpAppsEnabled === "true");
const mcpCalls = computed(() => {
  if (!mcpAppsOn.value) return [];
  const results = new Map<string, TimelineItem>();
  for (const item of props.message.timeline) {
    if (item.eventType === "tool_result" && item.callId) results.set(item.callId, item);
  }
  return props.message.timeline
    .filter((item) => item.eventType === "tool_use" && item.callId && item.tool?.startsWith("mcp__"))
    .map((use) => ({ use, result: results.get(use.callId as string) ?? null }));
});
/**
 * 点了、还在发送队列里的块：点击后立即置灰，防止连点发多条。
 * 从队列推出来而不是另存：发送成功后乐观消息接手（变成已提交），失败时出队，按钮自动恢复。
 */
const genUiSending = computed(() => genUiSendingIds(store.queuedMessages, props.message.id));
const genUiBusy = computed(() => Boolean(props.running) || store.sessionProcessing(props.message.sessionId));
const genUiDisabled = computed(() => !props.uiInteractive || genUiBusy.value);
const genUiDisabledReason = computed(() =>
  !props.uiInteractive ? t("genui.readonly") : genUiBusy.value ? t("genui.busy") : "",
);

function onGenUiAction(payload: { blockId: string; type: string; title?: string; action: string; values?: unknown }) {
  if (genUiDisabled.value || genUiSending.value.has(payload.blockId) || genUiSubmitted.value.has(payload.blockId)) return;
  sendGenUiAction(
    // priority：上方若还有没回答的提问，排队会停住；界面块提交本身就是用户的回答
    (content, sessionId, attachments, options) => store.send(content, sessionId, attachments, { ...options, priority: true }),
    { sessionId: props.message.sessionId, messageId: props.message.id, agentId: props.message.agentId, ...payload },
  );
}
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
      <template v-if="genUiSegments">
        <template v-for="(segment, index) in genUiSegments" :key="segment.kind === 'md' ? `md-${index}` : `ui-${segment.index}`">
          <RichContent v-if="segment.kind === 'md'" v-show="segment.text.trim()" :content="segment.text" />
          <GenUiBlock
            v-else
            :segment="segment"
            :message-id="message.id"
            :state-id="message.runId || message.id"
            :over-limit="segment.index >= GENUI_MAX_BLOCKS"
            :submissions="genUiSubmitted"
            :sending="genUiSending"
            :disabled="genUiDisabled"
            :disabled-reason="genUiDisabledReason"
            :session-id="uiInteractive ? message.sessionId : undefined"
            @action="onGenUiAction"
          />
        </template>
      </template>
      <RichContent v-else-if="bodyText.trim() && !isInternalReply" :content="bodyText" />
      <!-- 工具结果带 UI：multara ... --ui 的输出提到正文下方，工具本身仍在过程里折叠 -->
      <div v-for="(tool, index) in toolUis" :key="tool.callId" class="tool-ui">
        <GenUiBlock
          :segment="{ kind: 'ui', index: 100 + index, raw: tool.raw, source: tool.raw, specs: [tool.spec] }"
          :message-id="message.id"
          :state-id="`${message.runId || message.id}-tool`"
          :disabled="true"
          :disabled-reason="t('genui.readonly')"
        />
        <details class="tool-ui-raw">
          <summary>{{ t("genui.viewRawOutput") }}</summary>
          <pre><code>$ {{ tool.command }}
{{ tool.raw }}</code></pre>
        </details>
      </div>
      <McpAppFrame
        v-for="call in mcpCalls"
        :key="call.use.callId ?? call.use.seq"
        :session-id="message.sessionId"
        :tool-use="call.use"
        :tool-result="call.result"
      />
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
