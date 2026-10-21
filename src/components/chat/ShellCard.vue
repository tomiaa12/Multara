<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronRight } from "@lucide/vue";
import type { PairedTool } from "./timeline-display";
import { shellResultText, shellTranscript, toolCommand, toolDescription } from "./timeline-display";

const props = defineProps<{ item: PairedTool }>();
const open = ref(false);
const command = computed(() => toolCommand(props.item.use).trim());
const output = computed(() => shellResultText(props.item.result?.output || props.item.result?.content || ""));
const transcript = computed(() => shellTranscript(command.value, output.value));
const title = computed(() => {
  const description = toolDescription(props.item.use);
  if (description) return description;
  const first = command.value.split("\n")[0] ?? "";
  return first.length > 72 ? `${first.slice(0, 69)}…` : first || props.item.use.tool || "Command";
});

function toggle() {
  open.value = !open.value;
}

function expandFromPreview() {
  if (!open.value) open.value = true;
}
</script>

<template>
  <article class="shell-card" :class="{ 'is-open': open }">
    <button type="button" class="shell-summary" :aria-expanded="open" @click="toggle">
      <ChevronRight class="shell-chevron" :size="12" />
      <span>{{ title }}</span>
    </button>
    <div
      v-if="transcript"
      class="shell-body"
      :class="{ 'is-collapsed': !open }"
      @click="expandFromPreview"
    >
      <pre class="shell-output">{{ transcript }}</pre>
    </div>
  </article>
</template>
