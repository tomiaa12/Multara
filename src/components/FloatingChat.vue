<script setup lang="ts">
import { computed, ref } from "vue";
import { t } from "../i18n";
import { Maximize2, MessageCircle, Minimize2, Minus } from "@lucide/vue";
import { useRoute } from "vue-router";
import ChatPanel from "./ChatPanel.vue";
import { useAppStore } from "../stores/app";

const store = useAppStore();
const route = useRoute();
const projectId = computed(() => typeof route.params.projectId === "string" ? route.params.projectId : null);
const expanded = ref(false);
</script>

<template>
  <Transition name="floating-chat">
    <section v-if="store.chatOpen" class="floating-chat" :class="{ expanded }">
      <ChatPanel :key="projectId ?? 'global'" :project-id="projectId" compact>
        <template #header-actions>
          <UiButton class="icon-button" variant="ghost" size="icon" :title="expanded ? t('chat.floatRestore') : t('chat.floatExpand')" @click="expanded = !expanded">
            <Minimize2 v-if="expanded" :size="15" />
            <Maximize2 v-else :size="15" />
          </UiButton>
          <UiButton class="icon-button" variant="ghost" size="icon" :title="t('chat.floatMinimize')" @click="store.chatOpen = false">
            <Minus :size="16" />
          </UiButton>
        </template>
      </ChatPanel>
    </section>
  </Transition>

  <UiButton v-if="!store.chatOpen" class="chat-fab" :title="t('chat.floatOpen')" @click="store.chatOpen = true">
    <MessageCircle :size="22" />
  </UiButton>
</template>
