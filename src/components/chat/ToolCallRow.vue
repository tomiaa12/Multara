<script setup lang="ts">
import { computed } from "vue";
import { ChevronRight } from "@lucide/vue";
import type { TimelineItem } from "../../domain/types";
import { exploreDetail, inferToolName } from "./timeline-display";

const props = defineProps<{ item: TimelineItem; running?: boolean; output?: string | null }>();
const name = computed(() => inferToolName(props.item) || "Tool");
const detail = computed(() => exploreDetail(props.item));
const output = computed(() => (props.output || props.item.output || "").trim());
const failed = computed(() => props.item.status === "error");
const expandable = computed(() => Boolean(props.item.input || props.item.content || output.value));
</script>

<template>
  <details class="read-row" :class="{ 'is-running': running }">
    <summary>
      <UiShimmerText class="read-row-text" :text="`${name} ${detail}`" :active="Boolean(running)" />
      <ChevronRight v-if="expandable" :size="12" />
    </summary>
    <pre v-if="item.input" class="process-detail vs-scroll">{{ JSON.stringify(item.input, null, 2) }}</pre>
    <div v-else-if="item.content" class="process-detail vs-scroll">{{ item.content }}</div>
    <pre v-if="output" class="process-detail vs-scroll" :class="{ 'is-error': failed }">{{ output }}</pre>
  </details>
</template>
