<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Archive, ArrowRight, FolderKanban, GitBranch, Inbox as InboxIcon, ListTodo, MailOpen, MessageSquare, MessagesSquare, Trash2 } from "@lucide/vue";
import AgentAvatar from "../components/AgentAvatar.vue";
import ChannelMark from "../components/channels/ChannelMark.vue";
import ChatPanel from "../components/ChatPanel.vue";
import WorkflowRunView from "./workflows/WorkflowRunView.vue";
import { isWorkflowNodeSession, isWorkflowRunSession, workflowRunIdFromSession } from "../domain/workflow";
import SegmentedTabs from "../components/ui/SegmentedTabs.vue";
import { firstUnreadMessageId, plainInboxLine, unreadBadgeLabel, unreadTotal } from "../domain/inbox";
import UiCountBadge from "../components/ui/UiCountBadge.vue";
import type { ChannelType, FeishuContact, InboxItem } from "../domain/types";
import { t } from "../i18n";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";
import InboxFileReview from "./InboxFileReview.vue";
import InboxToolRail from "./inbox/InboxToolRail.vue";
import { inboxProjectIds, workspacesForProjects } from "./inbox/inbox-workspaces";
import TaskActivity from "./TaskActivity.vue";
import { relativeTime } from "./task-board";

const store = useAppStore();
const route = useRoute();
const router = useRouter();
const selectedId = ref<string | null>(null);
const threadPanel = ref<{ focus: () => void } | null>(null);
const dividerMessageId = ref<string | null>(null);
const REVIEW_MIN_WIDTH = 240;
const REVIEW_DEFAULT_WIDTH = 420;
const RAIL_MIN_WIDTH = 260;
const RAIL_MAX_WIDTH = 560;
const RAIL_DEFAULT_WIDTH = 340;
const RAIL_WIDTH_KEY = "multara.inbox.railWidth";
const reviewFile = ref<{ workspaceId: string; filePath: string } | null>(null);
const reviewWidth = ref(REVIEW_DEFAULT_WIDTH);
const railWidth = ref(readRailWidth());
const reviewPane = ref<{ layout: () => void } | null>(null);
const toolRail = ref<{ closeAll: () => void } | null>(null);
const stageContent = ref(false);
const stagePanelWidth = ref(0);
const stageTerminal = ref(false);
const pickedWorkspaceId = ref("");
const stageWidth = computed(() => (stageContent.value ? stagePanelWidth.value : 0));

const items = computed(() =>
  [...(store.data?.inbox ?? [])]
    .filter((item) => !isWorkflowNodeSession(item.sessionId))
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt)),
);
type InboxSource = "all" | "unread" | "waiting" | "project" | "task" | "workflow" | "chat" | "channel";
const SOURCE_TABS = computed(() => [
  { id: "all" as const, label: t("inbox.filter.all") },
  { id: "unread" as const, label: t("inbox.filter.unread") },
  { id: "waiting" as const, label: t("inbox.filter.waiting") },
  { id: "project" as const, label: t("inbox.filter.project") },
  { id: "task" as const, label: t("nav.tasks") },
  { id: "workflow" as const, label: t("inbox.filter.workflow") },
  { id: "chat" as const, label: t("inbox.filter.chat") },
  { id: "channel" as const, label: t("inbox.filter.channel") },
]);
const sourceFilter = ref<InboxSource>("all");

/**
 * 通知来源：工作流总览 → 工作流；挂了任务 → 任务；只挂需求 → 需求工作台里的对话；
 * 两者都没有 → 右下角浮动聊天发起的对话。
 */
const CHANNEL_SOURCES = new Set(["feishu", "lark", "slack", "dingtalk", "wecom", "telegram", "weixin", "qq"]);

function isChannel(item: InboxItem) {
  return CHANNEL_SOURCES.has(item.channelType ?? "");
}

function channelMarkType(item: InboxItem): ChannelType | null {
  const type = item.channelType ?? "";
  return CHANNEL_SOURCES.has(type) ? type as ChannelType : null;
}

function sourceOf(item: InboxItem): "project" | "task" | "workflow" | "chat" | "channel" {
  if (isWorkflowRunSession(item.sessionId)) return "workflow";
  if (isChannel(item)) return "channel";
  if (item.taskId) return "task";
  if (item.projectId) return "project";
  return "chat";
}

function railKind(item: InboxItem): "project" | "task" | "chat" {
  if (item.taskId) return "task";
  if (item.projectId) return "project";
  return "chat";
}

/** 按类型筛过时列表里已经是同一类，不再重复打来源标签。 */
const showSourceTag = computed(
  () => sourceFilter.value !== "project" && sourceFilter.value !== "task" && sourceFilter.value !== "workflow" && sourceFilter.value !== "chat" && sourceFilter.value !== "channel",
);

function sourceLabel(item: InboxItem) {
  const source = sourceOf(item);
  if (source === "channel") return t("inbox.filter.channel");
  if (source === "project") return t("inbox.filter.project");
  if (source === "task") return t("nav.tasks");
  if (source === "workflow") return t("inbox.filter.workflow");
  return t("inbox.filter.chat");
}

function sourceTone(item: InboxItem): "primary" | "accent" | "purple" | "success" | "warning" {
  const source = sourceOf(item);
  if (source === "channel") return "success";
  if (source === "project") return "primary";
  if (source === "task") return "accent";
  if (source === "workflow") return "warning";
  return "purple";
}

const sourceCounts = computed(() => {
  const counts: Record<InboxSource, number> = { all: 0, unread: 0, waiting: 0, project: 0, task: 0, workflow: 0, chat: 0, channel: 0 };
  for (const item of items.value) {
    counts.all += 1;
    if (!item.read) counts.unread += 1;
    if (item.status === "needs_input" || item.status === "needs_permission") counts.waiting += 1;
    counts[sourceOf(item)] += 1;
  }
  return counts;
});
const sourceTabs = computed(() =>
  SOURCE_TABS.value.map((tab) => ({
    ...tab,
    count: sourceCounts.value[tab.id],
    pinCount: tab.id === "unread",
  })),
);
const visibleItems = computed(() =>
  items.value.filter((item) => {
    if (sourceFilter.value === "all") return true;
    if (sourceFilter.value === "unread") return !item.read;
    if (sourceFilter.value === "waiting") return item.status === "needs_input" || item.status === "needs_permission";
    return sourceOf(item) === sourceFilter.value;
  }),
);
const selected = computed(() => items.value.find((item) => item.id === selectedId.value) ?? null);
const channelContacts = ref<FeishuContact[]>([]);
const channelContactsLoaded = ref(false);
const selectedRunId = computed(() =>
  selected.value && isWorkflowRunSession(selected.value.sessionId)
    ? workflowRunIdFromSession(selected.value.sessionId)
    : "",
);
const selectedSession = computed(() =>
  store.data?.sessions.find((item) => item.id === selected.value?.sessionId),
);
const toolWorkspaces = computed(() => {
  const sessionId = selected.value?.sessionId;
  // 与工作台一致：合并会话只展示专属临时 Worktree，普通会话展示共享项目工作区。
  if (sessionId) {
    const scoped = store.sessionWorkspaces(sessionId);
    if (scoped.length) return scoped;
  }
  return workspacesForProjects(
    inboxProjectIds({
      itemProjectId: selected.value?.projectId,
      sessionProjectId: selectedSession.value?.projectId,
      sessionProjectIds: selectedSession.value?.projectIds,
    }),
    store.data?.workspaces ?? [],
  );
});
const activeWorkspace = computed(() =>
  toolWorkspaces.value.find((item) => item.id === pickedWorkspaceId.value) ?? toolWorkspaces.value[0] ?? null,
);

/** 冲突 / AI 解决后切到合并收件箱条目，中间看聊天、右侧绑临时 Git。 */
async function onMergeFocus(payload: { inboxItemId: string | null; workspaceId: string; sessionId: string }) {
  pickedWorkspaceId.value = payload.workspaceId;
  const item = (payload.inboxItemId
    ? items.value.find((entry) => entry.id === payload.inboxItemId)
    : null)
    ?? items.value.find((entry) => entry.sessionId === payload.sessionId)
    ?? null;
  if (item) {
    await select(item);
    pickedWorkspaceId.value = payload.workspaceId;
    return;
  }
  // 条目尚未刷进列表时，仍按 session 拉消息并选中稍后出现的项
  await store.loadMessages(payload.sessionId);
  const again = items.value.find((entry) => entry.sessionId === payload.sessionId);
  if (again) await select(again);
  pickedWorkspaceId.value = payload.workspaceId;
}

function onToolLayout(payload: { content: boolean; files: boolean; width: number; terminal: boolean }) {
  stageContent.value = payload.content;
  stagePanelWidth.value = payload.width;
  stageTerminal.value = payload.terminal;
}

function openReview(file: { workspaceId: string; filePath: string }, origin: "chat" | "tool") {
  reviewFile.value = file;
  if (origin === "chat") toolRail.value?.closeAll();
}
const selectedTask = computed(() =>
  store.data?.tasks.find((task) => task.id === selected.value?.taskId) ?? null,
);
const unreadCount = computed(() => unreadTotal(items.value));

function statusLabel(status: string) {
  if (status === "completed") return t("inbox.status.completed");
  if (status === "failed") return t("inbox.status.failed");
  if (status === "stopped") return t("inbox.status.stopped");
  if (status === "needs_input") return t("inbox.status.needsInput");
  if (status === "needs_permission") return t("inbox.status.needsPermission");
  return status;
}

const sessionsById = computed(() => new Map((store.data?.sessions ?? []).map((entry) => [entry.id, entry])));
const squadsById = computed(() => new Map((store.data?.squads ?? []).map((entry) => [entry.id, entry])));
const agentsById = computed(() => new Map((store.data?.agents ?? []).map((entry) => [entry.id, entry])));

function agentFor(item: InboxItem) {
  const session = sessionsById.value.get(item.sessionId);
  if (!session) return { avatar: "M", name: t("inbox.agentFallback") };
  if (session.targetType === "squad") {
    const squad = squadsById.value.get(session.targetId);
    return { avatar: "队", name: squad?.name ?? t("inbox.squadFallback") };
  }
  const agent = agentsById.value.get(session.targetId);
  return { avatar: agent?.avatar ?? "M", name: agent?.name ?? t("inbox.agentFallback") };
}

function channelPerson(item: InboxItem) {
  const contact = channelContacts.value.find((entry) => entry.sessionId === item.sessionId);
  if (!contact) return "";
  return contact.displayName?.trim() || contact.note?.trim() || contact.openId;
}

function listTitle(item: InboxItem) {
  if (sourceOf(item) !== "channel") return item.title;
  const name = channelPerson(item);
  if (!name) return item.title;
  return t("inbox.channel.private", { name });
}

const headerTitle = computed(() => {
  const item = selected.value;
  if (!item) return "";
  const source = sourceOf(item);
  if (source === "project") {
    return store.data?.projects.find((project) => project.id === item.projectId)?.name || item.title;
  }
  if (source === "channel") return channelPerson(item) || item.title;
  return item.title;
});

const headerMeta = computed(() => {
  const item = selected.value;
  if (!item) return "";
  const status = item.status !== "completed" ? statusLabel(item.status) : "";
  if (sourceOf(item) === "chat" || sourceOf(item) === "channel" || sourceOf(item) === "project") return status;
  const name = agentFor(item).name;
  return status ? `${name} · ${status}` : name;
});

function destination(item: InboxItem) {
  if (isWorkflowRunSession(item.sessionId)) return null;
  if (item.taskId) return { to: `/tasks/${item.taskId}`, label: t("inbox.openTask") };
  if (item.projectId) return { to: `/projects/${item.projectId}`, label: t("inbox.openProject") };
  return null;
}

/** 需求、任务、气泡聊天都可以继续回复。频道通知和工作流总览只在等输入时可写，审批走消息上的按钮。 */
function composerHistoryOnly(item: InboxItem) {
  const source = sourceOf(item);
  if (source === "project" || source === "task" || source === "chat") return false;
  return item.status !== "needs_input" && item.status !== "needs_permission";
}

async function select(item: InboxItem) {
  const same = selectedId.value === item.id;
  const wasUnread = !item.read;
  if (isWorkflowRunSession(item.sessionId)) await store.ensureSessionLoaded(item.sessionId);
  else await store.loadMessages(item.sessionId);
  const ids = (store.messages[item.sessionId] ?? []).map((message) => message.id);
  if (!same) {
    dividerMessageId.value = wasUnread ? firstUnreadMessageId(ids, item.readMessageId) : null;
  }
  if (!same) reviewFile.value = null;
  selectedId.value = item.id;
  await nextTick();
  threadPanel.value?.focus();
  if (!wasUnread) return;
  await store.markInboxRead(item.id, ids.at(-1) ?? null);
}

function readRailWidth() {
  const stored = Number(localStorage.getItem(RAIL_WIDTH_KEY));
  if (stored >= RAIL_MIN_WIDTH && stored <= RAIL_MAX_WIDTH) return stored;
  return RAIL_DEFAULT_WIDTH;
}

function beginRailResize(event: PointerEvent) {
  const handle = event.currentTarget;
  if (!(handle instanceof HTMLElement)) return;
  event.preventDefault();
  handle.setPointerCapture(event.pointerId);
  const startX = event.clientX;
  const startWidth = railWidth.value;
  document.body.classList.add("is-resizing-panels");
  const move = (moveEvent: PointerEvent) => {
    railWidth.value = Math.min(RAIL_MAX_WIDTH, Math.max(RAIL_MIN_WIDTH, startWidth + (moveEvent.clientX - startX)));
  };
  const finish = () => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", finish);
    handle.removeEventListener("pointercancel", finish);
    if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId);
    document.body.classList.remove("is-resizing-panels");
    localStorage.setItem(RAIL_WIDTH_KEY, String(railWidth.value));
  };
  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", finish);
  handle.addEventListener("pointercancel", finish);
}

function beginReviewResize(event: PointerEvent) {
  const handle = event.currentTarget;
  if (!(handle instanceof HTMLElement)) return;
  event.preventDefault();
  handle.setPointerCapture(event.pointerId);
  const startX = event.clientX;
  const startWidth = reviewWidth.value;
  const max = Math.round((handle.parentElement?.clientWidth ?? window.innerWidth) * 0.72);
  document.body.classList.add("is-resizing-panels");
  const move = (moveEvent: PointerEvent) => {
    reviewWidth.value = Math.min(max, Math.max(REVIEW_MIN_WIDTH, startWidth - (moveEvent.clientX - startX)));
  };
  const finish = () => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", finish);
    handle.removeEventListener("pointercancel", finish);
    if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId);
    document.body.classList.remove("is-resizing-panels");
    reviewPane.value?.layout();
  };
  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", finish);
  handle.addEventListener("pointercancel", finish);
}

function openDestination(item: InboxItem) {
  const target = destination(item);
  if (!target) return;
  void store.markInboxRead(item.id);
  void router.push(target.to);
}

watch(
  () => [route.query.session, items.value.map((item) => item.id).join("\n")] as const,
  () => {
    const sessionId = typeof route.query.session === "string" ? route.query.session : "";
    if (!sessionId) return;
    const item = items.value.find((entry) => entry.sessionId === sessionId);
    if (!item || selectedId.value === item.id) return;
    void select(item);
  },
  { immediate: true },
);

watch(items, (list) => {
  if (selectedId.value && !list.some((item) => item.id === selectedId.value)) {
    selectedId.value = null;
    dividerMessageId.value = null;
    reviewFile.value = null;
  }
});

watch(selectedId, () => {
  pickedWorkspaceId.value = "";
});

watch(
  () => items.value.some((item) => sourceOf(item) === "channel"),
  (hasChannel) => {
    if (!hasChannel || channelContactsLoaded.value) return;
    channelContactsLoaded.value = true;
    void backend.listFeishuContacts().then((page) => {
      channelContacts.value = page.contacts;
    }).catch(() => {
      channelContactsLoaded.value = false;
    });
  },
  { immediate: true },
);

/** 用户在本会话继续发送后，收起未读分割线，避免滚动逻辑反复定位到旧消息。 */
watch(
  () => {
    const sessionId = selected.value?.sessionId;
    return sessionId ? (store.messages[sessionId]?.length ?? 0) : 0;
  },
  (length, previousLength) => {
    if (!dividerMessageId.value || !selected.value) return;
    const before = previousLength ?? 0;
    if (length <= before) return;
    const added = (store.messages[selected.value.sessionId] ?? []).slice(before);
    if (added.some((message) => message.role === "user")) dividerMessageId.value = null;
  },
);

watch(
  selected,
  (item) => {
    store.setActiveInboxSession(item?.sessionId ?? null);
  },
  { immediate: true },
);

onUnmounted(() => {
  store.setActiveInboxSession(null);
});
</script>

<template>
  <div
    class="inbox-page"
    :class="{
      'has-file-review': reviewFile,
      'has-stage': selected && stageWidth,
      'has-terminal': selected && stageTerminal,
    }"
    :style="{
      '--inbox-rail': `${railWidth}px`,
      ...(reviewFile ? { '--inbox-review-width': `${reviewWidth}px` } : {}),
      ...(stageWidth ? { '--inbox-stage-width': `${stageWidth}px` } : {}),
    }"
  >
    <aside class="inbox-rail">
      <header>
        <div class="inbox-rail-title">
          <h1>{{ t("inbox.title") }}</h1>
          <span v-if="unreadCount" class="ds-badge" data-tone="primary">{{ unreadBadgeLabel(unreadCount) }}</span>
          <UiButton v-if="unreadCount" variant="ghost" size="small" @click="store.markInboxAllRead()">{{ t("common.markRead") }}</UiButton>
        </div>
        <p>{{ t("inbox.subtitle") }}</p>
        <div v-if="items.length" class="inbox-filters">
          <SegmentedTabs v-model="sourceFilter" :label="t('inbox.filterBySource')" :items="sourceTabs" />
        </div>
      </header>
      <ul v-if="visibleItems.length" class="inbox-list vs-scroll">
        <li v-for="item in visibleItems" :key="item.id">
          <button
            type="button"
            :class="{ selected: item.id === selectedId, unread: item.unreadCount > 0 }"
            @click="select(item)"
          >
            <AgentAvatar :value="agentFor(item).avatar" :name="agentFor(item).name" :size="28" />
            <span>
              <strong>
                <i v-if="item.unreadCount" class="inbox-unread-dot" />
                <span v-if="showSourceTag" class="ds-badge" :data-tone="sourceTone(item)">
                  <GitBranch v-if="sourceOf(item) === 'workflow'" :size="12" />
                  <ListTodo v-else-if="sourceOf(item) === 'task'" :size="12" />
                  <FolderKanban v-else-if="sourceOf(item) === 'project'" :size="12" />
                  <MessagesSquare v-else-if="sourceOf(item) === 'channel'" :size="12" />
                  <MessageSquare v-else :size="12" />
                  {{ sourceLabel(item) }}
                </span>
                <span v-if="channelMarkType(item)" class="inbox-channel-mark">
                  <ChannelMark :type="channelMarkType(item)!" />
                </span>
                <span class="inbox-item-title">{{ listTitle(item) }}</span>
              </strong>
              <small>{{ plainInboxLine(item.preview) }}</small>
            </span>
            <span class="inbox-meta">
              <UiCountBadge
                v-if="item.unreadCount"
                class="nav-badge"
                tone="primary"
                :count="item.unreadCount"
              />
              <em v-else>{{ relativeTime(item.createdAt) }}</em>
            </span>
          </button>
        </li>
      </ul>
      <div v-else-if="items.length" class="ds-empty inbox-rail-empty">
        <span class="ds-empty-icon"><InboxIcon :size="18" /></span>
        <h3>{{ t("inbox.emptyFiltered") }}</h3>
        <p>{{ t("inbox.emptyFilteredHint") }}</p>
      </div>
      <div v-else class="ds-empty inbox-rail-empty">
        <span class="ds-empty-icon"><InboxIcon :size="18" /></span>
        <h3>{{ t("inbox.empty") }}</h3>
        <p>{{ t("inbox.emptyHint") }}</p>
      </div>
      <div
        class="panel-resize-handle inbox-rail-resize"
        role="separator"
        :aria-label="t('app.resizeSidebar')"
        aria-orientation="vertical"
        :aria-valuemin="RAIL_MIN_WIDTH"
        :aria-valuemax="RAIL_MAX_WIDTH"
        :aria-valuenow="railWidth"
        tabindex="0"
        @pointerdown="beginRailResize"
      />
    </aside>

    <section v-if="selected && selectedRunId" class="inbox-detail inbox-run">
      <WorkflowRunView :key="selectedRunId" :run-id="selectedRunId">
        <template #actions>
          <UiButton v-if="selected.read" variant="ghost" size="small" @click="store.markInboxUnread(selected.id)">{{ t("common.markUnread") }}</UiButton>
          <UiButton variant="ghost" size="icon" :tip="t('common.archive')" @click="store.archiveInboxItem(selected.id)"><Archive :size="14" /></UiButton>
          <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="store.deleteInboxItem(selected.id)"><Trash2 :size="14" /></UiButton>
        </template>
      </WorkflowRunView>
    </section>
    <section v-else-if="selected" class="inbox-detail chat-surface">
      <header class="chat-surface-header">
        <div>
          <strong>{{ headerTitle }}</strong>
          <small v-if="headerMeta">{{ headerMeta }}</small>
        </div>
        <UiButton v-if="destination(selected)" variant="ghost" size="small" @click="openDestination(selected)">
          {{ destination(selected)?.label }}
          <ArrowRight :size="14" />
        </UiButton>
        <UiButton v-if="selected.read" variant="ghost" size="small" @click="store.markInboxUnread(selected.id)">{{ t("common.markUnread") }}</UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('common.archive')" @click="store.archiveInboxItem(selected.id)"><Archive :size="14" /></UiButton>
        <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="store.deleteInboxItem(selected.id)"><Trash2 :size="14" /></UiButton>
      </header>
      <TaskActivity
        v-if="selected.taskId"
        :key="selected.id"
        ref="threadPanel"
        class="inbox-thread"
        thread
        :session-id="selected.sessionId"
        :agent-id="selectedTask?.agentId ?? ''"
        :unread-message-id="dividerMessageId"
        :hide-composer="sourceOf(selected) === 'channel'"
      />
      <ChatPanel
        v-else
        :key="selected.id"
        ref="threadPanel"
        class="inbox-thread"
        embedded
        :history-view-only="composerHistoryOnly(selected)"
        :hide-composer="sourceOf(selected) === 'channel'"
        :session-id="selected.sessionId"
        :project-id="selected.projectId"
        :unread-message-id="dividerMessageId"
        review-in-place
        @review-file="openReview($event, 'chat')"
      />
    </section>
    <section v-else class="inbox-detail inbox-detail-empty">
      <div class="ds-empty">
        <span class="ds-empty-icon"><MailOpen :size="18" /></span>
        <h3>{{ t("inbox.noneSelected") }}</h3>
        <p>{{ t("inbox.noneSelectedHint") }}</p>
      </div>
    </section>
    <div
      v-if="reviewFile"
      class="panel-resize-handle"
      role="separator"
      :aria-label="t('inbox.resizePreview')"
      aria-orientation="vertical"
      tabindex="0"
      @pointerdown="beginReviewResize"
    />
    <InboxFileReview
      v-if="reviewFile"
      ref="reviewPane"
      :key="`${reviewFile.workspaceId}\n${reviewFile.filePath}`"
      :workspace-id="reviewFile.workspaceId"
      :file-path="reviewFile.filePath"
      @close="reviewFile = null"
    />
    <InboxToolRail
      v-if="selected && !selectedRunId"
      ref="toolRail"
      :workspaces="toolWorkspaces"
      :workspace-id="activeWorkspace?.id ?? ''"
      :session-id="selected.sessionId"
      :kind="railKind(selected)"
      log-label="收件箱"
      :task-id="selected.taskId"
      @pick-workspace="pickedWorkspaceId = $event"
      @layout="onToolLayout"
      @merge-focus="onMergeFocus"
    />
  </div>
</template>

<style scoped>
.inbox-run > :deep(.run-page) {
  flex: 1;
  min-width: 0;
  min-height: 0;
}
</style>

