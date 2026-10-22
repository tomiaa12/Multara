<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { ArrowLeft, ChevronRight, Download, FolderOpen, Plus, Sparkles, Store, X } from "@lucide/vue";
import type { CatalogItem, ResourceSource } from "../domain/types";
import CliSourceIcon from "../components/CliSourceIcon.vue";
import { cliSkillsDisplay } from "../domain/cli-catalog";
import { t } from "../i18n";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";
import CatalogBrowse from "./CatalogBrowse.vue";

type Method = "chooser" | "manual" | "generate" | "local" | "url";

const emit = defineEmits<{
  close: [];
  created: [id: string];
}>();

const store = useAppStore();
const method = ref<Method>("chooser");
const browsing = ref(false);
const busy = ref(false);
const error = ref("");
const folderPath = ref("");
const owner = ref("multara");
const sources = ref<ResourceSource[]>([]);
const form = reactive({
  name: "",
  description: "",
  body: "",
  url: "",
  requirement: "",
});

const title = computed(() => {
  if (method.value === "manual") return t("skills.create.manualTitle");
  if (method.value === "generate") return t("skills.create.generateTitle");
  if (method.value === "local") return t("skills.create.localTitle");
  if (method.value === "url") return t("skills.create.urlTitle");
  return t("skills.create");
});

const methods = computed(() => [
  { key: "generate" as const, icon: Sparkles, title: t("skills.create.method.generate"), desc: t("skills.create.method.generateDesc") },
  { key: "manual" as const, icon: Plus, title: t("skills.create.method.manual"), desc: t("skills.create.method.manualDesc") },
  { key: "local" as const, icon: FolderOpen, title: t("skills.create.method.local"), desc: t("skills.create.method.localDesc") },
  { key: "url" as const, icon: Download, title: t("skills.create.method.url"), desc: t("skills.create.method.urlDesc") },
]);

const ownerOptions = computed(() =>
  sources.value.map((item) => {
    const meta = item.scope === "workspace" ? t("common.workspaceScope") : cliSkillsDisplay(item.owner);
    return { value: item.owner, label: item.label, meta };
  }),
);

function submitLabel() {
  if (busy.value) return t("common.processing");
  if (method.value === "generate") return t("skills.create.submitGenerate");
  if (method.value === "manual") return t("common.create");
  return t("skills.create.submitImport");
}

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape" && !busy.value) emit("close");
}

onMounted(() => {
  document.addEventListener("keydown", onKey);
  void backend.listResourceSources("skill").then((rows) => {
    sources.value = rows;
    if (!rows.some((item) => item.owner === owner.value) && rows[0]) owner.value = rows[0].owner;
  });
});
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));

function choose(next: Exclude<Method, "chooser">) {
  error.value = "";
  method.value = next;
}

function back() {
  if (busy.value) return;
  error.value = "";
  method.value = "chooser";
}

async function pickFolder() {
  error.value = "";
  const path = await backend.selectFolder();
  if (path) folderPath.value = path;
}

async function submitManual() {
  if (!form.name.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const created = await store.createSkill(form.name, form.description, form.body, owner.value);
    emit("created", created.id);
  } catch (caught) {
    error.value = caught instanceof Error ? caught.message : t("skills.create.failCreate");
  } finally {
    busy.value = false;
  }
}

async function submitGenerate() {
  if (!form.requirement.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const created = await store.generateSkill(form.requirement, owner.value);
    emit("created", created.id);
  } catch (caught) {
    error.value = caught instanceof Error ? caught.message : t("skills.create.failGenerate");
  } finally {
    busy.value = false;
  }
}

async function submitFolder() {
  if (!folderPath.value || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const created = await store.importSkillFromFolder(folderPath.value, owner.value);
    emit("created", created.id);
  } catch (caught) {
    error.value = caught instanceof Error ? caught.message : t("skills.create.failImport");
  } finally {
    busy.value = false;
  }
}

async function submitUrl() {
  if (!form.url.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const created = await store.importSkillFromUrl(form.url, owner.value);
    emit("created", created.id);
  } catch (caught) {
    error.value = caught instanceof Error ? caught.message : t("skills.create.failInstall");
  } finally {
    busy.value = false;
  }
}

async function installCatalog(item: CatalogItem, nextOwner = owner.value) {
  browsing.value = false;
  form.url = item.install;
  owner.value = nextOwner;
  method.value = "url";
  await submitUrl();
}

function submit() {
  if (method.value === "manual") return void submitManual();
  if (method.value === "generate") return void submitGenerate();
  if (method.value === "local") return void submitFolder();
  if (method.value === "url") return void submitUrl();
}

const canSubmit = computed(() => {
  if (method.value === "manual") return Boolean(form.name.trim());
  if (method.value === "generate") return Boolean(form.requirement.trim());
  if (method.value === "local") return Boolean(folderPath.value);
  if (method.value === "url") return Boolean(form.url.trim());
  return false;
});
</script>

<template>
  <div class="modal-backdrop" @click.self="!busy && emit('close')">
    <form
      class="modal domain-modal skill-create-dialog"
      @submit.prevent="submit"
    >
      <header class="modal-heading">
        <div class="skill-create-title">
          <UiButton v-if="method !== 'chooser'" class="icon-button" :disabled="busy" @click="back">
            <ArrowLeft :size="16" />
          </UiButton>
          <h2>{{ title }}</h2>
        </div>
        <UiButton class="icon-button" :disabled="busy" @click="emit('close')"><X :size="16" /></UiButton>
      </header>

      <div v-if="method === 'chooser'" class="skill-create-methods">
        <button
          v-for="item in methods"
          :key="item.key"
          type="button"
          class="skill-create-method"
          @click="choose(item.key)"
        >
          <span class="skill-create-method-icon"><component :is="item.icon" :size="16" /></span>
          <span>
            <strong>{{ item.title }}</strong>
            <small>{{ item.desc }}</small>
          </span>
          <ChevronRight :size="16" />
        </button>
        <button type="button" class="skill-create-method" @click="browsing = true">
          <span class="skill-create-method-icon"><Store :size="16" /></span>
          <span>
            <strong>{{ t("skills.create.browseStore") }}</strong>
            <small>{{ t("skills.create.browseStoreDesc") }}</small>
          </span>
          <ChevronRight :size="16" />
        </button>
      </div>

      <template v-else>
        <label>{{ t("skills.create.saveTo") }}
          <UiSelect v-model="owner" :options="ownerOptions">
            <template #option-leading="{ option }">
              <CliSourceIcon v-if="option" :owner="String(option.value)" :size="16" />
            </template>
          </UiSelect>
        </label>
      </template>

      <template v-if="method === 'manual'">
        <label>{{ t("skills.create.field.name") }}<UiInput v-model="form.name" required :placeholder="t('skills.create.namePlaceholder')" /></label>
        <label>{{ t("skills.create.field.description") }}<UiTextarea v-model="form.description" rows="3" :placeholder="t('skills.create.descPlaceholder')" /></label>
        <label>{{ t("skills.create.field.body") }}<UiTextarea v-model="form.body" rows="6" :placeholder="t('skills.create.bodyPlaceholder')" /></label>
      </template>

      <template v-else-if="method === 'generate'">
        <label>{{ t("skills.create.field.requirement") }}
          <UiTextarea v-model="form.requirement" rows="8" required :placeholder="t('skills.create.requirementPlaceholder')" />
        </label>
        <p>{{ t("skills.create.generateHint") }}</p>
      </template>

      <template v-else-if="method === 'local'">
        <p>{{ t("skills.create.localHint") }}</p>
        <div class="skill-create-pick">
          <UiInput :model-value="folderPath" readonly :placeholder="t('skills.create.folderPlaceholder')" />
          <UiButton type="button" :disabled="busy" @click="pickFolder"><FolderOpen :size="14" /> {{ t("skills.create.pickFolder") }}</UiButton>
        </div>
      </template>

      <template v-else-if="method === 'url'">
        <p>{{ t("skills.create.urlHint") }}</p>
        <label>{{ t("skills.create.field.url") }}
          <UiInput v-model="form.url" required placeholder="https://github.com/owner/repo/tree/main/pdf" />
        </label>
        <div class="skill-create-hint">
          <UiButton type="button" variant="ghost" size="small" @click="browsing = true">{{ t("skills.create.openStore") }}</UiButton>
        </div>
      </template>

      <p v-if="error" class="form-error">{{ error }}</p>
      <div v-if="method !== 'chooser'" class="ds-modal-footer">
        <UiButton type="button" :disabled="busy" @click="back">{{ t("common.back") }}</UiButton>
        <UiButton
          type="submit"
          variant="primary"
          :disabled="busy || !canSubmit"
        >
          {{ submitLabel() }}
        </UiButton>
      </div>
    </form>
  </div>
  <CatalogBrowse v-if="browsing" kind="skill" @close="browsing = false" @install="installCatalog" />
</template>
