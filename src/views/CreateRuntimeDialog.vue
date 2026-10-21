<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { ArrowLeft, Plug, X } from "@lucide/vue";
import type { CustomRuntimeForm, Runtime } from "../domain/types";
import ProviderLogo from "../components/ProviderLogo.vue";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import * as backend from "../services/backend";
import {
  PROFILE_FAMILIES,
  envTextFrom,
  familyOf,
  formatCommandLine,
  gatewayDraftFor,
} from "./runtime-catalog";
import RuntimeGatewayForm from "./RuntimeGatewayForm.vue";
import RuntimeProfileForm from "./RuntimeProfileForm.vue";

const props = defineProps<{ runtime?: Runtime | null }>();
const emit = defineEmits<{ close: [] }>();

const store = useAppStore();
const step = ref<"kind" | "profile" | "gateway">(props.runtime ? "profile" : "kind");
const busy = ref(false);
const error = ref("");
const ready = ref(!props.runtime?.id?.startsWith("custom:"));
const editingId = ref<string | undefined>();
const locked = ref(false);
const legacy = ref(false);
const hasApiKey = ref(false);
const profile = ref({
  provider: "claude",
  name: "",
  commandLine: "claude",
  description: "",
  maxConcurrent: 2,
});
const gateway = ref(gatewayDraftFor("claude"));

const title = computed(() => (editingId.value ? t("runtime.editTitle") : t("runtime.createTitle")));

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape" && !busy.value) emit("close");
}

onMounted(async () => {
  document.addEventListener("keydown", onKey);
  if (!props.runtime?.id?.startsWith("custom:")) return;
  try {
    applyLoaded(await backend.getCustomRuntime(props.runtime.id));
  } catch {
    profile.value = {
      provider: props.runtime.provider || "claude",
      name: props.runtime.name,
      commandLine: formatCommandLine(props.runtime.command, props.runtime.args),
      description: props.runtime.description ?? "",
      maxConcurrent: props.runtime.maxConcurrent,
    };
    editingId.value = props.runtime.id;
    legacy.value = props.runtime.provider === "custom";
    step.value = props.runtime.kind === "gateway" ? "gateway" : "profile";
  } finally {
    ready.value = true;
  }
});
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));

function applyLoaded(current: CustomRuntimeForm) {
  editingId.value = current.id;
  hasApiKey.value = current.hasApiKey;
  legacy.value = current.provider === "custom";
  locked.value = current.provider !== "custom";
  const commandLine = formatCommandLine(current.command, current.args);
  if (current.kind === "gateway") {
    gateway.value = {
      ...gatewayDraftFor(familyOf(current.provider).gateway ? familyOf(current.provider).id : "claude", {
        name: current.name,
        description: current.description,
        maxConcurrent: current.maxConcurrent,
      }),
      commandLine,
      apiUrl: current.apiUrl,
      apiKey: current.apiKey,
      modelsText: current.models.join("\n"),
      wireApi: current.wireApi === "chat" ? "chat" : "responses",
      keyStyle: current.keyStyle === "x-api-key" ? "x-api-key" : "bearer",
      envText: envTextFrom(current.env),
    };
    step.value = "gateway";
    return;
  }
  profile.value = {
    provider: current.provider,
    name: current.name,
    commandLine,
    description: current.description,
    maxConcurrent: current.maxConcurrent,
  };
  step.value = "profile";
}

function pickProfile(id: string) {
  const spec = familyOf(id);
  if (!spec.supported) return;
  profile.value = {
    provider: id,
    name: t(`runtime.family.${id}`),
    commandLine: spec.command,
    description: "",
    maxConcurrent: 2,
  };
  locked.value = false;
  legacy.value = false;
  error.value = "";
  step.value = "profile";
}

function openGateway() {
  gateway.value = gatewayDraftFor("claude", { name: t("runtime.kind.gateway") });
  locked.value = false;
  hasApiKey.value = false;
  error.value = "";
  step.value = "gateway";
}

function goBack() {
  if (editingId.value) {
    emit("close");
    return;
  }
  error.value = "";
  step.value = "kind";
}

async function saveProfile(payload: {
  provider: string;
  name: string;
  command: string;
  args: string[];
  description: string;
  maxConcurrent: number;
}) {
  await save({
    ...payload,
    kind: "profile",
    apiUrl: "",
    apiKey: "",
    models: [],
    env: {},
    wireApi: "responses",
    keyStyle: "bearer",
  });
}

async function saveGateway(payload: {
  provider: string;
  name: string;
  command: string;
  args: string[];
  description: string;
  apiUrl: string;
  apiKey: string;
  models: string[];
  wireApi: "responses" | "chat";
  keyStyle: "bearer" | "x-api-key";
  env: Record<string, string>;
  maxConcurrent: number;
}) {
  await save({ ...payload, kind: "gateway" });
}

async function save(payload: {
  kind: "profile" | "gateway";
  provider: string;
  name: string;
  command: string;
  args: string[];
  description: string;
  apiUrl: string;
  apiKey: string;
  models: string[];
  env: Record<string, string>;
  wireApi: string;
  keyStyle: string;
  maxConcurrent: number;
}) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await store.saveCustomRuntime({
      id: editingId.value,
      name: payload.name,
      command: payload.command,
      args: payload.args,
      maxConcurrent: payload.maxConcurrent,
      provider: payload.provider,
      protocol: familyOf(payload.provider).protocol,
      apiUrl: payload.apiUrl,
      apiKey: payload.apiKey,
      models: payload.models,
      env: payload.env,
      kind: payload.kind,
      description: payload.description,
      wireApi: payload.wireApi,
      keyStyle: payload.keyStyle,
    });
    emit("close");
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-backdrop" @click.self="!busy && emit('close')">
    <form class="modal domain-modal runtime-create-dialog" @submit.prevent>
      <header class="modal-heading">
        <div class="skill-create-title">
          <UiButton v-if="step !== 'kind' && !editingId" class="icon-button" type="button" :disabled="busy" @click="goBack">
            <ArrowLeft :size="16" />
          </UiButton>
          <h2>{{ title }}</h2>
        </div>
        <UiButton class="icon-button" type="button" :disabled="busy" @click="emit('close')"><X :size="16" /></UiButton>
      </header>

      <template v-if="step === 'kind'">
        <section class="runtime-kind-block">
          <div>
            <h3>{{ t("runtime.kind.profile") }}</h3>
            <p class="skill-create-hint">{{ t("runtime.kind.profileHint") }}</p>
          </div>
          <div class="runtime-family-grid">
            <button
              v-for="item in PROFILE_FAMILIES"
              :key="item.id"
              type="button"
              class="runtime-family-card"
              :disabled="!item.supported"
              @click="pickProfile(item.id)"
            >
              <ProviderLogo :provider="item.id" :size="18" />
              <span>{{ t(`runtime.family.${item.id}`) }}</span>
            </button>
          </div>
        </section>
        <section class="runtime-kind-block">
          <div>
            <h3>{{ t("runtime.kind.gateway") }}</h3>
            <p class="skill-create-hint">{{ t("runtime.kind.gatewayHint") }}</p>
          </div>
          <button type="button" class="runtime-family-card runtime-gateway-entry" @click="openGateway">
            <Plug :size="18" />
            <span>{{ t("runtime.kind.gateway") }}</span>
          </button>
        </section>
      </template>

      <RuntimeProfileForm
        v-else-if="step === 'profile' && ready"
        :provider="profile.provider"
        :name="profile.name"
        :command-line="profile.commandLine"
        :description="profile.description"
        :max-concurrent="profile.maxConcurrent"
        :locked="locked && !legacy"
        :legacy="legacy"
        :busy="busy"
        :error="error"
        @back="goBack"
        @save="saveProfile"
      />
      <RuntimeGatewayForm
        v-else-if="step === 'gateway' && ready"
        :id="editingId"
        :provider="gateway.provider"
        :name="gateway.name"
        :command-line="gateway.commandLine"
        :description="gateway.description"
        :api-url="gateway.apiUrl"
        :initial-api-key="gateway.apiKey"
        :has-api-key="hasApiKey"
        :models-text="gateway.modelsText"
        :wire-api="gateway.wireApi"
        :key-style="gateway.keyStyle"
        :env-text="gateway.envText"
        :max-concurrent="gateway.maxConcurrent"
        :locked="locked"
        :legacy="legacy"
        :busy="busy"
        :error="error"
        @back="goBack"
        @save="saveGateway"
      />
    </form>
  </div>
</template>
