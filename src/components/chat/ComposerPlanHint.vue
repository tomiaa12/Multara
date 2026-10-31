<script setup lang="ts">
import { computed, ref } from "vue";
import { ClipboardList, LoaderCircle, MoreHorizontal } from "@lucide/vue";
import { useAppStore } from "../../stores/app";
import { useI18n } from "../../i18n";
import ContextMenu, { type ContextMenuItem } from "../ui/ContextMenu.vue";
import { composerPlanHintKey, latestAwaitingPlan } from "./plan-state";
import { usePlanAction } from "./use-plan-action";

const props = defineProps<{ sessionId: string }>();

const store = useAppStore();
const { t } = useI18n();
const { pendingAction, actionError, act } = usePlanAction(() => props.sessionId);

const copiedKey = ref("");
const menu = ref<{ x: number; y: number } | null>(null);

const states = computed(() => store.planStates(props.sessionId));
const hintKey = computed(() =>
  composerPlanHintKey(
    store.messages[props.sessionId] ?? [],
    states.value,
    store.sessionMode(props.sessionId),
    store.messageRunStatus,
  ),
);
/** 规划轮还在跑时只给提示，不给操作 */
const awaiting = computed(() => (hintKey.value === "chat.plan.hint.steer" ? null : latestAwaitingPlan(states.value)));
const copied = computed(() => Boolean(copiedKey.value) && copiedKey.value === awaiting.value?.key);
const menuItems = computed<ContextMenuItem[]>(() => [
  { id: "reject", label: t("chat.plan.reject"), disabled: Boolean(pendingAction.value) },
  { id: "copy", label: t("chat.plan.copy") },
]);

function openMenu(event: MouseEvent) {
  menu.value = { x: event.clientX, y: event.clientY };
}

async function onMenu(id: string) {
  menu.value = null;
  if (id === "reject") {
    await act(awaiting.value, "reject");
    return;
  }
  await navigator.clipboard.writeText(awaiting.value?.artifact.markdown ?? "");
  copiedKey.value = awaiting.value?.key ?? "";
}
</script>

<template>
  <!-- 有待回答的提问时让位给输入框上方的提问卡片，答完作为修改意见再出新一版 -->
  <div v-if="hintKey && !store.awaitingAnswer(sessionId)" class="composer-plan-hint">
    <ClipboardList :size="13" />
    <span class="composer-plan-hint-text">{{ copied ? t("chat.plan.copied") : t(hintKey) }}</span>
    <template v-if="awaiting">
      <UiButton size="small" variant="primary" :disabled="Boolean(pendingAction)" @click="act(awaiting, 'approve')">
        <LoaderCircle v-if="pendingAction === 'approve'" class="process-spinner" :size="12" />
        {{ t("chat.plan.approve") }}
      </UiButton>
      <UiButton size="icon" variant="ghost" :tip="t('chat.plan.more')" :aria-label="t('chat.plan.more')" @click="openMenu">
        <LoaderCircle v-if="pendingAction === 'reject'" class="process-spinner" :size="13" />
        <MoreHorizontal v-else :size="14" />
      </UiButton>
    </template>
    <ContextMenu v-if="menu" :x="menu.x" :y="menu.y" :items="menuItems" @select="onMenu" @close="menu = null" />
  </div>
  <UiNotice v-if="actionError" tone="error" sticky :message="actionError" @close="actionError = ''" />
</template>
