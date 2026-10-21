<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { ArrowUp, Square } from "@lucide/vue";
import type { ChatMessage, MessageAttachmentInput } from "../domain/types";
import AssistantMessage from "../components/chat/AssistantMessage.vue";
import ChatComposer from "../components/chat/ChatComposer.vue";
import ChatQuestionCarousel from "../components/chat/ChatQuestionCarousel.vue";
import ComposerPasteImages from "../components/chat/ComposerPasteImages.vue";
import UserPrompt from "../components/chat/UserPrompt.vue";
import { extractQuestions } from "../components/chat/question-block";
import { formatAgentPrompt, type ComposerNode } from "../components/chat/composer-content";
import { scrollListToMessage } from "../components/chat/scroll-to-message";
import { speakerForMessage } from "../components/chat/message-speaker";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";

const props = withDefaults(defineProps<{
  sessionId: string;
  agentId: string;
  thread?: boolean;
  historyViewOnly?: boolean;
  unreadMessageId?: string | null;
}>(), {
  thread: false,
  historyViewOnly: false,
  unreadMessageId: null,
});

const store = useAppStore();
const logEl = ref<HTMLElement | null>(null);
const composer = ref<{
  clear: () => void;
  serialize: () => { text: string; nodes: ComposerNode[]; empty: boolean };
} | null>(null);
const draftEmpty = ref(true);
const pasteError = ref("");
const pastes = ref<Array<MessageAttachmentInput & { previewUrl: string }>>([]);
const emptyNodes: ComposerNode[] = [{ type: "text", value: "" }];

const messages = computed(() => store.messages[props.sessionId] ?? []);
const agent = computed(() => store.data?.agents.find((item) => item.id === props.agentId));
const sessionRunning = computed(() =>
  Object.values(store.runStates).some((event) =>
    event.sessionId === props.sessionId && (event.status === "queued" || event.status === "running"),
  ),
);
const canSend = computed(() => !draftEmpty.value || pastes.value.length > 0);
const pendingQuestion = computed(() => {
  if (sessionRunning.value || props.historyViewOnly || !store.awaitingAnswer(props.sessionId)) return null;
  for (let index = messages.value.length - 1; index >= 0; index -= 1) {
    const message = messages.value[index];
    if (message.role === "user") return null;
    if (message.role !== "assistant" || message.failureReason) continue;
    const questions = extractQuestions(message.content).questions;
    if (!questions.length) continue;
    return { messageId: message.id, questions, agentName: agent.value?.name ?? "智能体" };
  }
  return null;
});

function answerQuestion(text: string) {
  if (!pendingQuestion.value || !text.trim()) return;
  store.send(text, props.sessionId, [], { priority: true });
}

function skipQuestion() {
  if (!pendingQuestion.value) return;
  store.dismissQuestion(pendingQuestion.value.messageId);
}

function speaker(message: ChatMessage) {
  return speakerForMessage(message, store.data?.agents ?? [], store.data?.squads ?? [], props.agentId);
}

function messageRunning(message: ChatMessage) {
  return message.runId ? store.runStates[message.runId]?.status === "running" : false;
}

function onChange(nodes: ComposerNode[]) {
  draftEmpty.value = !formatAgentPrompt(nodes).trim();
}

async function pasteImages(files: File[]) {
  pasteError.value = "";
  const room = Math.max(0, 8 - pastes.value.length);
  for (const file of files.slice(0, room)) {
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      if (bytes.length > 25 * 1024 * 1024) {
        pasteError.value = "粘贴的图片超过 25 MB";
        continue;
      }
      const saved = await backend.savePastedAttachment(bytes, file.type || "image/png", file.name || undefined);
      pastes.value.push({ ...saved, previewUrl: URL.createObjectURL(file) });
    } catch (cause) {
      pasteError.value = cause instanceof Error ? cause.message : "粘贴图片失败";
    }
  }
}

function removePaste(path: string) {
  const target = pastes.value.find((item) => item.path === path);
  if (target) URL.revokeObjectURL(target.previewUrl);
  pastes.value = pastes.value.filter((item) => item.path !== path);
}

function submit() {
  const text = formatAgentPrompt(composer.value?.serialize().nodes ?? []).trim();
  if (!text && !pastes.value.length) return;
  const attachments = pastes.value.map(({ name, path, mimeType, size }) => ({ name, path, mimeType, size }));
  store.send(text, props.sessionId, attachments);
  for (const image of pastes.value) URL.revokeObjectURL(image.previewUrl);
  pastes.value = [];
  composer.value?.clear();
  draftEmpty.value = true;
}

function stop() {
  void store.stopSessionRuns(props.sessionId);
}

function revealUnread() {
  if (!props.unreadMessageId) return;
  if (scrollListToMessage(logEl.value, props.unreadMessageId)) return;
}

onMounted(() => {
  void store.loadMessages(props.sessionId);
});

watch(() => props.sessionId, (sessionId) => {
  void store.loadMessages(sessionId);
});

watch(
  () => [props.unreadMessageId, messages.value.length] as const,
  async ([messageId]) => {
    if (!messageId) return;
    await nextTick();
    requestAnimationFrame(() => revealUnread());
  },
  { immediate: true },
);
</script>

<template>
  <div class="task-activity" :class="{ 'is-thread': thread }">
    <div v-if="messages.length" ref="logEl" class="task-activity-log vs-scroll">
      <template v-for="message in messages" :key="message.id">
        <div
          v-if="message.id === unreadMessageId"
          class="chat-new-divider"
          role="separator"
          :data-message-id="message.id"
        >
          <span>新消息</span>
        </div>
        <UserPrompt
          v-if="message.role === 'user'"
          :data-message-id="message.id"
          :message="message"
          :target-label="agent?.name ?? '智能体'"
          :target-avatar="agent?.avatar ?? 'M'"
          :target-value="agent ? `agent:${agent.id}` : ''"
          :allow-target-change="false"
          :enable-replay="false"
          :view-only="historyViewOnly"
          :running="!historyViewOnly && sessionRunning"
          @stop="stop"
        />
        <AssistantMessage
          v-else
          :data-message-id="message.id"
          :message="message"
          :speaker="speaker(message)"
          :running="messageRunning(message)"
          :allow-continue="false"
        />
      </template>
    </div>
    <p v-else class="task-activity-empty">还没有动态。发一条消息后，智能体会在这个任务里回复。</p>
    <p v-if="pasteError" class="task-create-error">{{ pasteError }}</p>
    <ChatQuestionCarousel
      v-if="pendingQuestion"
      :key="pendingQuestion.messageId"
      :questions="pendingQuestion.questions"
      :agent-name="pendingQuestion.agentName"
      @submit="answerQuestion"
      @skip="skipQuestion"
    />
    <ChatComposer
      ref="composer"
      placeholder="回复…"
      :initial-nodes="emptyNodes"
      :contexts="[]"
      :target-label="agent?.name ?? '未分配'"
      :target-avatar="agent?.avatar ?? 'M'"
      :allow-target-change="false"
      :show-context-usage="false"
      :can-send="canSend"
      @change="onChange"
      @submit="submit"
      @paste-image="pasteImages"
    >
      <ComposerPasteImages
        :images="pastes.map((image) => ({ key: image.path, src: image.previewUrl, name: image.name }))"
        removable
        @remove="removePaste"
      />
      <template #actions>
        <UiButton v-if="sessionRunning && !canSend" class="icon-button" type="button" title="停止" @click="stop">
          <Square :size="14" />
        </UiButton>
        <UiButton v-else class="ds-send-button send-button" type="submit" :title="sessionRunning ? '加入队列' : '发送'" :disabled="!canSend">
          <ArrowUp :size="16" />
        </UiButton>
      </template>
    </ChatComposer>
  </div>
</template>
