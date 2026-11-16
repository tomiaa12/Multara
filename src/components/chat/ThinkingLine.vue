<script setup lang="ts">
import { computed } from "vue";
import { ChevronRight } from "@lucide/vue";
import type { TimelineItem } from "../../domain/types";
import { useAppStore } from "../../stores/app";
import { thinkingSummary } from "./timeline-display";
import ClipScroll from "./ClipScroll.vue";

const props = defineProps<{ item: TimelineItem; running?: boolean }>();
const store = useAppStore();
const summary = computed(() =>
  thinkingSummary(props.item, props.running, store.data?.settings.language),
);
const detail = computed(() => (props.item.content ?? "").trim());
</script>

<template>
  <details v-if="detail" class="thinking-line">
    <summary>
      <UiShimmerText :text="summary" :active="Boolean(running)" />
      <ChevronRight :size="12" />
    </summary>
    <ClipScroll class="process-detail">{{ detail }}</ClipScroll>
  </details>
  <div v-else class="thinking-line"><UiShimmerText :text="summary" :active="Boolean(running)" /></div>
</template>
