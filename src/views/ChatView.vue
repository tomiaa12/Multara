<script setup lang="ts">
import { computed, ref, watch } from "vue";
import ChatPanel from "../components/ChatPanel.vue";
import { useAppStore } from "../stores/app";

const store = useAppStore();
const selectedProjectId = ref<string | null>(null);
const projects = computed(() => store.data?.projects ?? []);
watch(projects, (items) => {
  if (!selectedProjectId.value && items.length) selectedProjectId.value = items[0].id;
}, { immediate: true });
</script>

<template>
  <div class="chat-page">
    <div class="chat-project-bar">
      <strong>聊天</strong>
      <label>需求
        <UiSelect
          v-model="selectedProjectId"
          :options="[{ value: null, label: '通用对话' }, ...projects.map((project) => ({ value: project.id, label: project.name }))]"
        />
      </label>
    </div>
    <ChatPanel :key="selectedProjectId ?? 'global'" :project-id="selectedProjectId" show-session-list />
  </div>
</template>
