<script setup lang="ts">
import { computed } from "vue";
import type { ChatMessage, MessageAttachmentInput } from "../domain/types";
import AssistantMessage from "./chat/AssistantMessage.vue";
import { sentTargetForMessage, speakerForMessage } from "./chat/message-speaker";
import UserPrompt from "./chat/UserPrompt.vue";
import { useAppStore } from "../stores/app";
import { t } from "../i18n";
import type { SelectOption } from "./ui/select-types";

const props = withDefaults(defineProps<{
  message: ChatMessage;
  duplicateCount?: number;
  running?: boolean;
  targetLabel?: string;
  targetAvatar?: string;
  targetValue?: string;
  targetOptions?: SelectOption<string>[];
  allowTargetChange?: boolean;
  enableReplay?: boolean;
  viewOnly?: boolean;
  allowContinue?: boolean;
  restoreNonce?: number;
  restoreMessageId?: string;
  turnRunning?: boolean;
  questionState?: "pending" | "answered" | "skipped" | null;
}>(), {
  questionState: null,
  duplicateCount: 1,
  targetLabel: "智能体",
  targetAvatar: "M",
  targetValue: "",
  targetOptions: () => [],
  allowTargetChange: true,
  enableReplay: true,
  viewOnly: false,
  allowContinue: true,
  restoreNonce: 0,
  restoreMessageId: "",
  turnRunning: false,
});
const emit = defineEmits<{
  "start-reask": [messageId: string, content: string, attachments: MessageAttachmentInput[]];
  "target-change": [value: string];
  "pick-action": [prompt: string];
  stop: [];
}>();

const store = useAppStore();
const sessionMessages = computed(() => store.messages[props.message.sessionId] ?? []);
const messageSession = computed(() =>
  store.data?.sessions.find((item) => item.id === props.message.sessionId) ?? store.activeSession,
);
const speaker = computed(() =>
  speakerForMessage(
    props.message,
    store.data?.agents ?? [],
    store.data?.squads ?? [],
    undefined,
    { messages: sessionMessages.value, session: messageSession.value },
  ),
);
const sentTarget = computed(() =>
  sentTargetForMessage(
    props.message,
    store.data?.agents ?? [],
    store.data?.squads ?? [],
    messageSession.value,
  ),
);

const channelSource = computed(() => {
  if (!props.message.sourceType) return "";
  const meta = (props.message.sourceMeta ?? {}) as Record<string, unknown>;
  const channel = props.message.sourceType === "lark"
    ? "Lark"
    : props.message.sourceType === "feishu"
      ? t("channels.feishu")
      : props.message.sourceType;
  const sender = String(meta.senderName ?? meta.senderId ?? "").trim();
  return sender ? t("chat.fromChannel", { channel, sender }) : channel;
});

function startReask(
  messageId: string,
  content: string,
  attachments: MessageAttachmentInput[],
) {
  emit("start-reask", messageId, content, attachments);
}
</script>

<template>
  <div v-if="message.role === 'user'">
    <div v-if="channelSource" class="channel-message-source">{{ channelSource }}</div>
    <UserPrompt
      :message="message"
      :target-label="sentTarget?.label ?? targetLabel"
      :target-avatar="sentTarget?.avatar ?? targetAvatar"
      :target-value="sentTarget?.value ?? targetValue"
      :target-options="targetOptions"
      :allow-target-change="false"
      :enable-replay="enableReplay && !viewOnly"
      :view-only="viewOnly"
      :restore-nonce="restoreNonce"
      :restore-message-id="restoreMessageId"
      :running="turnRunning"
      @start-reask="startReask"
      @stop="emit('stop')"
      @target-change="emit('target-change', $event)"
    />
  </div>
  <AssistantMessage
    v-else
    :message="message"
    :speaker="speaker"
    :duplicate-count="duplicateCount"
    :running="running"
    :allow-continue="allowContinue"
    :question-state="questionState"
    @pick-action="emit('pick-action', $event)"
  />
</template>

<style scoped>
.channel-message-source {
  margin: 0 0 4px 12px;
  font-size: 11px;
  color: var(--text-muted);
}
</style>
