<script setup lang="ts">
import { computed } from "vue";
import { ClipboardList, Sparkles, Zap } from "@lucide/vue";
import type { RunMode } from "../../domain/types";
import { useAppStore } from "../../stores/app";
import { useRemoteStore } from "../../stores/remote";
import { useI18n } from "../../i18n";
import type { SelectOption } from "../ui/select-types";
import {
  RUN_MODES,
  agentRuntime,
  modeDescriptionKey,
  modeLabel,
  modeSupported,
  modeUnsupportedReason,
  sessionLeadAgentId,
} from "./run-mode";

const props = defineProps<{
  sessionId: string;
}>();

const MODE_ICONS = { auto: Sparkles, plan: ClipboardList, agent: Zap } satisfies Record<RunMode, unknown>;

const PLAN_MODE: RunMode = "plan";

const store = useAppStore();
const remote = useRemoteStore();
const { t } = useI18n();

const runtime = computed(() => {
  const session = store.data?.sessions.find((item) => item.id === props.sessionId);
  return agentRuntime(store.data, sessionLeadAgentId(store.data, session));
});

const options = computed<SelectOption<RunMode>[]>(() =>
  RUN_MODES.map((mode) => {
    const supported = modeSupported(runtime.value, mode);
    const lockTip = remote.lockTip(mode === PLAN_MODE ? "agentPlan" : "agentRun");
    return {
      value: mode,
      label: modeLabel(mode),
      description: lockTip || (supported ? t(modeDescriptionKey(runtime.value, mode)) : modeUnsupportedReason(runtime.value)),
      disabled: !supported || Boolean(lockTip),
    };
  }),
);

const mode = computed({
  get: () => store.sessionMode(props.sessionId),
  set: (value: RunMode) => {
    store.setSessionMode(props.sessionId, value).catch((cause: unknown) => {
      store.reportComposerNotice(
        t("chat.mode.switchFailed", { message: cause instanceof Error ? cause.message : String(cause) }),
        "",
        props.sessionId,
      );
    });
  },
});
</script>

<template>
  <UiSelect
    v-model="mode"
    class="chat-mode-picker"
    variant="ghost"
    size="sm"
    :searchable="false"
    :options="options"
    :aria-label="t('chat.mode.pickerLabel')"
    :title="t('chat.mode.pickerLabel')"
  >
    <template #prefix>
      <component :is="MODE_ICONS[mode] ?? Sparkles" :size="14" />
    </template>
  </UiSelect>
</template>
