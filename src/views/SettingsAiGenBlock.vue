<script setup lang="ts">
import { computed, watch } from "vue";
import { GEN_MODE_AGENT, GEN_MODE_API } from "../domain/ai-helper-defaults";
import { t } from "../i18n";

const props = defineProps<{
  agentOptions: Array<{ value: string; label: string; avatar: string }>;
  defaultPrompt: string;
}>();

const mode = defineModel<string>("mode", { required: true });
const agentId = defineModel<string>("agentId", { required: true });
const apiUrl = defineModel<string>("apiUrl", { required: true });
const apiModel = defineModel<string>("apiModel", { required: true });
const apiKey = defineModel<string>("apiKey", { required: true });
const prompt = defineModel<string>("prompt", { required: true });

const isApi = computed(() => mode.value === GEN_MODE_API);

const agentTarget = computed({
  get: () => {
    if (!agentId.value) return props.agentOptions[0]?.value ?? "";
    if (agentId.value.startsWith("agent:") || agentId.value.startsWith("squad:")) return agentId.value;
    return `agent:${agentId.value}`;
  },
  set: (value: string) => {
    agentId.value = value;
  },
});

/** 与聊天输入框同一套分组：智能体 / 小队 */
const targetGroups = computed(() => {
  const agents = props.agentOptions.filter((option) => !option.value.startsWith("squad:"));
  const squads = props.agentOptions
    .filter((option) => option.value.startsWith("squad:"))
    .map((option) => ({ ...option, label: option.label.replace(/^小队 · /, "") }));
  return [
    { label: "智能体", options: agents },
    { label: "小队", options: squads },
  ].filter((group) => group.options.length);
});

watch(
  () => props.agentOptions,
  (options) => {
    if (agentId.value || !options.length) return;
    const first = options[0]?.value;
    if (first) agentTarget.value = first;
  },
  { immediate: true },
);

function resetPrompt(fallback: string) {
  prompt.value = fallback;
}
</script>

<template>
  <div class="settings-row">
    <div>
      <strong>{{ t("settings.gen.source") }}</strong>
      <p>{{ t("settings.gen.sourceHint") }}</p>
    </div>
    <div class="ds-segmented" role="group" :aria-label="t('settings.gen.source')">
      <button
        type="button"
        :class="{ active: mode === GEN_MODE_AGENT }"
        :aria-pressed="mode === GEN_MODE_AGENT"
        @click="mode = GEN_MODE_AGENT"
      >{{ t("settings.gen.source.agent") }}</button>
      <button
        type="button"
        :class="{ active: mode === GEN_MODE_API }"
        :aria-pressed="mode === GEN_MODE_API"
        @click="mode = GEN_MODE_API"
      >{{ t("settings.gen.source.api") }}</button>
    </div>
  </div>

  <div class="settings-ai-gen-body">
    <label v-if="!isApi" class="target-picker settings-ai-gen-agent">
      <UiSelect
        v-model="agentTarget"
        variant="ghost"
        size="sm"
        :aria-label="t('settings.gen.agent')"
        :groups="targetGroups"
      />
    </label>

    <div v-else class="settings-api-grid">
      <label class="settings-outline-field settings-api-url">
        <span>{{ t("settings.gen.apiUrl") }}</span>
        <UiInput
          v-model="apiUrl"
          spellcheck="false"
          :placeholder="t('settings.gen.apiUrlPlaceholder')"
        />
      </label>
      <label class="settings-outline-field settings-api-model">
        <span>{{ t("settings.gen.apiModel") }}</span>
        <UiInput v-model="apiModel" spellcheck="false" :placeholder="t('settings.gen.apiModelPlaceholder')" />
      </label>
      <label class="settings-outline-field settings-api-key">
        <span>{{ t("settings.gen.apiKey") }}</span>
        <UiInput
          v-model="apiKey"
          type="password"
          autocomplete="off"
          spellcheck="false"
          :placeholder="t('settings.gen.apiKeyPlaceholder')"
        />
      </label>
    </div>

    <div class="settings-prompt-head">
      <div>
        <strong>{{ t("settings.gen.prompt") }}</strong>
        <p>{{ t("settings.gen.promptHint") }}</p>
      </div>
      <UiButton type="button" @click="resetPrompt(defaultPrompt)">{{ t("settings.gen.resetPrompt") }}</UiButton>
    </div>
    <UiTextarea v-model="prompt" rows="6" />
  </div>
</template>
