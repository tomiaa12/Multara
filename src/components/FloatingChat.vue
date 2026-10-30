<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { t } from "../i18n";
import { Maximize2, MessageCircle, Minimize2, Minus } from "@lucide/vue";
import { useRoute } from "vue-router";
import ChatPanel from "./ChatPanel.vue";
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
  },
  { immediate: true },
);
const panelOpen = computed(() => store.chatOpen && !props.hidden);
const fabVisible = computed(() => !store.chatOpen && !props.hidden);
</script>

<template>
  <!-- 收起/换页用 v-show，保留会话/滚动；勿用 v-if 把 ChatPanel 拆掉 -->
  <Transition name="floating-chat">
    <section v-show="panelOpen" class="floating-chat" :class="{ expanded }">
      <ChatPanel v-if="retained" :key="projectId ?? 'global'" :project-id="projectId" compact session-commands>
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

  <UiButton v-show="fabVisible" class="chat-fab" :tip="t('chat.floatOpen')" @click="store.chatOpen = true">
    <MessageCircle :size="22" />
  </UiButton>
</template>
