<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { t } from "../i18n";
import { Maximize2, MessageCircle, Minimize2, Minus } from "@lucide/vue";
import { useRoute } from "vue-router";
import ChatPanel from "./ChatPanel.vue";
import { dictationService } from "../dictation/dictation-service";
import { useAppStore } from "../stores/app";

const props = defineProps<{
  /** 需求工作台等自带聊天时隐藏，但保持挂载以免丢会话 */
  hidden?: boolean;
}>();

const store = useAppStore();
const route = useRoute();
const projectId = computed(() => typeof route.params.projectId === "string" ? route.params.projectId : null);
const expanded = ref(false);
/** 首次打开后再常驻，避免启动就 ensureSession；收起/切页不再拆掉 */
const retained = ref(false);
watch(
  () => store.chatOpen,
  (open) => {
    if (open) retained.value = true;
    else void dictationService.stop();
  },
  { immediate: true },
);
watch(
  () => props.hidden,
  (hidden) => {
    if (hidden) void dictationService.stop();
  },
);
/** host 已在 hidden 时整体 v-show=false；此处只区分面板 / FAB */
const panelOpen = computed(() => store.chatOpen);
const fabVisible = computed(() => !store.chatOpen);
/** 收起时有待确认的计划就在气泡按钮上亮点，只看已加载过消息的会话 */
const pendingPlan = computed(() =>
  store.openedSessionIds(projectId.value).some((sessionId) => Boolean(store.planStates(sessionId).latestAwaitingKey)),
);
</script>

<template>
  <!-- 工作台等自带聊天时整块隐藏；ChatPanel 仍用 v-show/retained，避免拆掉丢会话 -->
  <div v-show="!hidden" class="floating-chat-host">
    <Transition name="floating-chat">
      <section v-show="panelOpen" class="floating-chat" :class="{ expanded }">
        <ChatPanel
          v-if="retained"
          :key="projectId ?? 'global'"
          :project-id="projectId"
          compact
          session-commands
          :show-editing-bar="false"
        >
          <template #header-actions>
            <UiButton class="icon-button" variant="ghost" size="icon" :tip="expanded ? t('chat.floatRestore') : t('chat.floatExpand')" @click="expanded = !expanded">
              <Minimize2 v-if="expanded" :size="15" />
              <Maximize2 v-else :size="15" />
            </UiButton>
            <UiButton class="icon-button" variant="ghost" size="icon" :tip="t('chat.floatMinimize')" @click="store.chatOpen = false">
              <Minus :size="16" />
            </UiButton>
          </template>
        </ChatPanel>
      </section>
    </Transition>

    <UiButton
      v-show="fabVisible"
      class="chat-fab"
      :tip="pendingPlan ? t('chat.plan.pendingBadge') : t('chat.floatOpen')"
      @click="store.chatOpen = true"
    >
      <MessageCircle :size="22" />
      <span v-if="pendingPlan" class="chat-fab-dot" aria-hidden="true" />
    </UiButton>
  </div>
</template>
