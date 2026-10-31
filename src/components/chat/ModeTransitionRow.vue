<script setup lang="ts">
import { computed } from "vue";
import { ArrowRightLeft } from "@lucide/vue";
import type { RunMode, TimelineItem } from "../../domain/types";
import { useI18n } from "../../i18n";
import { RUN_MODES, modeLabel } from "./run-mode";
import { resolvedInput, stringField } from "./timeline-display";

const MODE_SOURCES = new Set(["user", "model", "runtime"]);

const props = defineProps<{ item: TimelineItem }>();

const { t } = useI18n();
const input = computed(() => resolvedInput(props.item));

/** ACP 运行时可能报自定义模式 id，认不出的原样显示 */
function displayMode(mode: string) {
  return RUN_MODES.includes(mode as RunMode) ? modeLabel(mode as RunMode) : mode;
}

const text = computed(() => {
  const to = stringField(input.value, ["to"]);
  return to === "plan" ? t("chat.modeChange.enterPlan") : t("chat.modeChange.switchTo", { mode: displayMode(to) });
});
const detail = computed(() => {
  const source = stringField(input.value, ["source"]);
  return [
    MODE_SOURCES.has(source) ? t(`chat.modeChange.source.${source}`) : "",
    stringField(input.value, ["reason"]),
  ].filter(Boolean).join(" · ");
});
</script>

<template>
  <p class="process-mode-change">
    <ArrowRightLeft :size="12" />
    <span>{{ text }}</span>
    <em v-if="detail">{{ detail }}</em>
  </p>
</template>
