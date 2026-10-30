<script setup lang="ts">
import { computed, ref } from "vue";
import { ImagePlus, Sparkles } from "@lucide/vue";
import AgentAvatar from "./AgentAvatar.vue";

const props = withDefaults(defineProps<{
  modelValue?: string;
  name?: string;
  /** 锁定后只展示头像，不可选择/生成（如内置 Miku） */
  locked?: boolean;
}>(), {
  modelValue: "",
  name: "",
  locked: false,
});
const emit = defineEmits<{
  "update:modelValue": [value: string];
  error: [message: string];
}>();
const fileInput = ref<HTMLInputElement | null>(null);
const preview = computed(() => props.modelValue || "🤖");

// DiceBear 全部风格很大，点生成时才加载，不进入口 chunk
async function generate() {
  if (props.locked) return;
  const base = props.name.trim() || "agent";
  const { generateAgentAvatar } = await import("../utils/generate-agent-avatar");
  emit("update:modelValue", generateAgentAvatar(`${base}-${crypto.randomUUID().slice(0, 8)}`));
}

function choose() {
  if (props.locked) return;
  fileInput.value?.click();
}

function loadFile(event: Event) {
  if (props.locked) return;
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  if (!file.type.startsWith("image/")) {
    emit("error", "请选择图片文件");
    input.value = "";
    return;
  }
  if (file.size > 2 * 1024 * 1024) {
    emit("error", "头像图片不能超过 2 MB");
    input.value = "";
    return;
  }
  const reader = new FileReader();
  reader.onload = () => emit("update:modelValue", String(reader.result));
  reader.onerror = () => emit("error", "读取头像失败");
  reader.readAsDataURL(file);
  input.value = "";
}
</script>

<template>
  <div class="avatar-picker" :class="{ 'is-locked': locked }">
    <AgentAvatar :value="preview" :name="name" :size="64" />
    <div v-if="!locked">
      <UiButton size="small" @click="choose"><ImagePlus :size="14" />选择图片</UiButton>
      <UiButton size="small" @click="generate"><Sparkles :size="14" />生成头像</UiButton>
    </div>
    <input
      v-if="!locked"
      ref="fileInput"
      class="avatar-file-input"
      type="file"
      tabindex="-1"
      aria-hidden="true"
      accept="image/png,image/jpeg,image/webp,image/gif,image/svg+xml"
      @change="loadFile"
    />
  </div>
</template>
