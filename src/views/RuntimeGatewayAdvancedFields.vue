<script setup lang="ts">
import { t } from "../i18n";
import type { GatewayDraft } from "./runtime-catalog";

type GatewayAdvanced = Pick<
  GatewayDraft,
  | "compatibilityMode"
  | "fullUrl"
  | "customUserAgent"
  | "customHeadersText"
  | "reasoningEffort"
  | "promptCache"
>;

const props = defineProps<{ modelValue: GatewayAdvanced }>();
const emit = defineEmits<{ "update:modelValue": [value: GatewayAdvanced] }>();

function update<K extends keyof GatewayAdvanced>(key: K, value: GatewayAdvanced[K]) {
  emit("update:modelValue", { ...props.modelValue, [key]: value });
}
</script>

<template>
  <label>{{ t("runtime.gateway.compatibilityMode") }}
    <UiSelect
      :model-value="modelValue.compatibilityMode"
      :options="[
        { value: 'auto', label: t('runtime.gateway.compatibility.auto') },
        { value: 'generic', label: t('runtime.gateway.compatibility.generic') },
        { value: 'openai', label: 'OpenAI' },
        { value: 'codex_oauth', label: 'Codex OAuth' },
        { value: 'xai', label: 'xAI' },
        { value: 'moonshot', label: 'Moonshot' },
        { value: 'opencode', label: 'OpenCode' },
        { value: 'openrouter', label: 'OpenRouter' },
        { value: 'gemini', label: 'Gemini' },
      ]"
      @change="update('compatibilityMode', String($event) as GatewayAdvanced['compatibilityMode'])"
    />
    <small>{{ t("runtime.gateway.compatibilityHint") }}</small>
  </label>
  <label class="check-row">
    <UiInput
      :model-value="modelValue.fullUrl"
      type="checkbox"
      @update:model-value="update('fullUrl', Boolean($event))"
    />
    {{ t("runtime.gateway.fullUrl") }}
  </label>
  <label>{{ t("runtime.gateway.reasoningEffort") }}
    <UiSelect
      :model-value="modelValue.reasoningEffort"
      :options="['auto', 'low', 'medium', 'high', 'xhigh', 'max'].map((value) => ({ value, label: value }))"
      @change="update('reasoningEffort', String($event) as GatewayAdvanced['reasoningEffort'])"
    />
  </label>
  <label class="check-row">
    <UiInput
      :model-value="modelValue.promptCache"
      type="checkbox"
      @update:model-value="update('promptCache', Boolean($event))"
    />
    {{ t("runtime.gateway.promptCache") }}
  </label>
  <label>{{ t("runtime.gateway.userAgent") }}
    <UiInput
      :model-value="modelValue.customUserAgent"
      spellcheck="false"
      @update:model-value="update('customUserAgent', String($event))"
    />
  </label>
  <label>{{ t("runtime.gateway.customHeaders") }}
    <UiTextarea
      :model-value="modelValue.customHeadersText"
      rows="3"
      placeholder="X-Custom-Header: value"
      @update:model-value="update('customHeadersText', String($event))"
    />
  </label>
</template>
