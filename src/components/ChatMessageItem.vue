<script setup lang="ts">
import { computed } from "vue";
import type { ChatMessage, MessageAttachmentInput } from "../domain/types";
import AssistantMessage from "./chat/AssistantMessage.vue";
import { sentTargetForMessage, speakerForMessage } from "./chat/message-speaker";
import UserPrompt from "./chat/UserPrompt.vue";
import { useAppStore } from "../stores/app";
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

function startReask(
  messageId: string,
  content: string,
  attachments: MessageAttachmentInput[],
) {
  emit("start-reask", messageId, content, attachments);
}
</script>

<template>
  <UserPrompt
    v-if="message.role === 'user'"
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
