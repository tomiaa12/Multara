<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, watch } from "vue";
import { t } from "../i18n";
import { ArrowDown, ArrowUp, ChevronDown, FileText, LoaderCircle, MessageSquare, MessageSquarePlus, Paperclip, Pencil, Plus, Square, Trash2, X } from "@lucide/vue";
import AgentAvatar from "./AgentAvatar.vue";
import ChatComposer from "./chat/ChatComposer.vue";
import { chatMessagesKey } from "./chat/context-usage";
import ChatEditingBar from "./chat/ChatEditingBar.vue";
import ChatQuestionCarousel from "./chat/ChatQuestionCarousel.vue";
import { extractQuestions } from "./chat/question-block";
import ComposerPasteImages from "./chat/ComposerPasteImages.vue";
import { buildChatTargetOptions } from "./chat/chat-target-options";
import type { ComposerNode } from "./chat/composer-content";
import { formatAgentPrompt, parseUserPromptNodes, visiblePromptText } from "./chat/composer-content";
import ReshuffleModal from "./chat/ReshuffleModal.vue";
import { countMessagesFrom, reaskTouchesFiles } from "../stores/reask";
import { useDiffStore } from "../stores/diff";
import {
  confirmRollbackIfDirty,
  listDirtyEditorPaths,
} from "./workbench/unsaved-editor";
import ChatMessageItem from "./ChatMessageItem.vue";
import { liveWorkParts } from "./chat/timeline-display";
import ContextMenu, { type ContextMenuItem } from "./ui/ContextMenu.vue";
import { useAppStore } from "../stores/app";
import type { ChatMessage, EditorSelectionContext, MessageAttachmentInput, RunMode } from "../domain/types";
import { CONTINUE_ASSIGNMENT } from "../domain/continue-task";
import * as backend from "../services/backend";
import { fileToBase64, PASTED_IMAGE_MAX_BYTES } from "../utils/file-base64";
import { resolvePathInWorkspaces, type OsDroppedEntry } from "./chat/workspace-file-drag";
import { listIsAtBottom, stickListToBottom } from "./chat/chat-scroll";
import { scrollListToMessage } from "./chat/scroll-to-message";
import { lastUnansweredUser } from "./chat/unanswered-turn";
import { decodeTextBytes } from "./workbench/text-encoding";

const emit = defineEmits<{
  "review-file": [file: { workspaceId: string; filePath: string }];
}>();

const props = withDefaults(defineProps<{
  projectId?: string | null;
  sessionId?: string | null;
  showSessionList?: boolean;
  compact?: boolean;
  embedded?: boolean;
  historyViewOnly?: boolean;
  reviewInPlace?: boolean;
  unreadMessageId?: string | null;
  allowTargetChange?: boolean;
  preview?: boolean;
  previewInteractive?: boolean;
  previewMessages?: ChatMessage[];
  previewComposerText?: string;
  previewSessionTitle?: string | null;
  previewBusy?: boolean;
  /** 是否展示 /compact、/fork（需求工作台 / 右下角气泡）。 */
  sessionCommands?: boolean;
  /** 输入框上方的待审改文件条；右下角气泡关掉。 */
  showEditingBar?: boolean;
}>(), {
  projectId: null,
  sessionId: null,
  showSessionList: false,
  compact: false,
  embedded: false,
  historyViewOnly: false,
  reviewInPlace: false,
  unreadMessageId: null,
  allowTargetChange: true,
  preview: false,
  previewInteractive: false,
  previewMessages: () => [],
  previewComposerText: "",
  previewSessionTitle: null,
  previewBusy: false,
  sessionCommands: false,
  showEditingBar: true,
});
const reaskTarget = ref<{
  messageId: string;
  content: string;
  attachments: MessageAttachmentInput[];
} | null>(null);
const reshuffleOpen = ref(false);
const dirtyRollbackOpen = ref(false);
const dirtyRollbackMessage = ref("");
let dirtyRollbackResolve: ((allowed: boolean) => void) | null = null;
const rollbackFailureOpen = ref(false);
const rollbackFailureMessage = ref("");
let rollbackFailureResolve: ((allowed: boolean) => void) | null = null;
const restoreNonce = ref(0);
const restoreMessageId = ref("");
const MAX_ATTACHMENTS = 10;

const store = useAppStore();
const diffStore = useDiffStore();
const drafts = ref<Record<string, ComposerNode[]>>({});
const pendingAttachments = ref<Record<string, MessageAttachmentInput[]>>({});
const pendingPastes = ref<Record<string, Array<MessageAttachmentInput & { previewUrl: string }>>>({});
const messageList = ref<HTMLElement | null>(null);
const followBottom = ref(true);
const sessionScroll = new Map<string, { scrollTop: number; atBottom: boolean }>();
const composerError = ref("");
const composerEmpty = ref(true);
const seenChipIds = ref(new Set<string>());
const previewContexts = ref<EditorSelectionContext[]>([]);
const composerContexts = computed(() =>
  props.preview ? previewContexts.value : store.composerContexts,
);

const taskSessionIds = computed(() =>
  new Set(store.data?.tasks.map((task) => task.sessionId) ?? []),
);
const sessions = computed(() =>
  store.data?.sessions.filter((session) =>
    props.sessionId
      ? session.id === props.sessionId
      : session.projectId === props.projectId && !taskSessionIds.value.has(session.id),
  ) ?? [],
);
const previewTabs = ref<Array<{ id: string; title: string }>>([{ id: "preview", title: t("chat.previewTab") }]);
const previewActiveId = ref("preview");
const chatRoot = ref<HTMLElement | null>(null);
const sessionTabs = ref<HTMLElement | null>(null);
const tabMenu = ref<{ x: number; y: number; sessionId: string } | null>(null);
const displaySessions = computed(() => {
  if (props.preview) return previewTabs.value;
  const list = [...sessions.value].reverse();
  if (props.sessionId) return list;
  const opened = store.openSessionTabs[props.projectId ?? "global"];
  if (!opened) return list;
  const byId = new Map(sessions.value.map((item) => [item.id, item]));
  return opened
    .map((id) => byId.get(id))
    .filter((item): item is NonNullable<typeof item> => Boolean(item));
});
const session = computed(() => {
  const visible = displaySessions.value;
  if (props.preview) {
    const tab = visible.find((item) => item.id === previewActiveId.value) ?? visible[0];
    return tab
      ? {
          id: tab.id,
          projectId: null,
          title: tab.title,
          targetType: "agent" as const,
          targetId: "preview",
          createdAt: "",
          updatedAt: "",
        }
      : undefined;
  }
  const active = sessions.value.find((item) => item.id === store.activeSessionId);
  if (active && visible.some((item) => item.id === active.id)) return active;
  return undefined;
});
const reviewWorkspaceIds = computed(() => {
  const workspaces = store.data?.workspaces ?? [];
  const projectId = props.projectId ?? session.value?.projectId;
  const inProject = projectId
    ? workspaces.filter((item) => item.projectId === projectId)
    : workspaces;
  return (inProject.length ? inProject : workspaces).map((item) => item.id);
});
const sessionMessages = computed(() =>
  props.preview
    ? props.previewMessages
    : session.value ? store.messages[session.value.id] ?? [] : [],
);
const afterCount = computed(() =>
  reaskTarget.value ? countMessagesFrom(sessionMessages.value, reaskTarget.value.messageId) : 0,
);
const duplicateCounts = computed(() => {
  const counts = new Map<string, number>();
  let previous: ChatMessage | undefined;
  for (const message of sessionMessages.value) {
    if (
      previous &&
      message.failureReason &&
      previous.failureReason &&
      previous.content === message.content
      && (previous.agentId ?? "") === (message.agentId ?? "")
    ) {
      counts.set(previous.id, (counts.get(previous.id) ?? 1) + 1);
    } else {
      previous = message;
    }
  }
  return counts;
});
// 直接返回 store 里的响应式消息对象：流式更新只会让正在变化的那一条重新渲染，
// 不要再 `{ ...message }` 复制，否则每个片段都会让整段历史重绘。
const displayMessages = computed(() => {
  const result: ChatMessage[] = [];
  let previous: ChatMessage | undefined;
  for (const message of sessionMessages.value) {
    if (
      previous &&
      message.failureReason &&
      previous.failureReason &&
      previous.content === message.content
      && (previous.agentId ?? "") === (message.agentId ?? "")
    ) {
      continue;
    }
    result.push(message);
    previous = message;
  }
  return result;
});
provide(chatMessagesKey, displayMessages);

const messageGroups = computed(() => {
  const groups: Array<{ id: string; messages: typeof displayMessages.value }> = [];
  for (const message of displayMessages.value) {
    const current = groups.at(-1);
    if (message.role === "user" || !current) {
      groups.push({ id: message.id, messages: [message] });
    } else {
      current.messages.push(message);
    }
  }
  return groups;
});
// 长对话只渲染最近若干轮，向上滚到顶再分批展开，避免上千个节点常驻 DOM。
const GROUP_WINDOW = 30;
const visibleGroupCount = ref(GROUP_WINDOW);
const visibleGroups = computed(() =>
  messageGroups.value.length > visibleGroupCount.value
    ? messageGroups.value.slice(-visibleGroupCount.value)
    : messageGroups.value,
);
const hiddenGroupCount = computed(() => messageGroups.value.length - visibleGroups.value.length);
const lastGroupId = computed(() => messageGroups.value.at(-1)?.id ?? "");
let expandingEarlier = false;
async function showEarlierGroups() {
  const element = messageList.value;
  if (!element || expandingEarlier || hiddenGroupCount.value <= 0) return;
  expandingEarlier = true;
  const before = element.scrollHeight;
  const top = element.scrollTop;
  visibleGroupCount.value += GROUP_WINDOW;
  await nextTick();
  // 保持用户正在看的那一行不动，新内容只出现在上方。
  element.scrollTop = top + (element.scrollHeight - before);
  expandingEarlier = false;
}
const attachments = computed(() =>
  session.value ? pendingAttachments.value[session.value.id] ?? [] : [],
);
const pastedImages = computed(() =>
  session.value ? pendingPastes.value[session.value.id] ?? [] : [],
);
const sessionQueue = computed(() =>
  !session.value
    ? []
    : store.queuedMessages
      .map((item, index) => ({ ...item, index }))
      .filter((item) => item.sessionId === session.value?.id),
);
const targetOptions = computed(() =>
  buildChatTargetOptions(
    store.data?.agents ?? [],
    store.data?.squads ?? [],
    (name) => t("chat.squadLabel", { name }),
  ),
);
const targetValue = computed({
  get: () => session.value ? `${session.value.targetType}:${session.value.targetId}` : "",
  set: (value: string) => {
    if (props.preview) return;
    // 标签全关掉时没有当前会话，但下拉仍可选：此时用所选执行者开一个新对话，而不是静默丢弃
    if (!session.value) {
      if (!props.sessionId) void store.createChatSession(props.projectId ?? null, value);
      return;
    }
    void store.setSessionTarget(session.value.id, value);
  },
});
const targetLabel = computed(() =>
  props.preview
    ? t("chat.sampleAgent")
    : targetOptions.value.find((option) => option.value === targetValue.value)?.label ?? t("chat.pickTarget"),
);
const headerSubtitle = computed(() =>
  !props.preview && sessionProcessing.value
    ? t("chat.executingQueued", { n: sessionQueue.value.length })
    : targetLabel.value,
);
const composerPlaceholder = computed(() =>
  session.value ? t("chat.messageTo", { name: targetLabel.value }) : t("chat.pickTargetFirst"),
);
const targetAvatar = computed(() =>
  props.preview
    ? "M"
    : targetOptions.value.find((option) => option.value === targetValue.value)?.avatar ?? "M",
);
const workingAgents = computed(() => {
  if (props.preview) {
    return props.previewBusy
      ? [{ id: "preview", name: targetLabel.value, avatar: targetAvatar.value }]
      : [];
  }
  if (!session.value) return [];
  const seen = new Set<string>();
  const agents: Array<{ id: string; name: string; avatar: string }> = [];
  for (const event of Object.values(store.runStates)) {
    if (event.sessionId !== session.value.id) continue;
    if (event.status !== "queued" && event.status !== "running") continue;
    if (!event.agentId || seen.has(event.agentId)) continue;
    seen.add(event.agentId);
    const agent = store.data?.agents.find((item) => item.id === event.agentId);
    agents.push({
      id: event.agentId,
      name: agent?.name ?? targetLabel.value,
      avatar: agent?.avatar ?? targetAvatar.value,
    });
  }
  return agents;
});
const sessionProcessing = computed(() =>
  props.preview ? props.previewBusy : Boolean(session.value && store.sessionProcessing(session.value.id)),
);
const liveClock = ref(Date.now());
const liveAnchorFallback = ref<number | null>(null);
let liveTimer = 0;
const currentTurnAssistant = computed(() => {
  const messages = displayMessages.value;
  let lastUser = -1;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (messages[index].role === "user") {
      lastUser = index;
      break;
    }
  }
  const afterUser = lastUser >= 0 ? messages.slice(lastUser + 1) : messages;
  for (let index = afterUser.length - 1; index >= 0; index -= 1) {
    if (afterUser[index].role === "assistant") return afterUser[index];
  }
  return null;
});
const liveRunStatus = computed(() => {
  if (props.preview) return props.previewBusy ? "running" as const : null;
  if (!session.value) return store.processingMessage ? "sending" as const : null;
  const events = Object.values(store.runStates).filter((event) =>
    event.sessionId === session.value!.id
    && (event.status === "queued" || event.status === "running")
    && !event.runId.startsWith("pending:"),
  );
  if (events.some((event) => event.status === "running")) return "running" as const;
  if (events.some((event) => event.status === "queued")) return "queued" as const;
  if (sessionProcessing.value) {
    return "sending" as const;
  }
  return null;
});
const liveAnchor = computed(() => {
  const created = Date.parse(currentTurnAssistant.value?.createdAt ?? "");
  if (Number.isFinite(created) && created > 0 && created <= liveClock.value + 1000) return created;
  return liveAnchorFallback.value ?? liveClock.value;
});
const showWorking = computed(() => sessionProcessing.value);
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
    // 窗口不可见时不推进，避免依赖它的计算属性每秒白算；回到前台下一秒自然追上
    if (!document.hidden) liveClock.value = Date.now();
  }, 1000);
}, { immediate: true });

const canCreateSession = computed(() => !props.sessionId);
const composerDraft = computed(() => session.value ? drafts.value[session.value.id] ?? [] : []);
const composerLocked = computed(() =>
  reshuffleOpen.value || dirtyRollbackOpen.value || rollbackFailureOpen.value,
);
const canSend = computed(() => composerHasContent() && Boolean(session.value) && !composerLocked.value);
const canStop = computed(() => sessionProcessing.value);
// 运行中停止按钮一直在；没有草稿时只显示停止，有草稿时再并排显示「加入队列」。
const showComposerStop = computed(() => canStop.value && !composerHasContent());

function userTurnRunning(message: ChatMessage) {
  if (!sessionProcessing.value || message.role !== "user") return false;
  const lastUser = [...displayMessages.value].reverse().find((item) => item.role === "user");
  return lastUser?.id === message.id;
}
// 跳过记录放在 store：排队消息要据此判断是否还在等用户回答
const dismissedQuestions = computed(() => store.dismissedQuestions);
const dismissQuestion = (messageId: string) => store.dismissQuestion(messageId);

const questionStates = computed(() => {
  const states = new Map<string, "pending" | "answered" | "skipped">();
  const messages = displayMessages.value;
  let userAfter = false;
  let pendingTaken = false;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role === "user") {
      userAfter = true;
      continue;
    }
    if (message.role !== "assistant" || message.failureReason) continue;
    if (!extractQuestions(message.content).questions.length) continue;
    if (userAfter) states.set(message.id, "answered");
    else if (dismissedQuestions.value.has(message.id)) states.set(message.id, "skipped");
    else if (!pendingTaken) {
      // 同一轮小队里多条带提问时，只让最新一条占用输入框上方
      states.set(message.id, "pending");
      pendingTaken = true;
    }
  }
  return states;
});
const pendingQuestion = computed(() => {
  if (sessionProcessing.value || composerLocked.value) return null;
  for (const [id, state] of questionStates.value) {
    if (state !== "pending") continue;
    const message = displayMessages.value.find((item) => item.id === id);
    if (!message || messageRunning(message)) continue;
    const questions = extractQuestions(message.content).questions;
    const agent = store.data?.agents.find((item) => item.id === message.agentId);
    return { messageId: id, questions, agentName: agent?.name ?? targetLabel.value };
  }
  return null;
});

function answerQuestion(text: string) {
  if (!pendingQuestion.value || !session.value || !text.trim()) return;
  if (props.preview) {
    dismissQuestion(pendingQuestion.value.messageId);
    return;
  }
  followBottom.value = true;
  // 回答插到队列最前，排在它前面的消息等这轮回答完再发
  store.send(text, session.value.id, [], { priority: true });
}

/** 空闲且有待回答提问时，从输入框直接发的也算回答，不能被提问暂停挡住 */
function replyPriority() {
  return { priority: !sessionProcessing.value && !!session.value && store.awaitingAnswer(session.value.id) };
}

function skipQuestion() {
  if (!pendingQuestion.value) return;
  dismissQuestion(pendingQuestion.value.messageId);
  void nextTick(() => composerInput.value?.focus());
}

const pendingFailure = computed(() => {
  const failure = store.sendFailure;
  if (!failure || !session.value || failure.sessionId !== session.value.id) return null;
  return failure;
});

async function ensureSession() {
  if (props.preview) return;
  const visible = displaySessions.value;
  if (visible.length) {
    if (!visible.some((item) => item.id === store.activeSessionId)) {
      await store.selectSession(visible[visible.length - 1].id);
    }
    return;
  }
  if (!sessions.value.length && !props.sessionId && targetOptions.value.length) {
    await store.createChatSession(props.projectId, targetOptions.value[0].value);
  }
}

async function createSession() {
  if (props.preview) {
    const id = crypto.randomUUID();
    previewTabs.value = [...previewTabs.value, { id, title: t("chat.newChat") }];
    previewActiveId.value = id;
    await nextTick();
    scrollActiveTabIntoView();
    composerInput.value?.focus();
    return;
  }
  if (!targetOptions.value.length) return;
  const preferred =
    (targetValue.value.startsWith("agent:") ? targetValue.value : "")
    || targetOptions.value.find((item) => item.value.startsWith("agent:"))?.value
    || targetValue.value
    || targetOptions.value[0].value;
  await store.createChatSession(props.projectId, preferred);
  await nextTick();
  scrollActiveTabIntoView();
  composerInput.value?.focus();
}

function hasCopySelection(event: KeyboardEvent) {
  const target = event.target;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
    return (target.selectionStart ?? 0) !== (target.selectionEnd ?? 0);
  }
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || !selection.toString()) return false;
  const root = chatRoot.value;
  const anchor = selection.anchorNode;
  return Boolean(root && anchor && root.contains(anchor));
}

function onChatKeydown(event: KeyboardEvent) {
  // 侧边栏里 Ctrl+C 停止当前运行。有选中文字时仍交给复制。
  if (
    event.ctrlKey &&
    !event.metaKey &&
    !event.altKey &&
    !event.shiftKey &&
    event.key.toLowerCase() === "c" &&
    canStop.value &&
    chatRoot.value?.closest(".project-chat") &&
    !hasCopySelection(event)
  ) {
    event.preventDefault();
    void stopAgent();
    return;
  }
  if (props.sessionId || !(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
  if (event.key.toLowerCase() !== "t") return;
  event.preventDefault();
  void createSession();
}

function onTabsWheel(event: WheelEvent) {
  const el = sessionTabs.value;
  if (!el || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
  if (el.scrollWidth <= el.clientWidth) return;
  event.preventDefault();
  el.scrollLeft += event.deltaY;
}

function scrollActiveTabIntoView() {
  const active = sessionTabs.value?.querySelector<HTMLElement>(".chat-session-tab.active");
  active?.scrollIntoView({ block: "nearest", inline: "nearest" });
}

async function selectSession(sessionId: string) {
  if (props.preview) {
    previewActiveId.value = sessionId;
    await nextTick();
    scrollActiveTabIntoView();
    return;
  }
  store.addOpenSessionTab(props.projectId, sessionId);
  await store.selectSession(sessionId);
  await nextTick();
  scrollActiveTabIntoView();
}

async function closeSessionTab(sessionId: string, event?: Event) {
  event?.preventDefault();
  event?.stopPropagation();
  tabMenu.value = null;
  await closeSessionTabs([sessionId]);
}

async function closeSessionTabs(sessionIds: string[]) {
  if (!sessionIds.length) return;
  const closing = new Set(sessionIds);
  const remaining = displaySessions.value.filter((item) => !closing.has(item.id));
  if (props.preview) {
    previewTabs.value = previewTabs.value.filter((item) => !closing.has(item.id));
    if (closing.has(previewActiveId.value)) {
      previewActiveId.value = remaining.at(-1)?.id ?? "";
    }
    return;
  }
  store.closeOpenSessionTabs(props.projectId, sessionIds);
  if (store.activeSessionId && closing.has(store.activeSessionId)) {
    const next = remaining.at(-1);
    if (next) await store.selectSession(next.id);
  }
}

function closeOtherSessionTabs(sessionId: string) {
  return closeSessionTabs(displaySessions.value.filter((item) => item.id !== sessionId).map((item) => item.id));
}

function closeSessionTabsToTheRight(sessionId: string) {
  const index = displaySessions.value.findIndex((item) => item.id === sessionId);
  if (index < 0) return;
  return closeSessionTabs(displaySessions.value.slice(index + 1).map((item) => item.id));
}

function closeAllSessionTabs() {
  return closeSessionTabs(displaySessions.value.map((item) => item.id));
}

function openTabMenu(sessionId: string, event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  tabMenu.value = { x: event.clientX, y: event.clientY, sessionId };
}

const tabMenuItems = computed<ContextMenuItem[]>(() => {
  const sessionId = tabMenu.value?.sessionId;
  if (!sessionId) return [];
  const index = displaySessions.value.findIndex((item) => item.id === sessionId);
  return [
    { id: "fork", label: t("chat.forkSession") },
    { id: "compact", label: t("chat.compact") },
    { id: "close", label: t("chat.close") },
    { id: "close-others", label: t("chat.closeOthers"), disabled: displaySessions.value.length < 2 },
    { id: "close-right", label: t("chat.closeRight"), disabled: index < 0 || index >= displaySessions.value.length - 1 },
    { id: "close-all", label: t("chat.closeAll"), disabled: displaySessions.value.length === 0 },
  ];
});

function onTabMenuSelect(id: string) {
  const sessionId = tabMenu.value?.sessionId;
  tabMenu.value = null;
  if (!sessionId) return;
  if (id === "fork") void store.forkSession(sessionId);
  if (id === "compact") void store.compactSession(sessionId);
  if (id === "close") void closeSessionTab(sessionId);
  if (id === "close-others") void closeOtherSessionTabs(sessionId);
  if (id === "close-right") void closeSessionTabsToTheRight(sessionId);
  if (id === "close-all") void closeAllSessionTabs();
}

const composerInput = ref<{
  focus: () => void;
  clear: () => void;
  insertChip: (item: EditorSelectionContext) => void;
  insertText: (value: string) => void;
  restore: (nodes: ComposerNode[]) => void;
  serialize: () => { text: string; nodes: ComposerNode[]; empty: boolean };
} | null>(null);

function canEditComposer() {
  return !props.preview || props.previewInteractive;
}

function composerHasContent() {
  return !composerEmpty.value || Boolean(attachments.value.length || pastedImages.value.length);
}

function queuePreview(content: string, names: string[]) {
  const text = visiblePromptText(content);
  return [text, names.join("、")].filter(Boolean).join(" · ");
}

function queuePreviewShort(content: string, names: string[]) {
  const full = queuePreview(content, names);
  return full.length > 180 ? `${full.slice(0, 180)}…` : full;
}

function attachmentCount() {
  return attachments.value.length + pastedImages.value.length;
}

function noteAttachmentOverflow(dropped: number) {
  if (dropped <= 0) return;
  composerError.value = t("chat.attachLimit", { n: dropped });
}

function messageRunning(message: ChatMessage) {
  if (props.preview) {
    if (props.previewBusy) {
      const last = [...displayMessages.value].reverse().find((item) => item.role === "assistant");
      return Boolean(last && message.id === last.id);
    }
    return message.role === "assistant"
      && !message.content
      && message.timeline.some((item) => item.eventType === "tool_use")
      && !message.timeline.some((item) => item.eventType === "tool_result");
  }
  return message.runId ? store.runStates[message.runId]?.status === "running" : false;
}

function namePreviewTab(content: string) {
  const tab = previewTabs.value.find((item) => item.id === previewActiveId.value);
  if (!tab || tab.title !== t("chat.newChat")) return;
  const next = backend.titleFromPrompt(content);
  if (next && next !== tab.title) tab.title = next;
}

function onChatPointerDown(event: PointerEvent) {
  const target = event.target as HTMLElement | null;
  if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
  chatRoot.value?.focus({ preventScroll: true });
}

function submit() {
  if (props.preview) {
    const serialized = composerInput.value?.serialize() ?? { text: "", nodes: [] as ComposerNode[] };
    namePreviewTab(formatAgentPrompt(serialized.nodes, { directMentions: true }) || serialized.text);
    if (!props.previewInteractive) return;
    return;
  }
  if (!canSend.value || !session.value) return;
  if (reaskTarget.value) {
    return;
  }
  const serialized = composerInput.value?.serialize() ?? { text: "", nodes: [] as ComposerNode[], empty: true };
  const content = formatAgentPrompt(serialized.nodes, { directMentions: true });
  const selectedAttachments = [
    ...attachments.value,
    ...pastedImages.value.map(({ previewUrl: _previewUrl, ...attachment }) => attachment),
  ];
  for (const image of pastedImages.value) URL.revokeObjectURL(image.previewUrl);
  composerInput.value?.clear();
  drafts.value[session.value.id] = [];
  pendingAttachments.value[session.value.id] = [];
  pendingPastes.value[session.value.id] = [];
  seenChipIds.value = new Set();
  store.clearComposerContexts();
  composerEmpty.value = true;
  composerError.value = "";
  store.clearComposerNotice();
  followBottom.value = true;
  const command = content.trim();
  if (props.sessionCommands && command === "/compact") {
    void store.compactSession(session.value.id);
    return;
  }
  if (props.sessionCommands && command === "/fork") {
    void store.forkSession(session.value.id);
    return;
  }
  store.send(content, session.value.id, selectedAttachments, replyPriority());
}

const continuedFailureIds = ref(new Set<string>());

function dismissComposerMessage() {
  composerError.value = "";
  store.clearComposerNotice();
}

function searchMissingFile() {
  const query = store.composerNoticeQuery;
  store.clearComposerNotice();
  if (query) store.requestQuickOpen(query);
}

function allowContinueFor(message: ChatMessage) {
  if (props.preview || continuedFailureIds.value.has(message.id)) return false;
  const messages = displayMessages.value;
  const index = messages.findIndex((item) => item.id === message.id);
  if (index < 0) return false;
  return !messages.slice(index + 1).some((item) => item.role === "user");
}

function sendStarter(prompt: string) {
  if (!session.value) return;
  if (prompt === CONTINUE_ASSIGNMENT) {
    const pending = [...displayMessages.value].reverse().find((item) => item.failureReason && allowContinueFor(item));
    if (pending) continuedFailureIds.value = new Set([...continuedFailureIds.value, pending.id]);
  }
  store.send(prompt, session.value.id, [], replyPriority());
}

function projectWorkspaces() {
  return (store.data?.workspaces ?? []).filter((item) =>
    props.projectId ? item.projectId === props.projectId : true,
  );
}

function insertLocalWorkspaceChip(file: { workspaceId: string; path: string; isDirectory?: boolean }) {
  const listing = file.isDirectory
    ? "App.vue\ncomposer.ts\nutils.ts"
    : "export const demo = true;\n";
  const context: EditorSelectionContext = {
    id: crypto.randomUUID(),
    workspaceId: file.workspaceId,
    path: file.path,
    startLine: 0,
    endLine: file.isDirectory ? 0 : Math.max(1, listing.split("\n").length),
    text: listing,
    isDirectory: Boolean(file.isDirectory),
  };
  previewContexts.value = [...previewContexts.value, context].slice(-8);
  composerInput.value?.insertChip(context);
  composerInput.value?.focus();
}

async function attachWorkspaceFile(file: { workspaceId: string; path: string; isDirectory?: boolean }) {
  if (!canEditComposer()) return;
  if (props.preview) {
    insertLocalWorkspaceChip(file);
    return;
  }
  await store.addWorkspaceFileContext(file.workspaceId, file.path, { isDirectory: file.isDirectory });
}

function formatDroppedListing(entries: { name: string; isDirectory: boolean }[]) {
  return entries.map((entry) => (entry.isDirectory ? `${entry.name}/` : entry.name)).join("\n");
}

async function readExternalDroppedEntry(entry: OsDroppedEntry) {
  const path = entry.path || entry.name || t("chat.externalFile");
  if (entry.isDirectory) {
    if (!entry.path) return { path, text: "", isDirectory: true };
    return {
      path,
      text: formatDroppedListing(await backend.listExternalDirectory(entry.path)),
      isDirectory: true,
    };
  }
  if (entry.file) {
    const bytes = new Uint8Array(await entry.file.arrayBuffer());
    const text = decodeTextBytes(bytes);
    return { path, text, isDirectory: false };
  }
  if (!entry.path) throw new Error(t("chat.dragReadFail"));
  try {
    const file = await backend.readExternalFile(entry.path);
    return { path: file.path || path, text: file.content, isDirectory: false };
  } catch {
    return {
      path,
      text: formatDroppedListing(await backend.listExternalDirectory(entry.path)),
      isDirectory: true,
    };
  }
}

async function attachExternalEntry(entry: OsDroppedEntry) {
  const path = entry.path || entry.name || t("chat.externalFile");
  if (props.preview) {
    insertLocalWorkspaceChip({ workspaceId: "", path, isDirectory: entry.isDirectory });
    return;
  }
  try {
    const loaded = await readExternalDroppedEntry(entry);
    store.addComposerContext({
      workspaceId: "",
      path: loaded.path,
      startLine: 0,
      endLine: loaded.isDirectory ? 0 : Math.max(1, loaded.text.split(/\r\n|\r|\n/).length),
      text: loaded.text,
      isDirectory: loaded.isDirectory,
    });
  } catch {
    const name = path.split(/[\\/]/).pop() || path;
    store.reportComposerNotice(t("chat.attachEmpty", { name }));
    store.addComposerContext({
      workspaceId: "",
      path,
      startLine: 0,
      endLine: entry.isDirectory ? 0 : 1,
      text: "",
      isDirectory: Boolean(entry.isDirectory),
    });
  }
}

async function attachOsFiles(entries: OsDroppedEntry[]) {
  if (!canEditComposer()) return;
  if (!entries.length) {
    composerError.value = t("chat.noRefFiles");
    return;
  }
  composerError.value = "";
  const images = entries.flatMap((entry) =>
    entry.file?.type.startsWith("image/") ? [entry.file] : [],
  );
  if (images.length) await pasteImages(images);
  for (const entry of entries) {
    if (entry.file?.type.startsWith("image/")) continue;
    const resolved = entry.path ? resolvePathInWorkspaces(entry.path, projectWorkspaces()) : null;
    if (resolved) {
      await attachWorkspaceFile({
        workspaceId: resolved.workspaceId,
        path: resolved.path,
        isDirectory: entry.isDirectory,
      });
      continue;
    }
    await attachExternalEntry(entry);
  }
}

function onComposerChange(nodes: ComposerNode[]) {
  if (session.value) drafts.value[session.value.id] = nodes;
  composerEmpty.value = !nodes.some((node) =>
    node.type === "chip" || (node.type === "text" && node.value.replace(/\u00a0/g, " ").trim()),
  );
  const ids = new Set(nodes.filter((node) => node.type === "chip").map((node) => node.id));
  if (props.preview) {
    previewContexts.value = previewContexts.value.filter((item) => ids.has(item.id));
  } else {
    for (const id of seenChipIds.value) {
      if (!ids.has(id)) store.removeComposerContext(id);
    }
  }
  seenChipIds.value = ids;
}

async function pasteImages(files: File[]) {
  if (!canEditComposer() || !session.value) return;
  composerError.value = "";
  const current = pendingPastes.value[session.value.id] ?? [];
  const next = [...current];
  const room = Math.max(0, MAX_ATTACHMENTS - attachmentCount());
  const accepted = files.slice(0, room);
  noteAttachmentOverflow(files.length - accepted.length);
  for (const file of accepted) {
    try {
      if (file.size > PASTED_IMAGE_MAX_BYTES) {
        composerError.value = t("chat.pasteTooLarge");
        continue;
      }
      if (props.preview) {
        next.push({
          name: file.name || "paste.png",
          path: `preview-paste-${crypto.randomUUID()}`,
          mimeType: file.type || "image/png",
          size: file.size,
          previewUrl: URL.createObjectURL(file),
        });
        continue;
      }
      const attachment = await backend.savePastedAttachment(
        await fileToBase64(file),
        file.type || "image/png",
        file.name || undefined,
      );
      next.push({ ...attachment, previewUrl: URL.createObjectURL(file) });
    } catch (error) {
      composerError.value = error instanceof Error ? error.message : t("chat.pasteFailed");
    }
  }
  pendingPastes.value[session.value.id] = next.slice(0, 10);
}

function removePastedImage(path: string) {
  if (!session.value) return;
  const current = pendingPastes.value[session.value.id] ?? [];
  const target = current.find((item) => item.path === path);
  if (target) URL.revokeObjectURL(target.previewUrl);
  pendingPastes.value[session.value.id] = current.filter((item) => item.path !== path);
}

async function chooseAttachments() {
  if (!canEditComposer() || !session.value) return;
  if (props.preview) {
    addPreviewAttachment("notes.md", "text/markdown");
    return;
  }
  const selected = await backend.selectMessageAttachments();
  const current = pendingAttachments.value[session.value.id] ?? [];
  const incoming = selected.filter(
    (item) => !current.some((candidate) => candidate.path === item.path),
  );
  const room = Math.max(0, MAX_ATTACHMENTS - current.length - pastedImages.value.length);
  const accepted = incoming.slice(0, room);
  noteAttachmentOverflow(incoming.length - accepted.length);
  pendingAttachments.value[session.value.id] = [...current, ...accepted];
}

function removeAttachment(path: string) {
  if (!session.value) return;
  pendingAttachments.value[session.value.id] = attachments.value.filter((item) => item.path !== path);
}

/** 程序化滚底时忽略 scroll 事件，避免误关 followBottom。 */
let suppressScrollFollow = false;
let messageListResize: ResizeObserver | null = null;

function handleScroll() {
  const element = messageList.value;
  if (!element) return;
  if (!suppressScrollFollow) followBottom.value = listIsAtBottom(element);
  if (element.scrollTop < 160 && hiddenGroupCount.value > 0) void showEarlierGroups();
}

function rememberSessionScroll(sessionId: string | undefined) {
  const element = messageList.value;
  if (!element || !sessionId) return;
  sessionScroll.set(sessionId, {
    scrollTop: element.scrollTop,
    atBottom: listIsAtBottom(element),
  });
}

function scheduleStickToBottom(behavior: ScrollBehavior = "auto") {
  const run = () => {
    const element = messageList.value;
    if (!element || !followBottom.value) return;
    suppressScrollFollow = true;
    stickListToBottom(element, behavior);
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        suppressScrollFollow = false;
      });
    });
  };
  void nextTick(() => {
    run();
    requestAnimationFrame(run);
  });
}

function applySessionScroll(sessionId: string | undefined) {
  const element = messageList.value;
  if (!element || !sessionId) return;
  const saved = sessionScroll.get(sessionId);
  const atBottom = !saved || saved.atBottom;
  followBottom.value = atBottom;
  if (!saved || saved.atBottom) {
    scheduleStickToBottom("auto");
    return;
  }
  suppressScrollFollow = true;
  element.scrollTop = Math.min(saved.scrollTop, element.scrollHeight);
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      suppressScrollFollow = false;
    });
  });
}

function revealTranscript(sessionId: string | undefined) {
  if (props.unreadMessageId && hiddenGroupCount.value > 0) {
    // 目标消息可能在被折叠的早期轮次里，先全部展开再定位。
    visibleGroupCount.value = messageGroups.value.length;
    void nextTick(() => scrollListToMessage(messageList.value, props.unreadMessageId ?? null));
  }
  if (props.unreadMessageId && scrollListToMessage(messageList.value, props.unreadMessageId)) {
    followBottom.value = false;
    return;
  }
  applySessionScroll(sessionId);
}

function scrollToBottom(behavior: ScrollBehavior = "smooth") {
  followBottom.value = true;
  scheduleStickToBottom(behavior);
}

/** 发送后把最新一轮顶到视口顶部，配合末轮 min-height:100% 与 .user-turn sticky。 */
function pinLastExchangeToTop() {
  const list = messageList.value;
  const last = list?.querySelector<HTMLElement>(".message-exchange:last-child");
  if (!list || !last) {
    scrollToBottom("auto");
    return;
  }
  followBottom.value = true;
  suppressScrollFollow = true;
  const align = () => {
    const delta = last.getBoundingClientRect().top - list.getBoundingClientRect().top;
    if (Math.abs(delta) > 1) list.scrollTop += delta;
  };
  align();
  requestAnimationFrame(() => {
    align();
    // 末轮撑满一屏后，再贴一次绝对底部，短回答时提问仍停在顶部
    stickListToBottom(list, "auto");
    requestAnimationFrame(() => {
      suppressScrollFollow = false;
    });
  });
}

function bindMessageListResize() {
  messageListResize?.disconnect();
  messageListResize = null;
  const element = messageList.value;
  if (!element || typeof ResizeObserver === "undefined") return;
  messageListResize = new ResizeObserver(() => {
    if (!followBottom.value) return;
    stickListToBottom(element, "auto");
  });
  // 观察子节点高度：Markdown / 卡片异步撑高时继续贴底（观察列表自身不会因 scrollHeight 变化触发）。
  for (const child of element.children) messageListResize.observe(child);
}

async function startReaskFromMessage(
  messageId: string,
  content: string,
  attachments: MessageAttachmentInput[],
) {
  if (props.preview || !session.value) return;
  reaskTarget.value = { messageId, content, attachments };
  const filesByMessage = diffStore.snapshots.get(session.value.id) ?? new Map();
  if (!reaskTouchesFiles(sessionMessages.value, messageId, filesByMessage)) {
    void confirmReshuffle(false);
    return;
  }
  reshuffleOpen.value = true;
}

function cancelReask() {
  const messageId = reaskTarget.value?.messageId ?? "";
  reaskTarget.value = null;
  reshuffleOpen.value = false;
  if (!messageId) return;
  restoreMessageId.value = messageId;
  restoreNonce.value += 1;
}

async function applyFailedDraft(failure: { content: string; attachments: MessageAttachmentInput[]; requestedMode?: RunMode | null }) {
  if (!session.value) return;
  if (failure.requestedMode) {
    store.setSessionMode(session.value.id, failure.requestedMode).catch((cause: unknown) => {
      composerError.value = t("chat.mode.switchFailed", { message: cause instanceof Error ? cause.message : String(cause) });
    });
  }
  const nodes = parseUserPromptNodes(failure.content);
  drafts.value[session.value.id] = nodes;
  pendingAttachments.value[session.value.id] = [...failure.attachments];
  composerEmpty.value = !nodes.some((node) =>
    node.type === "chip" || (node.type === "text" && node.value.replace(/\u00a0/g, " ").trim()),
  ) && failure.attachments.length === 0;
  await nextTick();
  composerInput.value?.restore(nodes);
}

async function stopAgent() {
  if (props.preview || !session.value) return;
  const unanswered = lastUnansweredUser(sessionMessages.value);
  if (unanswered) {
    const draft = await store.withdrawUnansweredPrompt(session.value.id);
    if (draft) {
      await applyFailedDraft(draft);
      return;
    }
  }
  await store.stopActiveRun();
}

const queueCollapsed = ref(false);
/** 队列因待回答的提问暂停中（运行中则只是正常排队） */
const queueHeld = computed(() =>
  !sessionProcessing.value && !!session.value && store.awaitingAnswer(session.value.id),
);

function editQueued(index: number) {
  if (composerHasContent()) {
    composerError.value = t("chat.composerBlockedEdit");
    return;
  }
  const item = store.takeQueuedMessage(index);
  if (item) void applyFailedDraft(item);
}

function restoreFailedDraft() {
  const failure = pendingFailure.value;
  if (!failure) return;
  if (composerHasContent()) {
    composerError.value = t("chat.composerBlockedRestore");
    return;
  }
  void applyFailedDraft(failure).then(() => store.dismissSendFailure());
}

function askDirtyRollback(message: string) {
  dirtyRollbackMessage.value = message;
  dirtyRollbackOpen.value = true;
  return new Promise<boolean>((resolve) => {
    dirtyRollbackResolve = resolve;
  });
}

function finishDirtyRollback(allowed: boolean) {
  dirtyRollbackOpen.value = false;
  dirtyRollbackResolve?.(allowed);
  dirtyRollbackResolve = null;
}

function askRollbackFailure(message: string) {
  rollbackFailureMessage.value = message;
  rollbackFailureOpen.value = true;
  return new Promise<boolean>((resolve) => {
    rollbackFailureResolve = resolve;
  });
}

function finishRollbackFailure(allowed: boolean) {
  rollbackFailureOpen.value = false;
  rollbackFailureResolve?.(allowed);
  rollbackFailureResolve = null;
}

async function confirmReshuffle(rollback: boolean) {
  const target = reaskTarget.value;
  const sessionId = session.value?.id;
  if (!target || !sessionId) return;
  if (rollback) {
    const allowed = await confirmRollbackIfDirty(listDirtyEditorPaths(), askDirtyRollback);
    if (!allowed) return;
  }
  const content = target.content;
  const selectedAttachments = target.attachments;
  reshuffleOpen.value = false;
  reaskTarget.value = null;
  await store.reaskFrom(target.messageId, content, selectedAttachments, rollback, askRollbackFailure);
}

function updateTarget(value: string) {
  targetValue.value = value;
}

watch(
  () => [props.preview, props.projectId, Boolean(store.data)] as const,
  ([preview, projectId, ready]) => {
    if (preview || !ready) return;
    store.seedOpenSessionTabs(projectId, sessions.value.map((item) => item.id));
  },
  { immediate: true },
);
watch(
  () => [props.projectId, props.sessionId],
  () => void ensureSession(),
);
watch(
  () => [
    session.value?.id,
    sessionMessages.value.at(-1)?.content,
    sessionMessages.value.at(-1)?.timeline.length,
    sessionMessages.value.length,
  ],
  async (current, previous) => {
    if (!previous || current[0] !== previous[0]) return;
    const before = messageList.value?.scrollHeight ?? 0;
    const lengthBefore = Number(previous[3] ?? 0);
    const lengthAfter = Number(current[3] ?? 0);
    await nextTick();
    const element = messageList.value;
    if (!element) return;
    // 新对话是空的，切到已有对话时历史会一次出现。高度增量经常不到一屏，
    // 不能走平滑滚动，否则会从顶部滑到底部。直接落到这个对话上次的位置。
    if (lengthBefore === 0 && lengthAfter > 0) {
      applySessionScroll(typeof current[0] === "string" ? current[0] : undefined);
      return;
    }
    // 刚发出的用户消息：把这一轮顶到视口顶部（粘性提问）。
    if (lengthAfter > lengthBefore && sessionMessages.value.at(-1)?.role === "user") {
      followBottom.value = true;
      void nextTick(() => pinLastExchangeToTop());
      return;
    }
    if (!followBottom.value) return;
    const opened = element.scrollHeight - before > element.clientHeight;
    if (opened) {
      applySessionScroll(typeof current[0] === "string" ? current[0] : undefined);
      return;
    }
    // 流式输出时每个片段都会走到这里，smooth 动画会被反复重启并连带强制布局。
    // 新提问靠末轮 min-height:100% + sticky，贴底后提问停在视口顶，回答在下方展开。
    scrollToBottom(sessionProcessing.value ? "auto" : "smooth");
  },
);
watch(
  () => session.value?.id,
  (sessionId, previousId) => {
    if (previousId && previousId !== sessionId) rememberSessionScroll(previousId);
    visibleGroupCount.value = GROUP_WINDOW;
    reaskTarget.value = null;
    reshuffleOpen.value = false;
    seenChipIds.value = new Set(
      (sessionId ? drafts.value[sessionId] ?? [] : [])
        .filter((node) => node.type === "chip")
        .map((node) => node.id),
    );
    composerEmpty.value = !composerDraft.value.some((node) =>
      node.type === "chip" || (node.type === "text" && node.value.replace(/\u00a0/g, " ").trim()),
    );
    void nextTick(() => {
      revealTranscript(sessionId);
      requestAnimationFrame(() => {
        if (session.value?.id === sessionId) revealTranscript(sessionId);
      });
      scrollActiveTabIntoView();
    });
  },
);
watch(
  () => store.sendFailure?.nonce,
  () => {
    const failure = pendingFailure.value;
    if (!failure) return;
    composerError.value = failure.message;
    if (composerHasContent()) return;
    void applyFailedDraft(failure).then(() => store.dismissSendFailure());
  },
);

watch(
  () => props.unreadMessageId,
  (messageId) => {
    if (!messageId) return;
    void nextTick(() => {
      requestAnimationFrame(() => revealTranscript(session.value?.id));
    });
  },
  { immediate: true },
);

onMounted(() => {
  void ensureSession();
  void nextTick(() => {
    sessionTabs.value?.addEventListener("wheel", onTabsWheel, { passive: false });
    bindMessageListResize();
    if (session.value?.id) revealTranscript(session.value.id);
  });
});
watch(messageList, () => bindMessageListResize());
watch(
  () => [visibleGroups.value.length, sessionMessages.value.length, showWorking.value] as const,
  () => {
    void nextTick(() => bindMessageListResize());
  },
);
onBeforeUnmount(() => {
  messageListResize?.disconnect();
  messageListResize = null;
  window.clearInterval(liveTimer);
  sessionTabs.value?.removeEventListener("wheel", onTabsWheel);
});
watch(
  () => store.composerFocusNonce,
  async () => {
    if (props.preview) return;
    await nextTick();
    const latest = store.composerContexts.at(-1);
    if (latest) composerInput.value?.insertChip(latest);
    composerInput.value?.focus();
  },
);
watch(
  () => props.previewSessionTitle,
  (title) => {
    if (!props.preview || !title) return;
    const tab = previewTabs.value.find((item) => item.id === previewActiveId.value);
    if (tab && tab.title !== title) tab.title = title;
  },
  { immediate: true },
);
watch(
  () => props.previewComposerText,
  async (value) => {
    if (!props.preview || props.previewInteractive) return;
    const nodes: ComposerNode[] = value ? [{ type: "text", value }] : [];
    drafts.value.preview = nodes;
    composerEmpty.value = !value.trim();
    await nextTick();
    composerInput.value?.restore(nodes);
  },
  { immediate: true, flush: "post" },
);

function addPreviewAttachment(name: string, mimeType: string) {
  if (!session.value) return;
  const current = pendingAttachments.value[session.value.id] ?? [];
  pendingAttachments.value[session.value.id] = [
    ...current,
    {
      name,
      path: `preview-att-${crypto.randomUUID()}`,
      mimeType,
      size: 1024,
    },
  ].slice(0, 10);
}

function addPreviewPastedImage() {
  if (!session.value) return;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="160"><rect width="240" height="160" rx="12" fill="#2b4c73"/><text x="120" y="86" text-anchor="middle" fill="#d6e8ff" font-size="18" font-family="sans-serif">${t("chat.pasteScreenshot")}</text></svg>`;
  const previewUrl = `data:image/svg+xml,${encodeURIComponent(svg)}`;
  const current = pendingPastes.value[session.value.id] ?? [];
  pendingPastes.value[session.value.id] = [
    ...current,
    {
      name: "paste.png",
      path: `preview-paste-${crypto.randomUUID()}`,
      mimeType: "image/png",
      size: 512,
      previewUrl,
    },
  ].slice(0, 10);
}

function clearComposerPreview() {
  composerInput.value?.clear();
  if (!session.value) return;
  for (const image of pendingPastes.value[session.value.id] ?? []) {
    if (image.previewUrl.startsWith("blob:")) URL.revokeObjectURL(image.previewUrl);
  }
  pendingAttachments.value[session.value.id] = [];
  pendingPastes.value[session.value.id] = [];
  previewContexts.value = [];
  drafts.value[session.value.id] = [];
  composerEmpty.value = true;
}

defineExpose({
  insertChip: (item: EditorSelectionContext) => {
    if (props.preview) {
      previewContexts.value = [...previewContexts.value.filter((context) => context.id !== item.id), item].slice(-8);
    }
    composerInput.value?.insertChip(item);
    composerInput.value?.focus();
  },
  insertText: (value: string) => composerInput.value?.insertText(value),
  restore: (nodes: ComposerNode[]) => composerInput.value?.restore(nodes),
  addPastedImage: addPreviewPastedImage,
  addAttachment: addPreviewAttachment,
  attachWorkspaceFile,
  clearComposer: clearComposerPreview,
  focus: () => composerInput.value?.focus(),
});
</script>

<template>
  <div
    ref="chatRoot"
    class="shared-chat-panel"
    :class="{ compact, 'with-session-list': showSessionList }"
    tabindex="-1"
    @pointerdown="onChatPointerDown"
    @keydown="onChatKeydown"
  >
    <aside v-if="showSessionList" class="thread-list">
      <div class="thread-heading">
        <strong>{{ t("chat.threadsTitle") }}</strong>
        <UiButton class="icon-button" :tip="t('chat.newChat')" :disabled="!targetOptions.length" @click="createSession">
          <MessageSquarePlus :size="15" />
        </UiButton>
      </div>
      <div class="thread-items vs-scroll">
        <UiButton
          v-for="item in sessions"
          :key="item.id"
          class="thread"
          :class="{ active: item.id === session?.id }"
          @click="selectSession(item.id)"
        >
          <strong>{{ item.title }}</strong>
          <small>{{ item.targetType === "squad" ? t("nav.squads") : t("nav.agents") }}</small>
        </UiButton>
      </div>
    </aside>

    <section class="chat-surface">
      <header v-if="!embedded" class="chat-surface-header" :class="{ 'is-tabs': !showSessionList }">
        <div v-if="showSessionList">
          <strong>{{ session?.title ?? t("chat.newChatLabel") }}</strong>
          <small>{{ headerSubtitle }}</small>
        </div>
        <div v-else class="chat-session-bar">
          <div ref="sessionTabs" class="chat-session-tabs vs-scroll">
            <template v-for="item in displaySessions" :key="item.id">
              <UiTooltip :content="item.title">
                <button
                  type="button"
                  class="chat-session-tab"
                  :class="{ active: item.id === session?.id }"
                  :data-session-id="item.id"
                  @click="selectSession(item.id)"
                  @click.middle.prevent="closeSessionTab(item.id, $event)"
                  @auxclick.middle.prevent="closeSessionTab(item.id, $event)"
                  @mousedown.middle.prevent
                  @contextmenu="openTabMenu(item.id, $event)"
                >
                  <MessageSquare :size="13" />
                  <span>{{ item.title }}</span>
                  <UiTooltip :content="t('chat.closeTab')">
                    <span class="tab-close" @click="closeSessionTab(item.id, $event)">
                      <X :size="12" />
                    </span>
                  </UiTooltip>
                </button>
              </UiTooltip>
            </template>
          </div>
          <UiButton
            v-if="canCreateSession"
            class="chat-session-new"
            variant="ghost"
            :tip="t('chat.newChatLabel')"
            :disabled="!preview && !targetOptions.length"
            @click="createSession"
          >
            <Plus :size="15" />
            <span>{{ t("chat.newChatLabel") }}</span>
          </UiButton>
          <div v-if="$slots['header-actions']" class="chat-header-actions">
            <slot name="header-actions" />
          </div>
        </div>
      </header>

      <div ref="messageList" class="message-list vs-scroll" @scroll.passive="handleScroll">
        <div v-if="!sessionMessages.length" class="chat-welcome">
          <AgentAvatar :value="targetAvatar" :name="targetLabel" :size="34" />
          <h2>{{ t("chat.welcomeTitle") }}</h2>
          <p>{{ t("chat.welcomeHint") }}</p>
        </div>
        <button
          v-if="hiddenGroupCount > 0"
          type="button"
          class="chat-show-earlier"
          @click="showEarlierGroups"
        >{{ t("chat.showEarlier", { count: hiddenGroupCount }) }}</button>
        <section
          v-for="group in visibleGroups"
          :key="group.id"
          class="message-exchange"
        >
          <template v-for="message in group.messages" :key="message.id">
            <div
              v-if="message.id === unreadMessageId"
              class="chat-new-divider"
              role="separator"
              :data-message-id="message.id"
            >
              <span>{{ t("chat.newMessagesDivider") }}</span>
            </div>
            <ChatMessageItem
              :data-message-id="message.id"
              :message="message"
              :duplicate-count="duplicateCounts.get(message.id) ?? 1"
              :running="messageRunning(message)"
              :target-label="targetLabel"
              :target-avatar="targetAvatar"
              :target-value="targetValue"
              :target-options="targetOptions"
              :allow-target-change="allowTargetChange"
              :enable-replay="!preview && !historyViewOnly"
              :view-only="historyViewOnly"
              :default-collapse-subagents="embedded || historyViewOnly"
              :compact="compact"
              :allow-continue="!historyViewOnly && allowContinueFor(message)"
              :allow-fork="sessionCommands && !preview"
              :restore-nonce="restoreNonce"
              :restore-message-id="restoreMessageId"
              :turn-running="userTurnRunning(message)"
              :question-state="questionStates.get(message.id) ?? null"
              @start-reask="startReaskFromMessage"
              @stop="stopAgent"
              @target-change="updateTarget"
              @pick-action="sendStarter"
            />
          </template>
          <div
            v-if="showWorking && group.id === lastGroupId"
            class="assistant-working"
            aria-live="polite"
          >
            <LoaderCircle :size="14" />
            <AgentAvatar
              v-for="agent in workingAgents"
              :key="agent.id"
              :value="agent.avatar"
              :name="agent.name"
              :size="16"
            />
            <span class="assistant-working-lines">
              <span v-for="(line, index) in workingLines" :key="index" class="assistant-working-line">
                <UiShimmerText :text="line.label" rotate />
                <span>· {{ line.elapsed }}</span>
              </span>
            </span>
          </div>
        </section>
      </div>
      <div v-if="pendingFailure" class="chat-send-failure vs-scroll">
        <div class="chat-queue-heading">
          <span>{{ t("chat.sendNotDelivered") }}</span>
        </div>
        <div class="chat-queue-item">
          <UiTooltip :content="queuePreview(pendingFailure.content, pendingFailure.attachments.map((item) => item.name))">
            <span>{{ queuePreviewShort(pendingFailure.content, pendingFailure.attachments.map((item) => item.name)) || pendingFailure.message }}</span>
          </UiTooltip>
          <UiButton variant="ghost" size="small" @click="restoreFailedDraft">{{ t("chat.restoreToComposer") }}</UiButton>
          <UiButton class="icon-button" variant="ghost" size="icon" :tip="t('common.close')" @click="store.dismissSendFailure()">
            <X :size="12" />
          </UiButton>
        </div>
      </div>

      <div v-if="!followBottom" class="scroll-to-bottom-row">
        <UiButton
          class="scroll-to-bottom"
          variant="outline"
          size="small"
          @click="scrollToBottom()"
        ><ArrowDown :size="14" />{{ t("chat.scrollToBottom") }}</UiButton>
      </div>

      <div class="chat-editing-dock">
      <ChatQuestionCarousel
        v-if="pendingQuestion"
        :key="pendingQuestion.messageId"
        :questions="pendingQuestion.questions"
        :agent-name="pendingQuestion.agentName"
        @submit="answerQuestion"
        @skip="skipQuestion"
      />
      <section
        v-if="sessionQueue.length"
        class="chat-queue"
        :class="{ collapsed: queueCollapsed, 'is-held': queueHeld, 'under-question': !!pendingQuestion }"
      >
        <button type="button" class="chat-queue-heading" :aria-expanded="!queueCollapsed" @click="queueCollapsed = !queueCollapsed">
          <ChevronDown class="chat-queue-chevron" :size="13" />
          <span>{{ t("chat.queueCount", { n: sessionQueue.length }) }}</span>
          <small v-if="queueHeld">{{ t("chat.queueHeldHint") }}</small>
          <small v-else-if="sessionProcessing">{{ t("chat.queueAfterTurnHint") }}</small>
        </button>
        <ol v-show="!queueCollapsed" class="chat-queue-list vs-scroll">
          <li v-for="queued in sessionQueue" :key="queued.index" class="chat-queue-item">
            <span class="chat-queue-dot" aria-hidden="true" />
            <UiTooltip :content="queuePreview(queued.content, queued.attachments.map((item) => item.name))">
              <span class="chat-queue-text">{{ queuePreviewShort(queued.content, queued.attachments.map((item) => item.name)) }}</span>
            </UiTooltip>
            <span class="chat-queue-actions">
              <UiButton
                variant="ghost"
                size="small"
                :tip="queueHeld ? t('chat.queueSkipQuestion') : t('chat.queueSendImmediate')"
                @click="store.promoteQueuedMessage(queued.index)"
              >{{ t("chat.queueSendNow") }}</UiButton>
              <UiButton class="icon-button" variant="ghost" size="icon" :tip="t('chat.queueEdit')" @click="editQueued(queued.index)">
                <Pencil :size="12" />
              </UiButton>
              <UiButton class="icon-button" variant="ghost" size="icon" :tip="t('chat.queueRemove')" @click="store.removeQueuedMessage(queued.index)">
                <Trash2 :size="12" />
              </UiButton>
            </span>
          </li>
        </ol>
      </section>
      <ChatEditingBar
        v-if="!preview && showEditingBar"
        :workspace-ids="reviewWorkspaceIds"
        :review-in-place="reviewInPlace"
        @open="emit('review-file', $event)"
      />
      <ChatComposer
        :key="session?.id ?? 'none'"
        ref="composerInput"
        dictation
        :placeholder="composerPlaceholder"
        :initial-nodes="composerDraft"
        :contexts="composerContexts"
        :target-label="targetLabel"
        :target-avatar="targetAvatar"
        :target-value="targetValue"
        :target-options="targetOptions"
        :allow-target-change="allowTargetChange"
        :can-send="canSend"
        :disabled="composerLocked"
        :session-commands="sessionCommands"
        :mode-session-id="preview || historyViewOnly ? '' : session?.id"
        @change="onComposerChange"
        @submit="submit"
        @paste-image="pasteImages"
        @target-change="updateTarget"
        @drop-workspace-file="attachWorkspaceFile"
        @drop-os-files="attachOsFiles"
      >
        <ComposerPasteImages
          removable
          :images="pastedImages.map((image) => ({ key: image.path, src: image.previewUrl, name: image.name }))"
          @remove="removePastedImage"
        />
        <div v-if="attachments.length" class="composer-attachments vs-scroll">
          <div v-for="attachment in attachments" :key="attachment.path" class="composer-attachment">
            <FileText :size="15" />
            <span>{{ attachment.name }}</span>
            <UiButton type="button" variant="ghost" size="icon" :tip="t('chat.removeAttachment')" @click="removeAttachment(attachment.path)">
              <X :size="12" />
            </UiButton>
          </div>
        </div>
        <p v-if="composerError || store.composerNotice" class="composer-error">
          <span>{{ composerError || store.composerNotice }}</span>
          <span class="composer-error-actions">
            <UiButton
              v-if="store.composerNoticeQuery"
              type="button"
              size="small"
              variant="ghost"
              @click="searchMissingFile"
            >{{ t("chat.globalSearch") }}</UiButton>
            <UiButton type="button" size="icon" variant="ghost" :tip="t('common.close')" @click="dismissComposerMessage">
              <X :size="12" />
            </UiButton>
          </span>
        </p>
        <template #actions>
            <UiButton type="button" class="icon-button" variant="ghost" :tip="t('chat.addAttachment')" @click="chooseAttachments">
              <Paperclip :size="15" />
            </UiButton>
            <UiButton
              v-if="canStop"
              type="button"
              class="icon-button"
              :tip="t('chat.stopTaskShortcut')"
              @click="stopAgent"
            ><Square :size="14" /></UiButton>
            <UiButton
              v-if="!showComposerStop"
              class="ds-send-button send-button"
              type="submit"
              :disabled="!canSend"
              :tip="canStop ? t('chat.enqueue') : t('chat.send')"
            >
              <ArrowUp :size="16" />
            </UiButton>
        </template>
      </ChatComposer>
      </div>
    </section>
    <Teleport to="body">
      <ReshuffleModal
        v-if="reshuffleOpen"
        :after-count="afterCount"
        @confirm="confirmReshuffle"
        @cancel="cancelReask()"
      />
      <UiModal v-if="rollbackFailureOpen" @click.self="finishRollbackFailure(false)">
        <div class="modal" role="dialog" aria-modal="true">
          <h2>{{ t("chat.rollbackFailTitle") }}</h2>
          <p>{{ t("chat.rollbackFailBody") }}</p>
          <p>{{ rollbackFailureMessage }}</p>
          <div class="modal-actions">
            <UiButton @click="finishRollbackFailure(false)">{{ t("common.cancel") }}</UiButton>
            <UiButton variant="danger" @click="finishRollbackFailure(true)">{{ t("chat.rollbackFailConfirm") }}</UiButton>
          </div>
        </div>
      </UiModal>
      <UiModal v-if="dirtyRollbackOpen" @click.self="finishDirtyRollback(false)">
        <div class="modal" role="dialog" aria-modal="true">
          <h2>{{ t("dialog.unsavedTitle") }}</h2>
          <p>{{ dirtyRollbackMessage }}</p>
          <div class="modal-actions">
            <UiButton @click="finishDirtyRollback(false)">{{ t("common.cancel") }}</UiButton>
            <UiButton variant="danger" @click="finishDirtyRollback(true)">{{ t("chat.dirtyRollbackContinue") }}</UiButton>
          </div>
        </div>
      </UiModal>
      <ContextMenu
        v-if="tabMenu"
        :x="tabMenu.x"
        :y="tabMenu.y"
        :items="tabMenuItems"
        @select="onTabMenuSelect"
        @close="tabMenu = null"
      />
    </Teleport>
  </div>
</template>
