<script setup lang="ts">
/*
 * 公共智能体/小队下拉：与聊天输入框同一套选项（头像、运行时 Logo、模型、描述）。
 */
import { computed } from "vue";
import { Users } from "@lucide/vue";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import AgentAvatar from "./AgentAvatar.vue";
import ProviderLogo from "./ProviderLogo.vue";
import {
  buildChatTargetOptions,
  groupChatTargetOptions,
  type ChatTargetOption,
} from "./chat/chat-target-options";
import type { SelectOption } from "./ui/select-types";

const model = defineModel<string>({ default: "" });

const props = withDefaults(defineProps<{
  /** 外部传入选项；不传则从 store 拉全部智能体/小队 */
  options?: SelectOption<string>[];
  /** 提供时在顶部加「未设置」等空选项 */
  noneLabel?: string;
  required?: boolean;
  disabled?: boolean;
  placeholder?: string;
  size?: "sm" | "md" | "lg";
  variant?: "default" | "ghost";
  searchable?: boolean;
  pageSize?: number;
}>(), {
  noneLabel: undefined,
  required: false,
  disabled: false,
  placeholder: undefined,
  size: "md",
  variant: "default",
  searchable: true,
  pageSize: 5,
});

const store = useAppStore();

const resolvedOptions = computed(() =>
  props.options ?? buildChatTargetOptions(
    store.data?.agents ?? [],
    store.data?.squads ?? [],
    (name) => t("chat.squadLabel", { name }),
    store.data?.runtimes ?? [],
  ),
);

const groups = computed(() => {
  const base = groupChatTargetOptions(resolvedOptions.value, {
    agents: t("nav.agents"),
    squads: t("nav.squads"),
  });
  if (!props.noneLabel) return base;
  return [
    { label: "", options: [{ value: "", label: props.noneLabel }] },
    ...base,
  ].filter((group) => group.options.length);
});

function isAgentTargetOption(option: SelectOption | null | undefined): option is ChatTargetOption {
  return Boolean(option && String(option.value).startsWith("agent:"));
}

function isSquadTargetOption(option: SelectOption | null | undefined) {
  return Boolean(option && String(option.value).startsWith("squad:"));
}
</script>

<template>
  <UiSelect
    v-model="model"
    :groups="groups"
    :required="required"
    :disabled="disabled || (!noneLabel && !resolvedOptions.length)"
    :placeholder="placeholder"
    :size="size"
    :variant="variant"
    :searchable="searchable"
    :page-size="pageSize"
  >
    <template #option-leading="{ option }">
      <span v-if="isSquadTargetOption(option)" class="squad-mark target-squad-mark" aria-hidden="true">
        <Users :size="12" />
      </span>
      <AgentAvatar
        v-else-if="option?.avatar"
        class="ds-avatar"
        :value="option.avatar"
        :size="null"
      />
    </template>
    <template #option-description="{ option, parts }">
      <span
        v-if="isAgentTargetOption(option)"
        class="target-option-runtime"
      >
        <ProviderLogo
          v-if="option.runtimeProvider"
          class="target-option-runtime-logo"
          :provider="option.runtimeProvider"
          :size="12"
        />
        <span v-if="option.runtimeName" class="target-option-runtime-name">{{ option.runtimeName }}</span>
        <span class="target-option-runtime-model">{{ option.runtimeModel || "auto" }}</span>
        <span v-if="option.agentDescription" class="target-option-runtime-desc">{{ option.agentDescription }}</span>
      </span>
      <template v-else-if="parts">{{ parts[0] }}<mark v-if="parts[1]">{{ parts[1] }}</mark>{{ parts[2] }}</template>
    </template>
  </UiSelect>
</template>
