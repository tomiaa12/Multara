<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { LoaderCircle, MessageSquare, MessageSquarePlus, Pencil, Search, X } from "@lucide/vue";
import { t } from "../../i18n";
import ContextMenu, { type ContextMenuItem } from "../ui/ContextMenu.vue";
import { groupSessionList, type SessionListEntry } from "./session-list";

export interface SessionListRow extends SessionListEntry {
  /** 在标签栏里打开着；关掉标签的会话照样列出，只是文字弱一些 */
  open: boolean;
  active: boolean;
  running: boolean;
  awaitingPlan: boolean;
}

const props = withDefaults(defineProps<{
  rows: SessionListRow[];
  createDisabled?: boolean;
  editable?: boolean;
  sessionCommands?: boolean;
}>(), {
  createDisabled: false,
  editable: true,
  sessionCommands: false,
});

const emit = defineEmits<{
  select: [sessionId: string];
  create: [];
  rename: [sessionId: string, title: string];
  "close-tab": [sessionId: string];
  fork: [sessionId: string];
  compact: [sessionId: string];
}>();

const query = ref("");
const groups = computed(() => groupSessionList(props.rows, query.value, new Date()));
const searchInput = ref<HTMLInputElement | null>(null);
const renaming = ref<{ id: string; title: string } | null>(null);
const renameInput = ref<HTMLInputElement[] | HTMLInputElement | null>(null);
const menu = ref<{ x: number; y: number; row: SessionListRow } | null>(null);

const menuItems = computed<ContextMenuItem[]>(() => {
  const row = menu.value?.row;
  if (!row) return [];
  return [
    { id: "open", label: t("chat.sessionList.open") },
    ...(props.editable ? [{ id: "rename", label: t("chat.sessionList.rename") }] : []),
    ...(props.sessionCommands && props.editable
      ? [{ id: "fork", label: t("chat.forkSession") }, { id: "compact", label: t("chat.compact") }]
      : []),
    ...(row.open ? [{ type: "sep" as const }, { id: "close-tab", label: t("chat.closeTab") }] : []),
  ];
});

function rowTip(row: SessionListRow) {
  if (row.running) return t("chat.sessionList.running");
  if (row.awaitingPlan) return t("chat.plan.pendingBadge");
  return row.open ? row.title : t("chat.sessionList.closedTip", { title: row.title });
}

async function startRename(row: SessionListRow) {
  if (!props.editable) return;
  renaming.value = { id: row.id, title: row.title };
  await nextTick();
  const input = Array.isArray(renameInput.value) ? renameInput.value[0] : renameInput.value;
  input?.focus();
  input?.select();
}

function commitRename() {
  const current = renaming.value;
  renaming.value = null;
  if (!current) return;
  const title = current.title.trim();
  const original = props.rows.find((row) => row.id === current.id)?.title;
  if (title && title !== original) emit("rename", current.id, title);
}

function onRenameKeydown(event: KeyboardEvent) {
  if (event.isComposing) return;
  if (event.key === "Enter") {
    event.preventDefault();
    commitRename();
  } else if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    renaming.value = null;
  }
}

function openMenu(row: SessionListRow, event: MouseEvent) {
  event.preventDefault();
  menu.value = { x: event.clientX, y: event.clientY, row };
}

function onMenuSelect(id: string) {
  const row = menu.value?.row;
  menu.value = null;
  if (!row) return;
  if (id === "open") emit("select", row.id);
  if (id === "rename") void startRename(row);
  if (id === "fork") emit("fork", row.id);
  if (id === "compact") emit("compact", row.id);
  if (id === "close-tab") emit("close-tab", row.id);
}

defineExpose({ focusSearch: () => searchInput.value?.focus() });
</script>

<template>
  <nav class="chat-session-list" :aria-label="t('chat.sessionList.title')">
    <div class="chat-session-list-head">
      <label class="chat-session-list-search">
        <Search :size="13" />
        <input
          ref="searchInput"
          v-model="query"
          class="ui-input"
          type="search"
          :placeholder="t('chat.sessionList.search')"
          :aria-label="t('chat.sessionList.search')"
        />
      </label>
      <UiButton
        class="icon-button"
        variant="ghost"
        size="icon"
        :tip="t('chat.newChatLabel')"
        :disabled="createDisabled"
        @click="emit('create')"
      >
        <MessageSquarePlus :size="15" />
      </UiButton>
    </div>
    <div class="chat-session-list-body vs-scroll">
      <p v-if="!groups.length" class="chat-session-list-empty">
        {{ query.trim() ? t("chat.sessionList.noMatch") : t("chat.sessionList.empty") }}
      </p>
      <section v-for="group in groups" :key="group.key" class="chat-session-list-group">
        <h3>{{ t(`chat.sessionList.group.${group.key}`) }}</h3>
        <div
          v-for="row in group.items"
          :key="row.id"
          class="chat-session-row"
          :class="{ active: row.active, closed: !row.open, renaming: renaming?.id === row.id }"
          @contextmenu="openMenu(row, $event)"
        >
          <input
            v-if="renaming?.id === row.id"
            ref="renameInput"
            v-model="renaming.title"
            class="ui-input chat-session-rename"
            :aria-label="t('chat.sessionList.rename')"
            @keydown="onRenameKeydown"
            @blur="commitRename"
          />
          <template v-else>
            <UiTooltip :content="rowTip(row)">
              <button
                type="button"
                class="chat-session-row-main"
                :aria-current="row.active ? 'true' : undefined"
                @click="emit('select', row.id)"
                @dblclick="startRename(row)"
              >
                <LoaderCircle v-if="row.running" class="chat-session-row-icon process-spinner" :size="13" />
                <MessageSquare v-else class="chat-session-row-icon" :size="13" />
                <span class="chat-session-row-title">{{ row.title }}</span>
                <span v-if="row.awaitingPlan && !row.running" class="chat-session-row-dot" aria-hidden="true" />
                <UiRelativeTime class="chat-session-row-time" :value="row.activityAt" empty="" />
              </button>
            </UiTooltip>
            <span class="chat-session-row-actions">
              <UiButton
                v-if="editable"
                class="icon-button"
                variant="ghost"
                size="icon"
                :tip="t('chat.sessionList.rename')"
                @click="startRename(row)"
              >
                <Pencil :size="12" />
              </UiButton>
              <UiButton
                v-if="row.open"
                class="icon-button"
                variant="ghost"
                size="icon"
                :tip="t('chat.closeTab')"
                @click="emit('close-tab', row.id)"
              >
                <X :size="12" />
              </UiButton>
            </span>
          </template>
        </div>
      </section>
    </div>
    <Teleport to="body">
      <ContextMenu
        v-if="menu"
        :x="menu.x"
        :y="menu.y"
        :items="menuItems"
        @select="onMenuSelect"
        @close="menu = null"
      />
    </Teleport>
  </nav>
</template>
