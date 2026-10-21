<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { ArrowLeft, ChevronRight, Download, FolderOpen, Plus, Sparkles, Store, X } from "@lucide/vue";
import type { CatalogItem, ResourceSource } from "../domain/types";
import CliSourceIcon from "../components/CliSourceIcon.vue";
import { cliSkillsDisplay } from "../domain/cli-catalog";
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
  if (method.value === "manual") return "手动创建 Skill";
  if (method.value === "generate") return "用 AI 生成 Skill";
  if (method.value === "local") return "从本地文件夹导入";
  if (method.value === "url") return "从 URL 安装";
  return "新建 Skill";
});

const methods: { key: Exclude<Method, "chooser">; icon: typeof Plus; title: string; desc: string }[] = [
  { key: "generate", icon: Sparkles, title: "用 AI 生成", desc: "写需求，让当前运行时起草一份 SKILL.md。" },
  { key: "manual", icon: Plus, title: "手动创建", desc: "写一份 SKILL.md，放到所选 Skills 目录。" },
  { key: "local", icon: FolderOpen, title: "本地文件夹", desc: "选择已有的 Skill 目录，目录里需要有 SKILL.md。" },
  { key: "url", icon: Download, title: "从 URL 安装", desc: "粘贴 GitHub 或 skills.sh 地址，用 git 浅克隆导入。" },
];

const ownerOptions = computed(() =>
  sources.value.map((item) => {
    let meta: string | undefined;
    if (item.scope === "workspace") meta = "工作区";
    else meta = cliSkillsDisplay(item.owner);
    return { value: item.owner, label: item.label, meta };
  }),
);

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
    error.value = caught instanceof Error ? caught.message : "创建失败";
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
    error.value = caught instanceof Error ? caught.message : "生成失败";
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
    error.value = caught instanceof Error ? caught.message : "导入失败";
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
    error.value = caught instanceof Error ? caught.message : "安装失败";
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
            <strong>浏览商店</strong>
            <small>在应用内搜索 Anthropic Skills 并安装。</small>
          </span>
          <ChevronRight :size="16" />
        </button>
      </div>

      <template v-else>
        <label>保存到
          <UiSelect v-model="owner" :options="ownerOptions">
            <template #option-leading="{ option }">
              <CliSourceIcon v-if="option" :owner="String(option.value)" :size="16" />
            </template>
          </UiSelect>
        </label>
      </template>

      <template v-if="method === 'manual'">
        <label>名称<UiInput v-model="form.name" required placeholder="例如：代码审查" /></label>
        <label>描述<UiTextarea v-model="form.description" rows="3" placeholder="这个 Skill 什么时候该用" /></label>
        <label>正文（可选）<UiTextarea v-model="form.body" rows="6" placeholder="步骤、检查清单或使用说明" /></label>
      </template>

      <template v-else-if="method === 'generate'">
        <label>需求
          <UiTextarea v-model="form.requirement" rows="8" required placeholder="例如：审查 TypeScript 变更时检查权限、输入校验和依赖漏洞。" />
        </label>
        <p>会调用一个已配置的智能体起草 SKILL.md，可能需要几十秒。</p>
      </template>

      <template v-else-if="method === 'local'">
        <p>选择一个包含 <code>SKILL.md</code> 的目录。整个文件夹都会复制到所选 Skills 目录。</p>
        <div class="skill-create-pick">
          <UiInput :model-value="folderPath" readonly placeholder="还没有选择文件夹" />
          <UiButton type="button" :disabled="busy" @click="pickFolder"><FolderOpen :size="14" /> 选择</UiButton>
        </div>
      </template>

      <template v-else-if="method === 'url'">
        <p>主流目录用 GitHub 托管；<code>skills.sh/{owner}/{repo}/{skill}</code> 会映射到对应仓库。</p>
        <label>地址
          <UiInput v-model="form.url" required placeholder="https://github.com/owner/repo/tree/main/pdf" />
        </label>
        <div class="skill-create-hint">
          <UiButton type="button" variant="ghost" size="small" @click="browsing = true">打开商店</UiButton>
        </div>
      </template>

      <p v-if="error" class="form-error">{{ error }}</p>
      <div v-if="method !== 'chooser'" class="ds-modal-footer">
        <UiButton type="button" :disabled="busy" @click="back">返回</UiButton>
        <UiButton
          type="submit"
          variant="primary"
          :disabled="busy || !canSubmit"
        >
          {{ busy ? "处理中…" : method === "generate" ? "生成" : method === "manual" ? "创建" : "导入" }}
        </UiButton>
      </div>
    </form>
  </div>
  <CatalogBrowse v-if="browsing" kind="skill" @close="browsing = false" @install="installCatalog" />
</template>
