<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { Archive, ArrowRight, Inbox as InboxIcon, MailOpen, Trash2 } from "@lucide/vue";
import AgentAvatar from "../components/AgentAvatar.vue";
import ChatPanel from "../components/ChatPanel.vue";
import SegmentedTabs from "../components/ui/SegmentedTabs.vue";
import { firstUnreadMessageId, plainInboxLine, unreadBadgeLabel, unreadTotal } from "../domain/inbox";
import UiCountBadge from "../components/ui/UiCountBadge.vue";
import type { InboxItem } from "../domain/types";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import InboxFileReview from "./InboxFileReview.vue";
import TaskActivity from "./TaskActivity.vue";
import { relativeTime } from "./task-board";

const store = useAppStore();
const router = useRouter();
const selectedId = ref<string | null>(null);
const dividerMessageId = ref<string | null>(null);
const REVIEW_MIN_WIDTH = 240;
const REVIEW_DEFAULT_WIDTH = 420;
const reviewFile = ref<{ workspaceId: string; filePath: string } | null>(null);
const reviewWidth = ref(REVIEW_DEFAULT_WIDTH);
const reviewPane = ref<{ layout: () => void } | null>(null);

const items = computed(() =>
  [...(store.data?.inbox ?? [])].sort((a, b) => b.createdAt.localeCompare(a.createdAt)),
);
type InboxSource = "all" | "unread" | "waiting" | "project" | "task" | "chat";
const SOURCE_TABS = computed(() => [
  { id: "all" as const, label: t("inbox.filter.all") },
  { id: "unread" as const, label: t("inbox.filter.unread") },
  { id: "waiting" as const, label: t("inbox.filter.waiting") },
  { id: "project" as const, label: t("inbox.filter.project") },
  { id: "task" as const, label: t("nav.tasks") },
  { id: "chat" as const, label: t("inbox.filter.chat") },
]);
const sourceFilter = ref<InboxSource>("all");

/**
 * 通知来源：挂了任务 → 任务；只挂需求 → 需求工作台里的对话；
 * 两者都没有 → 右下角浮动聊天发起的对话。
 */
function sourceOf(item: InboxItem): "project" | "task" | "chat" {
  if (item.taskId) return "task";
  if (item.projectId) return "project";
  return "chat";
}

const sourceCounts = computed(() => {
  const counts: Record<InboxSource, number> = { all: 0, unread: 0, waiting: 0, project: 0, task: 0, chat: 0 };
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

function destination(item: InboxItem) {
  if (item.taskId) return { to: `/tasks/${item.taskId}`, label: t("inbox.openTask") };
  if (item.projectId) return { to: `/projects/${item.projectId}`, label: t("inbox.openProject") };
  return null;
}

async function select(item: InboxItem) {
  const same = selectedId.value === item.id;
  const wasUnread = !item.read;
  await store.loadMessages(item.sessionId);
  const ids = (store.messages[item.sessionId] ?? []).map((message) => message.id);
  if (!same) {
    dividerMessageId.value = wasUnread ? firstUnreadMessageId(ids, item.readMessageId) : null;
  }
  if (!same) reviewFile.value = null;
  selectedId.value = item.id;
  if (!wasUnread) return;
  await store.markInboxRead(item.id, ids.at(-1) ?? null);
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

watch(items, (list) => {
  if (selectedId.value && !list.some((item) => item.id === selectedId.value)) {
    selectedId.value = null;
    dividerMessageId.value = null;
    reviewFile.value = null;
  }
});
</script>

<template>
  <div
    class="inbox-page"
    :class="{ 'has-file-review': reviewFile }"
    :style="reviewFile ? { '--inbox-review-width': `${reviewWidth}px` } : undefined"
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
                {{ item.title }}
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
    </aside>

    <section v-if="selected" class="inbox-detail chat-surface">
      <header class="chat-surface-header">
        <div>
          <strong>{{ selected.title }}</strong>
          <small>
            {{ agentFor(selected).name }}
            <template v-if="selected.status !== 'completed'"> · {{ statusLabel(selected.status) }}</template>
          </small>
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
        class="inbox-thread"
        thread
        :history-view-only="selected.status !== 'needs_input' && selected.status !== 'needs_permission'"
        :session-id="selected.sessionId"
        :agent-id="selectedTask?.agentId ?? ''"
        :unread-message-id="dividerMessageId"
      />
      <ChatPanel
        v-else
        :key="selected.id"
        class="inbox-thread"
        embedded
        :history-view-only="selected.status !== 'needs_input' && selected.status !== 'needs_permission'"
        :session-id="selected.sessionId"
        :project-id="selected.projectId"
        :unread-message-id="dividerMessageId"
        review-in-place
        @review-file="reviewFile = $event"
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
  </div>
</template>
