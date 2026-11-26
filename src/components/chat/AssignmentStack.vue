<script setup lang="ts">
import { LoaderCircle } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import { t } from "../../i18n";

defineProps<{
  working: { name: string; avatar: string; label: string; elapsed: string } | null;
}>();
</script>

<template>
  <div class="assignment-stack">
    <slot />
    <div v-if="working" class="assignment-card-working">
      <AgentAvatar class="assistant-avatar" :value="working.avatar" :name="working.name" :size="22" />
      <div class="assignment-card-working-copy">
        <div class="speaker-meta">
          <b>{{ working.name }}</b>
          <span class="speaker-pill">{{ t("chat.role.agent") }}</span>
        </div>
        <span class="assistant-working-lines">
          <span class="assistant-working-line">
            <LoaderCircle :size="12" />
            <UiShimmerText :text="working.label" rotate />
            <span>· {{ working.elapsed }}</span>
          </span>
        </span>
      </div>
    </div>
  </div>
</template>
