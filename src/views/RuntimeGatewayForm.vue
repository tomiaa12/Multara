<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import ProviderLogo from "../components/ProviderLogo.vue";
import { t } from "../i18n";
import * as backend from "../services/backend";
import {
  GATEWAY_FAMILIES,
  familyOf,
  gatewayDraftFor,
  parseCommandLine,
  parseEnvText,
  type RuntimeFamilyId,
} from "./runtime-catalog";

const props = defineProps<{
  id?: string;
  provider: string;
  name: string;
  commandLine: string;
  description: string;
  apiUrl: string;
  initialApiKey?: string;
  hasApiKey: boolean;
  modelsText: string;
  wireApi: string;
  apiFormat: string;
  keyStyle: string;
  envText: string;
  maxConcurrent: number;
  locked: boolean;
  legacy?: boolean;
  busy: boolean;
  error: string;
}>();

const emit = defineEmits<{
  back: [];
  save: [payload: {
    provider: string;
    name: string;
    command: string;
    args: string[];
    description: string;
    apiUrl: string;
    apiKey: string;
    models: string[];
    wireApi: "responses" | "chat";
    apiFormat: "anthropic" | "responses" | "chat" | "gemini";
    keyStyle: "bearer" | "x-api-key";
    env: Record<string, string>;
    maxConcurrent: number;
  }];
}>();

const draft = ref(gatewayDraftFor((GATEWAY_FAMILIES.some((item) => item.id === props.provider) ? props.provider : "claude") as RuntimeFamilyId, {
  name: props.name,
  description: props.description,
  maxConcurrent: props.maxConcurrent,
}));
draft.value.commandLine = props.commandLine || draft.value.commandLine;
draft.value.apiUrl = props.apiUrl;
draft.value.modelsText = props.modelsText;
draft.value.wireApi = props.wireApi === "chat" ? "chat" : "responses";
if (["anthropic", "responses", "chat", "gemini"].includes(props.apiFormat)) {
  draft.value.apiFormat = props.apiFormat as typeof draft.value.apiFormat;
}
draft.value.keyStyle = props.keyStyle === "x-api-key" ? "x-api-key" : "bearer";
draft.value.envText = props.envText;
const apiKey = ref(props.initialApiKey ?? "");
const showKey = ref(false);
const testing = ref("");
const connectionLog = ref("");
const logEl = ref<HTMLElement | null>(null);
let stopLog: (() => void) | undefined;

const spec = computed(() => familyOf(draft.value.provider));
const parsed = computed(() => parseCommandLine(draft.value.commandLine));
const modelLines = computed(() =>
  draft.value.modelsText.split("\n").map((line) => line.trim()).filter(Boolean),
);
const defaultModel = computed({
  get: () => modelLines.value[0] ?? "",
  set: (value: string) => {
    const next = value.trim();
    draft.value.modelsText = [next, ...modelLines.value.slice(1)]
      .filter(Boolean)
      .join("\n");
  },
});
const additionalModels = computed({
  get: () => modelLines.value.slice(1).join("\n"),
  set: (value: string) => {
    draft.value.modelsText = [
      defaultModel.value,
      ...value.split("\n").map((line) => line.trim()).filter(Boolean),
    ].filter(Boolean).join("\n");
  },
});
const useApiKey = computed({
  get: () => draft.value.keyStyle === "x-api-key",
  set: (value: boolean) => {
    draft.value.keyStyle = value ? "x-api-key" : "bearer";
  },
});

function pickCli(id: string) {
  if (props.locked) return;
  const next = gatewayDraftFor(id as RuntimeFamilyId, {
    name: draft.value.name,
    description: draft.value.description,
    maxConcurrent: draft.value.maxConcurrent,
  });
  draft.value = next;
  apiKey.value = "";
}

function pushLog(line: string) {
  const next = line.endsWith("\n") ? line : `${line}\n`;
  if (connectionLog.value.endsWith(next)) return;
  connectionLog.value += next;
  void nextTick(() => {
    const el = logEl.value;
    if (el) el.scrollTop = el.scrollHeight;
  });
}

async function testConnection() {
  testing.value = t("runtime.gateway.testing");
  connectionLog.value = "";
  const parsedLine = parseCommandLine(draft.value.commandLine);
  if (!parsedLine.ok) {
    pushLog(t(`runtime.commandError.${parsedLine.error}`));
    testing.value = "";
    return;
  }
  stopLog?.();
  try {
    stopLog = await listen<string>("runtime-connection-log", (event) => {
      pushLog(String(event.payload ?? ""));
    });
  } catch (cause) {
    pushLog(cause instanceof Error ? cause.message : String(cause));
    testing.value = "";
    return;
  }
  try {
    testing.value = await backend.testRuntimeConnection({
      id: props.id,
      provider: draft.value.provider,
      command: parsedLine.commandName,
      args: parsedLine.fixedArgs,
      apiUrl: draft.value.apiUrl,
      apiKey: apiKey.value,
      model: defaultModel.value,
      wireApi: draft.value.wireApi,
      apiFormat: draft.value.apiFormat,
      keyStyle: draft.value.keyStyle,
      env: parseEnvText(draft.value.envText),
    });
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    if (message && !connectionLog.value.includes(message)) pushLog(message);
    testing.value = "";
  } finally {
    window.setTimeout(() => {
      stopLog?.();
      stopLog = undefined;
    }, 200);
  }
}

onBeforeUnmount(() => stopLog?.());

function submit() {
  if (!parsed.value.ok || !draft.value.name.trim() || !draft.value.apiUrl.trim() || !defaultModel.value) return;
  if (!apiKey.value.trim() && !props.hasApiKey) return;
  emit("save", {
    provider: draft.value.provider,
    name: draft.value.name.trim(),
    command: parsed.value.commandName,
    args: parsed.value.fixedArgs,
    description: draft.value.description.trim(),
    apiUrl: draft.value.apiUrl.trim().replace(/\/+$/, ""),
    apiKey: apiKey.value,
    models: modelLines.value,
    wireApi: draft.value.apiFormat === "chat" ? "chat" : "responses",
    apiFormat: draft.value.apiFormat,
    keyStyle: draft.value.keyStyle,
    env: parseEnvText(draft.value.envText),
    maxConcurrent: Number(draft.value.maxConcurrent) || 2,
  });
}
</script>

<template>
  <p v-if="legacy" class="ds-alert">{{ t("runtime.legacyHint") }}</p>
  <label>{{ t("runtime.gateway.cli") }}
    <UiSelect
      :model-value="draft.provider"
      :disabled="locked"
      :options="GATEWAY_FAMILIES.map((item) => ({ value: item.id, label: t(`runtime.family.${item.id}`) }))"
      @change="pickCli(String($event))"
    />
  </label>
  <div class="runtime-base-lock">
    <ProviderLogo :provider="spec.id" :size="18" />
    <small>{{ t(`runtime.gateway.urlHint.${spec.id}`) }}</small>
  </div>
  <label>{{ t("runtime.field.name") }}<UiInput v-model="draft.name" required /></label>
  <label>{{ t("runtime.field.url") }}<UiInput v-model="draft.apiUrl" required placeholder="https://gateway.example.com/v1" /></label>
  <label>{{ t("runtime.field.key") }}
    <span class="runtime-key-field">
      <UiInput
        v-model="apiKey"
        :type="showKey ? 'text' : 'password'"
        autocomplete="off"
        spellcheck="false"
        :placeholder="t('runtime.keyPlaceholder')"
      />
      <button
        type="button"
        class="runtime-key-toggle"
        :title="showKey ? '隐藏 API Key' : '显示 API Key'"
        :aria-pressed="showKey"
        @click="showKey = !showKey"
      >
        <Codicon :name="showKey ? 'eye-closed' : 'eye'" :size="14" />
      </button>
    </span>
  </label>
  <label>{{ t("runtime.field.defaultModel") }}<UiInput v-model="defaultModel" placeholder="gpt-4.1" /></label>
  <label>{{ t("runtime.field.additionalModels") }}<UiTextarea v-model="additionalModels" rows="3" :placeholder="t('runtime.field.additionalModelsPlaceholder')" /></label>
  <label>{{ t("runtime.gateway.apiFormat") }}
    <UiSelect
      v-model="draft.apiFormat"
      :options="[
        { value: 'anthropic', label: t('runtime.gateway.apiFormat.anthropic') },
        { value: 'responses', label: t('runtime.gateway.apiFormat.responses') },
        { value: 'chat', label: t('runtime.gateway.apiFormat.chat') },
        { value: 'gemini', label: t('runtime.gateway.apiFormat.gemini') },
      ]"
    />
    <small>{{ t("runtime.gateway.apiFormatHint") }}</small>
  </label>
  <label v-if="draft.provider === 'claude'" class="check-row">
    <UiInput v-model="useApiKey" type="checkbox" />
    {{ t("runtime.gateway.keyStyle") }}
  </label>
  <details class="runtime-advanced">
    <summary>{{ t("runtime.advanced") }}</summary>
    <label>{{ t("runtime.field.commandLine") }}<UiInput v-model="draft.commandLine" spellcheck="false" /></label>
    <p v-if="!parsed.ok && draft.commandLine.trim()" class="form-error">{{ t(`runtime.commandError.${parsed.error}`) }}</p>
    <label>{{ t("runtime.field.description") }}<UiTextarea v-model="draft.description" rows="2" /></label>
    <label>{{ t("runtime.field.env") }}<UiTextarea v-model="draft.envText" rows="3" placeholder="FOO=bar" /></label>
    <label>{{ t("runtime.field.concurrent") }}
      <UiSelect v-model="draft.maxConcurrent" :options="Array.from({ length: 6 }, (_, index) => ({ value: index + 1, label: String(index + 1) }))" />
    </label>
  </details>
  <div class="runtime-test-row">
    <UiButton type="button" :disabled="busy || testing === t('runtime.gateway.testing')" @click="testConnection">{{ t("runtime.gateway.test") }}</UiButton>
    <span v-if="testing">{{ testing }}</span>
  </div>
  <pre v-if="connectionLog" ref="logEl" class="runtime-test-log">{{ connectionLog }}</pre>
  <p v-if="error" class="form-error">{{ error }}</p>
  <div class="ds-modal-footer">
    <UiButton type="button" :disabled="busy" @click="emit('back')">{{ t("common.back") }}</UiButton>
    <UiButton type="button" variant="primary" :disabled="busy || !draft.name.trim() || !draft.apiUrl.trim() || !defaultModel || !parsed.ok || (!apiKey.trim() && !hasApiKey)" @click="submit">
      {{ busy ? t("common.saving") : t("common.save") }}
    </UiButton>
  </div>
</template>
