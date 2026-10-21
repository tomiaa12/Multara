<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import ProviderLogo from "../components/ProviderLogo.vue";
import { t } from "../i18n";
import * as backend from "../services/backend";
import {
  PROFILE_FAMILIES,
  familyOf,
  parseCommandLine,
} from "./runtime-catalog";

const props = defineProps<{
  provider: string;
  name: string;
  commandLine: string;
  description: string;
  maxConcurrent: number;
  locked: boolean;
  legacy: boolean;
  busy: boolean;
  error: string;
}>();

const emit = defineEmits<{
  back: [];
  save: [payload: { provider: string; name: string; command: string; args: string[]; description: string; maxConcurrent: number }];
}>();

const provider = ref(props.provider);
const name = ref(props.name);
const commandLine = ref(props.commandLine || familyOf(props.provider).command);
const description = ref(props.description);
const maxConcurrent = ref(props.maxConcurrent || 2);
const advanced = ref(false);
const missing = ref("");
let probeTimer = 0;

const parsed = computed(() => parseCommandLine(commandLine.value));
const spec = computed(() => familyOf(provider.value));
const choices = PROFILE_FAMILIES.filter((item) => item.supported);

watch(commandLine, () => {
  window.clearTimeout(probeTimer);
  missing.value = "";
  if (!parsed.value.ok) return;
  const commandName = parsed.value.commandName;
  const currentProvider = provider.value;
  probeTimer = window.setTimeout(async () => {
    const path = await backend.probeRuntimeCommand(commandName, currentProvider);
    const current = parseCommandLine(commandLine.value);
    if (!path && current.ok && current.commandName === commandName) {
      missing.value = commandName;
    }
  }, 400);
});

onBeforeUnmount(() => window.clearTimeout(probeTimer));

function pickProvider(id: string) {
  if (props.locked) return;
  const next = familyOf(id);
  const previous = familyOf(provider.value);
  if (!commandLine.value.trim() || commandLine.value.trim() === previous.command) {
    commandLine.value = next.command;
  }
  provider.value = id;
}

function submit() {
  if (!parsed.value.ok || !name.value.trim()) return;
  emit("save", {
    provider: provider.value,
    name: name.value.trim(),
    command: parsed.value.commandName,
    args: parsed.value.fixedArgs,
    description: description.value.trim(),
    maxConcurrent: Number(maxConcurrent.value) || 2,
  });
}
</script>

<template>
  <p v-if="legacy" class="ds-alert">{{ t("runtime.legacyHint") }}</p>
  <div class="runtime-base-lock">
    <ProviderLogo :provider="spec.id" :size="18" />
    <div>
      <strong>{{ legacy && provider === "custom" ? t("runtime.legacy") : t(`runtime.family.${spec.id}`) }}</strong>
      <small>{{ t("runtime.baseLocked") }}</small>
    </div>
  </div>
  <label v-if="legacy">{{ t("runtime.field.base") }}
    <UiSelect
      :model-value="provider"
      :options="choices.map((item) => ({ value: item.id, label: t(`runtime.family.${item.id}`) }))"
      @change="pickProvider(String($event))"
    />
  </label>
  <label>{{ t("runtime.field.name") }}<UiInput v-model="name" required /></label>
  <label>{{ t("runtime.field.commandLine") }}
    <UiInput v-model="commandLine" required :placeholder="spec.command" spellcheck="false" />
  </label>
  <p v-if="parsed.ok" class="runtime-command-preview">
    {{ t("runtime.commandPreview.executable") }} <code>{{ parsed.commandName }}</code>
    <template v-if="parsed.fixedArgs.length">
      · {{ t("runtime.commandPreview.args") }} <code>{{ parsed.fixedArgs.join(" ") }}</code>
    </template>
  </p>
  <p v-else-if="commandLine.trim()" class="form-error">{{ t(`runtime.commandError.${parsed.error}`) }}</p>
  <p v-if="missing" class="ds-alert">{{ t("runtime.commandMissing", { command: missing }) }}</p>
  <label>{{ t("runtime.field.description") }}<UiTextarea v-model="description" rows="2" /></label>
  <details class="runtime-advanced" :open="advanced" @toggle="advanced = ($event.target as HTMLDetailsElement).open">
    <summary>{{ t("runtime.advanced") }}</summary>
    <label>{{ t("runtime.field.concurrent") }}
      <UiSelect v-model="maxConcurrent" :options="Array.from({ length: 6 }, (_, index) => ({ value: index + 1, label: String(index + 1) }))" />
    </label>
  </details>
  <p v-if="error" class="form-error">{{ error }}</p>
  <div class="ds-modal-footer">
    <UiButton type="button" :disabled="busy" @click="emit('back')">{{ t("common.back") }}</UiButton>
    <UiButton type="button" variant="primary" :disabled="busy || !name.trim() || !parsed.ok" @click="submit">
      {{ busy ? t("common.saving") : t("common.save") }}
    </UiButton>
  </div>
</template>
