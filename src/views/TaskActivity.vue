<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ArrowUp, LoaderCircle, Square } from "@lucide/vue";
import type { ChatMessage, MessageAttachmentInput } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import AssistantMessage from "../components/chat/AssistantMessage.vue";
import ChatComposer from "../components/chat/ChatComposer.vue";
import ChatQuestionCarousel from "../components/chat/ChatQuestionCarousel.vue";
import ComposerPasteImages from "../components/chat/ComposerPasteImages.vue";
import UserPrompt from "../components/chat/UserPrompt.vue";
import { extractQuestions } from "../components/chat/question-block";
import { formatAgentPrompt, type ComposerNode } from "../components/chat/composer-content";
import { scrollListToMessage } from "../components/chat/scroll-to-message";
import { speakerForMessage } from "../components/chat/message-speaker";
import { liveWorkParts } from "../components/chat/timeline-display";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";
import { t } from "../i18n";

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
const liveClock = ref(Date.now());
const liveAnchorFallback = ref<number | null>(null);
let liveTimer = 0;

const messages = computed(() => store.messages[props.sessionId] ?? []);
const agent = computed(() => store.data?.agents.find((item) => item.id === props.agentId));
const sessionRunning = computed(() =>
  Object.values(store.runStates).some((event) =>
    event.sessionId === props.sessionId
    && (event.status === "queued" || event.status === "running")
    && !event.runId.startsWith("pending:"),
  ),
);
/** 含发送中：任务会话不一定等于 activeSessionId，不能照搬 ChatPanel。 */
const sessionProcessing = computed(() => {
  if (sessionRunning.value) return true;
  if (store.queuedMessages.some((item) => item.sessionId === props.sessionId)) return true;
  if (!store.processingMessage) return false;
  const last = messages.value[messages.value.length - 1];
  return last?.sessionId === props.sessionId && last.role === "user";
});
const workingAgents = computed(() => {
  const seen = new Set<string>();
  const agents: Array<{ id: string; name: string; avatar: string }> = [];
  for (const event of Object.values(store.runStates)) {
    if (event.sessionId !== props.sessionId) continue;
    if (event.status !== "queued" && event.status !== "running") continue;
    if (!event.agentId || seen.has(event.agentId)) continue;
    seen.add(event.agentId);
    const found = store.data?.agents.find((item) => item.id === event.agentId);
    agents.push({
      id: event.agentId,
      name: found?.name ?? agent.value?.name ?? t("inbox.agentFallback"),
      avatar: found?.avatar ?? agent.value?.avatar ?? "M",
    });
  }
  if (!agents.length && agent.value) {
    agents.push({ id: agent.value.id, name: agent.value.name, avatar: agent.value.avatar });
  }
  return agents;
});
const currentTurnAssistant = computed(() => {
  const list = messages.value;
  let lastUser = -1;
  for (let index = list.length - 1; index >= 0; index -= 1) {
    if (list[index]?.role === "user") {
      lastUser = index;
      break;
    }
  }
  const afterUser = lastUser >= 0 ? list.slice(lastUser + 1) : list;
  for (let index = afterUser.length - 1; index >= 0; index -= 1) {
    if (afterUser[index]?.role === "assistant") return afterUser[index];
  }
  return null;
});
const liveRunStatus = computed(() => {
  const events = Object.values(store.runStates).filter((event) =>
    event.sessionId === props.sessionId
    && (event.status === "queued" || event.status === "running")
    && !event.runId.startsWith("pending:"),
  );
  if (events.some((event) => event.status === "running")) return "running" as const;
  if (events.some((event) => event.status === "queued")) return "queued" as const;
  if (sessionProcessing.value) return "sending" as const;
  return null;
});
const liveAnchor = computed(() => {
  const created = Date.parse(currentTurnAssistant.value?.createdAt ?? "");
  if (Number.isFinite(created) && created > 0 && created <= liveClock.value + 1000) return created;
  return liveAnchorFallback.value ?? liveClock.value;
});
const showWorking = computed(() => !props.historyViewOnly && sessionProcessing.value);
const workingLines = computed(() => liveWorkParts(
  currentTurnAssistant.value?.timeline ?? [],
  Math.max(0, Math.floor((liveClock.value - liveAnchor.value) / 1000)),
  { language: store.data?.settings.language, status: liveRunStatus.value },
));
watch(sessionProcessing, (active) => {
  window.clearInterval(liveTimer);
  if (!active) {
    liveAnchorFallback.value = null;
    return;
  }
  if (liveAnchorFallback.value == null) liveAnchorFallback.value = Date.now();
  liveClock.value = Date.now();
  liveTimer = window.setInterval(() => {
    liveClock.value = Date.now();
  }, 1000);
}, { immediate: true });
onBeforeUnmount(() => window.clearInterval(liveTimer));
const canSend = computed(() => !draftEmpty.value || pastes.value.length > 0);
const pendingQuestion = computed(() => {
  if (sessionProcessing.value || props.historyViewOnly || !store.awaitingAnswer(props.sessionId)) return null;
  for (let index = messages.value.length - 1; index >= 0; index -= 1) {
    const message = messages.value[index];
    if (message.role === "user") return null;
    if (message.role !== "assistant" || message.failureReason) continue;
    const questions = extractQuestions(message.content).questions;
    if (!questions.length) continue;
    return { messageId: message.id, questions, agentName: agent.value?.name ?? t("inbox.agentFallback") };
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
  return speakerForMessage(
    message,
    store.data?.agents ?? [],
    store.data?.squads ?? [],
    props.agentId,
    {
      messages: store.messages[message.sessionId] ?? [],
      session: store.data?.sessions.find((item) => item.id === message.sessionId) ?? null,
    },
  );
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
        pasteError.value = t("chat.pasteTooLarge");
        continue;
      }
      const saved = await backend.savePastedAttachment(bytes, file.type || "image/png", file.name || undefined);
      pastes.value.push({ ...saved, previewUrl: URL.createObjectURL(file) });
    } catch (cause) {
      pasteError.value = cause instanceof Error ? cause.message : t("chat.pasteFailed");
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
  // 有未回答提问时用 priority，否则会被 awaitingAnswer 挡住、不进 run、也不出工作态。
  store.send(text, props.sessionId, attachments, {
    priority: store.awaitingAnswer(props.sessionId),
  });
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
    <!-- 任务详情由外层 task-issue-scroll 负责滚动；仅 inbox thread 自管滚动时挂 vs-scroll -->
    <div v-if="messages.length" ref="logEl" class="task-activity-log" :class="{ 'vs-scroll': thread }">
      <template v-for="message in messages" :key="message.id">
        <div
          v-if="message.id === unreadMessageId"
          class="chat-new-divider"
          role="separator"
          :data-message-id="message.id"
        >
          <span>{{ t('chat.newMessages') }}</span>
        </div>
        <UserPrompt
          v-if="message.role === 'user'"
          :data-message-id="message.id"
          :message="message"
          :target-label="agent?.name ?? t('inbox.agentFallback')"
          :target-avatar="agent?.avatar ?? 'M'"
          :target-value="agent ? `agent:${agent.id}` : ''"
          :allow-target-change="false"
          :enable-replay="false"
          :view-only="historyViewOnly"
          :running="!historyViewOnly && sessionProcessing"
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
      <div
        v-if="showWorking"
        class="assistant-working"
        aria-live="polite"
      >
        <LoaderCircle :size="14" />
        <AgentAvatar
          v-for="item in workingAgents"
          :key="item.id"
          :value="item.avatar"
          :name="item.name"
          :size="16"
        />
        <span class="assistant-working-lines">
          <span v-for="(line, index) in workingLines" :key="index" class="assistant-working-line">
            <UiShimmerText :text="line.label" rotate />
            <span>· {{ line.elapsed }}</span>
          </span>
        </span>
      </div>
    </div>
    <p v-else-if="showWorking" class="task-activity-empty">
      <span class="assistant-working" aria-live="polite">
        <LoaderCircle :size="14" />
        <AgentAvatar
          v-for="item in workingAgents"
          :key="item.id"
          :value="item.avatar"
          :name="item.name"
          :size="16"
        />
        <span class="assistant-working-lines">
          <span v-for="(line, index) in workingLines" :key="index" class="assistant-working-line">
            <UiShimmerText :text="line.label" rotate />
            <span>· {{ line.elapsed }}</span>
          </span>
        </span>
      </span>
    </p>
    <p v-else class="task-activity-empty">{{ t('tasks.activityEmpty') }}</p>
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
      :placeholder="t('tasks.activityReply')"
      :initial-nodes="emptyNodes"
      :contexts="[]"
      :target-label="agent?.name ?? t('common.unassigned')"
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
        <UiButton v-if="sessionRunning && !canSend" class="icon-button" type="button" :title="t('chat.stop')" @click="stop">
          <Square :size="14" />
        </UiButton>
        <UiButton v-else class="ds-send-button send-button" type="submit" :title="sessionRunning ? t('chat.enqueue') : t('chat.send')" :disabled="!canSend">
          <ArrowUp :size="16" />
        </UiButton>
      </template>
    </ChatComposer>
  </div>
</template>
