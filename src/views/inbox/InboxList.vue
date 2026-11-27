<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { FolderKanban, GitBranch, ListTodo, MessageSquare, MessagesSquare } from "@lucide/vue";
import AgentAvatar from "../../components/AgentAvatar.vue";
import ChannelMark from "../../components/channels/ChannelMark.vue";
import UiCountBadge from "../../components/ui/UiCountBadge.vue";
import { plainInboxLine } from "../../domain/inbox";
import type { FeishuContact, InboxItem } from "../../domain/types";
import * as backend from "../../services/backend";
import { useAppStore } from "../../stores/app";
import { relativeTime } from "../task-board";
import { channelMarkType, inboxAgent, inboxListTitle, inboxSource, inboxSourceLabel, inboxSourceTone } from "./inbox-row";

const props = withDefaults(defineProps<{
  items: InboxItem[];
  selectedId?: string | null;
  showSourceTag?: boolean;
  contacts?: FeishuContact[];
  /** @ 菜单键盘高亮。-1 表示不用。 */
  activeIndex?: number;
}>(), {
  selectedId: null,
  showSourceTag: true,
  contacts: undefined,
  activeIndex: -1,
});

const emit = defineEmits<{
  select: [item: InboxItem];
  hover: [index: number];
}>();

const store = useAppStore();
const ownContacts = ref<FeishuContact[]>([]);
const contactsLoaded = ref(false);
const contacts = computed(() => props.contacts ?? ownContacts.value);

watch(
  () => props.contacts === undefined && props.items.some((item) => inboxSource(item) === "channel"),
  (needed) => {
    if (!needed || contactsLoaded.value) return;
    contactsLoaded.value = true;
    void backend.listFeishuContacts().then((page) => {
      ownContacts.value = page.contacts;
    }).catch(() => {
      contactsLoaded.value = false;
    });
  },
  { immediate: true },
);

function agentFor(item: InboxItem) {
  return inboxAgent(
    item,
    store.data?.sessions ?? [],
    store.data?.agents ?? [],
    store.data?.squads ?? [],
  );
}
</script>

<template>
  <ul class="inbox-list vs-scroll">
    <li v-for="(item, index) in items" :key="item.id">
      <button
        type="button"
        :class="{ selected: item.id === selectedId, unread: item.unreadCount > 0 }"
        :data-nav-index="activeIndex >= 0 ? index : undefined"
        :data-active="activeIndex === index ? '' : undefined"
        @mousemove="emit('hover', index)"
        @click="emit('select', item)"
      >
        <AgentAvatar :value="agentFor(item).avatar" :name="agentFor(item).name" :size="28" />
        <span>
          <strong>
            <i v-if="item.unreadCount" class="inbox-unread-dot" />
            <span v-if="showSourceTag" class="ds-badge" :data-tone="inboxSourceTone(item)">
              <GitBranch v-if="inboxSource(item) === 'workflow'" :size="12" />
              <ListTodo v-else-if="inboxSource(item) === 'task'" :size="12" />
              <FolderKanban v-else-if="inboxSource(item) === 'project'" :size="12" />
              <MessagesSquare v-else-if="inboxSource(item) === 'channel'" :size="12" />
              <MessageSquare v-else :size="12" />
              {{ inboxSourceLabel(item) }}
            </span>
            <span v-if="channelMarkType(item)" class="inbox-channel-mark">
              <ChannelMark :type="channelMarkType(item)!" />
            </span>
            <span class="inbox-item-title">{{ inboxListTitle(item, contacts) }}</span>
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
</template>
