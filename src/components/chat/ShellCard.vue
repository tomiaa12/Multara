<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronRight } from "@lucide/vue";
import Codicon from "../ui/Codicon.vue";
import type { PairedTool } from "./timeline-display";
import {
  shellCardMode,
  shellFailed,
  shellResultText,
  shellTranscript,
  toolCommand,
  toolDescription,
  type ShellCardMode,
} from "./timeline-display";
import ClipScroll from "./ClipScroll.vue";

const props = defineProps<{ item: PairedTool; running?: boolean }>();
const userMode = ref<ShellCardMode | null>(null);
const command = computed(() => toolCommand(props.item.use).trim());
const output = computed(() => shellResultText(props.item.result?.output || props.item.result?.content || ""));
const transcript = computed(() => shellTranscript(command.value, output.value));
const title = computed(() => {
  const description = toolDescription(props.item.use);
  if (description) return description;
  const first = command.value.split("\n")[0] ?? "";
  return first.length > 72 ? `${first.slice(0, 69)}…` : first || props.item.use.tool || "Command";
});
const settled = computed(() => props.running === true ? false : Boolean(props.item.result) || props.running === false);
const mode = computed(() => shellCardMode({
  settled: settled.value,
  failed: shellFailed(props.item),
  hasOutput: Boolean(output.value.trim()),
  user: userMode.value,
}));
const open = computed(() => mode.value === "full");
const showBody = computed(() => mode.value !== "hidden" && Boolean(transcript.value));
const terminalIcon = computed(() => !settled.value && userMode.value === null);

function toggle() {
  if (mode.value === "hidden") {
    userMode.value = "peek";
    return;
  }
  userMode.value = mode.value === "full" ? "peek" : "full";
}

function expandFromPreview() {
  userMode.value = "full";
}
</script>

<template>
  <article class="shell-card" :class="{ 'is-open': open, 'is-title-only': !showBody }">
    <button type="button" class="shell-summary" :aria-expanded="showBody" @click="toggle">
      <Codicon v-if="terminalIcon" class="shell-terminal-icon" name="terminal" :size="14" />
      <ChevronRight v-else class="shell-chevron" :size="12" />
      <UiShimmerText class="shell-title" :text="title" :active="!settled" />
    </button>
    <ClipScroll
      v-if="showBody"
      pass-wheel
      class="shell-body"
      :class="{ 'is-collapsed': !open }"
      :pin-end="!open"
      @click="expandFromPreview"
    >
      <pre class="shell-output">{{ transcript }}</pre>
    </ClipScroll>
  </article>
</template>
