<script setup lang="ts">
import { computed, ref } from "vue";
import type { GitRepositoryStatus } from "../../domain/types";
import UiButton from "../ui/UiButton.vue";
import UiTextarea from "../ui/UiTextarea.vue";
import {
  resetCommitMessageCursor,
  scmBusyTitle,
  stepCommitMessage,
  type ScmPrimaryAction,
} from "../workbench/scm-commit-action";

const message = defineModel<string>({ required: true });
const props = defineProps<{
  workspaceId: string;
  repository: GitRepositoryStatus;
  action: ScmPrimaryAction;
  busy: string;
  canCommit: boolean;
  canPrimary: boolean;
}>();
const emit = defineEmits<{
  submit: [];
  menu: [event: MouseEvent];
  generate: [];
  settings: [];
}>();

const field = ref<{ el: HTMLTextAreaElement | null } | null>(null);
const repoBusy = computed(() => props.busy.endsWith(`:${props.workspaceId}`));

function placeholder() {
  return `消息 (Ctrl+Enter 在 "${props.repository.branch || "HEAD"}" 上提交)`;
}

function primaryLabel() {
  if (props.action === "publish") return "发布分支";
  if (props.action === "sync") return "同步更改";
  return "提交";
}

function onKeydown(event: KeyboardEvent) {
  if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
    event.preventDefault();
    emit("submit");
    return;
  }
  if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
  const input = field.value?.el;
  if (!input || event.shiftKey || event.altKey || event.metaKey || event.ctrlKey) return;
  const atStart = input.selectionStart === 0 && input.selectionEnd === 0;
  const atEnd = input.selectionStart === input.value.length && input.selectionEnd === input.value.length;
  if (event.key === "ArrowUp" && !atStart) return;
  if (event.key === "ArrowDown" && !atEnd) return;
  const next = stepCommitMessage(props.workspaceId, event.key === "ArrowUp" ? "older" : "newer");
  if (next == null) return;
  event.preventDefault();
  message.value = next;
}

function onInput() {
  resetCommitMessageCursor(props.workspaceId);
  const input = field.value?.el;
  if (!input) return;
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 120)}px`;
}
</script>

<template>
  <div class="scm-commit-box">
    <div class="scm-commit-field">
      <UiTextarea
        ref="field"
        v-model="message"
        :placeholder="placeholder()"
        spellcheck="false"
        @keydown="onKeydown"
        @input="onInput"
      />
      <UiButton
        variant="ghost"
        size="icon"
        :tip="scmBusyTitle(busy, `ai:${workspaceId}`, 'AI 生成提交信息')"
        :disabled="Boolean(busy)"
        @click="emit('generate')"
      >
        <Codicon v-if="busy === `ai:${workspaceId}`" name="loading" class="codicon-modifier-spin" />
        <Codicon v-else name="sparkle" />
      </UiButton>
      <UiButton
        variant="ghost"
        size="icon"
        tip="配置提交信息生成规则"
        @click="emit('settings')"
      >
        <Codicon name="gear" />
      </UiButton>
    </div>
    <div class="scm-commit-split">
      <UiButton
        class="scm-commit-go"
        :tip="scmBusyTitle(busy, [`commit:${workspaceId}`, `push:${workspaceId}`, `sync:${workspaceId}`], primaryLabel())"
        :disabled="repoBusy || !canPrimary"
        @click="emit('submit')"
      >
        <Codicon v-if="busy.startsWith(`commit:${workspaceId}`) || busy === `push:${workspaceId}` || busy === `sync:${workspaceId}`" name="loading" class="codicon-modifier-spin" />
        <Codicon v-else name="check" />
        {{ primaryLabel() }}
        <template v-if="action === 'sync'">
          <span v-if="repository.ahead > 0" class="scm-sync-count">{{ repository.ahead }}↑</span>
          <span v-if="repository.behind > 0" class="scm-sync-count">{{ repository.behind }}↓</span>
        </template>
      </UiButton>
      <UiButton
        v-if="action === 'commit'"
        class="scm-commit-more"
        :tip="scmBusyTitle(busy, `commit:${workspaceId}`, '更多提交操作')"
        :disabled="repoBusy || !canCommit"
        @click="emit('menu', $event)"
      >
        <Codicon name="chevron-down" />
      </UiButton>
    </div>
  </div>
</template>
