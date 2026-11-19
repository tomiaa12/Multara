<script setup lang="ts">
import { computed } from "vue";
import { vDetailsMotion } from "../../ui/motion";
import { ChevronRight } from "@lucide/vue";
import type { TimelineItem } from "../../domain/types";
import { exploreDetail, inferToolName } from "./timeline-display";
import ClipScroll from "./ClipScroll.vue";

const props = defineProps<{ item: TimelineItem; running?: boolean; output?: string | null }>();
const name = computed(() => inferToolName(props.item) || "Tool");
const detail = computed(() => exploreDetail(props.item));
const output = computed(() => (props.output || props.item.output || "").trim());
const failed = computed(() => props.item.status === "error");
const expandable = computed(() => Boolean(props.item.input || props.item.content || output.value));
</script>

<template>
  <details v-details-motion class="read-row" :class="{ 'is-running': running }">
    <summary>
      <UiShimmerText class="read-row-text" :text="`${name} ${detail}`" :active="Boolean(running)" />
      <ChevronRight v-if="expandable" :size="12" />
    </summary>
    <ClipScroll v-if="item.input" class="process-detail"><pre>{{ JSON.stringify(item.input, null, 2) }}</pre></ClipScroll>
    <ClipScroll v-else-if="item.content" class="process-detail">{{ item.content }}</ClipScroll>
    <ClipScroll v-if="output" class="process-detail" :class="{ 'is-error': failed }"><pre>{{ output }}</pre></ClipScroll>
  </details>
</template>
