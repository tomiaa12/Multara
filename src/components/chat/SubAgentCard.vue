<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronRight } from "@lucide/vue";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import ProcessFold from "./ProcessFold.vue";
import type { PairedTool } from "./timeline-display";
import {
  MAX_INDENTED_DEPTH,
  subAgentFailureLine,
  subAgentResultText,
  subAgentStatus,
  subAgentSummary,
  subAgentType,
} from "./subagent-display";

const props = defineProps<{
  item: PairedTool;
  /** 子代理所在层级，根过程里的子代理为 1 */
  depth: number;
  /** 根运行是否还在进行 */
  running?: boolean;
  defaultOpen?: boolean;
  runId?: string | null;
  defaultCollapseSubagents?: boolean;
}>();

const store = useAppStore();
// 用户点过之后以用户为准；否则跟随父级给的默认展开（运行中只展开最后一个在跑的）。
const userOpen = ref<boolean | null>(null);
const open = computed(() => userOpen.value ?? Boolean(props.defaultOpen));
const status = computed(() => subAgentStatus(props.item, props.running));
const summary = computed(() => subAgentSummary(props.item, status.value, store.data?.settings.language));
const failure = computed(() => (status.value === "failed" ? subAgentFailureLine(props.item) : ""));
const resultText = computed(() => subAgentResultText(props.item));
const deep = computed(() => props.depth > MAX_INDENTED_DEPTH);
</script>

<template>
  <article class="subagent-card" :class="{ 'is-open': open }" :data-status="status">
    <button type="button" class="subagent-summary" :aria-expanded="open" @click="userOpen = !open">
      <ChevronRight class="subagent-chevron" :size="12" />
      <UiShimmerText class="subagent-title" :text="summary" :active="status === 'running'" />
      <em v-if="subAgentType(item)" class="subagent-type">{{ subAgentType(item) }}</em>
      <em v-if="deep" class="subagent-depth">L{{ depth }}</em>
    </button>
    <p v-if="failure" class="subagent-failure">{{ failure }}</p>
    <div v-if="open" class="subagent-body" :class="{ 'is-deep': deep }">
      <ProcessFold
        v-if="item.children?.length"
        :blocks="item.children"
        :running="status === 'running'"
        :depth="depth"
        :run-id="runId"
        :default-collapse-subagents="defaultCollapseSubagents"
      />
      <section v-if="resultText" class="subagent-result">
        <header>{{ t("chat.subagent.result") }}</header>
        <div class="process-detail vs-scroll">{{ resultText }}</div>
      </section>
    </div>
  </article>
</template>
