<script setup lang="ts">
import { computed } from "vue";
import { ChevronDown } from "@lucide/vue";
import type { ExploreEntry, FileRevealTarget, PairedTool } from "./timeline-display";
import { exploreDetail, exploreTitle, exploreVerb, fileRevealTarget, itemKind } from "./timeline-display";
import ThinkingLine from "./ThinkingLine.vue";
import { useAppStore } from "../../stores/app";

const props = defineProps<{ items: ExploreEntry[]; running?: boolean; runningTail?: boolean }>();
const store = useAppStore();
const language = computed(() => store.data?.settings.language);
const title = computed(() => exploreTitle(props.items, language.value));

function openTarget(target: FileRevealTarget | null) {
  if (!target) return;
  store.revealWorkspaceFile(target.path, target.startLine, target.endLine);
}

function onRead(item: PairedTool) {
  openTarget(fileRevealTarget(item.use));
}

function canReveal(item: PairedTool) {
  return itemKind(item.use) === "read" && Boolean(fileRevealTarget(item.use)) && Boolean(item.result);
}

function entryKey(entry: ExploreEntry) {
  if (entry.kind === "thinking") return `think-${entry.item.seq}`;
  return entry.item.use.callId || String(entry.item.use.seq);
}
</script>

<template>
  <details class="explore-group">
    <summary>
      <span>{{ title }}</span>
      <ChevronDown :size="12" />
    </summary>
    <div class="explore-reads">
      <template v-for="(entry, index) in items" :key="entryKey(entry)">
        <ThinkingLine
          v-if="entry.kind === 'thinking'"
          :item="entry.item"
          :running="runningTail && index === items.length - 1"
        />
        <component
          :is="canReveal(entry.item) ? 'button' : 'div'"
          v-else
          class="read-row-line"
          :class="{
            'is-running': running && !entry.item.result,
            'is-clickable': canReveal(entry.item),
          }"
          :type="canReveal(entry.item) ? 'button' : undefined"
          :title="fileRevealTarget(entry.item.use)?.path || exploreDetail(entry.item.use)"
          @click="canReveal(entry.item) ? onRead(entry.item) : undefined"
        >
          <UiShimmerText
            class="read-row-text"
            :text="`${exploreVerb(entry.item.use, language)} ${exploreDetail(entry.item.use)}`"
            :active="Boolean(running && !entry.item.result)"
          />
        </component>
      </template>
    </div>
  </details>
</template>
