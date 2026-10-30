<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ChevronLeft, ChevronRight, RotateCcw } from "@lucide/vue";
import WorkspaceWorkbench from "../components/WorkspaceWorkbench.vue";
import { previewMessages } from "../components/chat/preview-messages";
import {
  protocolReplaySnapshot,
  protocolReplayStepCount,
} from "../components/chat/protocol-replay";
import {
  structureReplaySnapshot,
  structureReplayStepCount,
} from "../components/chat/structure-replay";
import { PREVIEW_WORKSPACE_ID } from "../components/workbench/preview-workspace";
import type { EditorSelectionContext } from "../domain/types";

type PreviewMode = "structure" | "protocol" | "gallery" | "composer";

const mode = ref<PreviewMode>("structure");
const step = ref(0);
const isReplay = computed(() => mode.value === "structure" || mode.value === "protocol");
const snapshot = computed(() =>
  mode.value === "structure"
    ? structureReplaySnapshot(step.value)
    : protocolReplaySnapshot(step.value),
);
const stepCount = computed(() =>
  mode.value === "structure" ? structureReplayStepCount : protocolReplayStepCount,
);
const workbench = ref<{
  insertChip: (item: EditorSelectionContext) => void;
  insertText: (value: string) => void;
  addPastedImage: () => void;
  addAttachment: (name: string, mimeType: string) => void;
  attachWorkspaceFile: (file: { workspaceId: string; path: string; isDirectory?: boolean }) => void;
  clearComposer: () => void;
} | null>(null);

function start() {
  step.value = 1;
}

function previous() {
  step.value = Math.max(0, step.value - 1);
}

function next() {
  step.value = Math.min(stepCount.value, step.value + 1);
}

function reset() {
  step.value = 0;
}

function chip(partial: Omit<EditorSelectionContext, "id" | "workspaceId">): EditorSelectionContext {
  return {
    id: crypto.randomUUID(),
    workspaceId: PREVIEW_WORKSPACE_ID,
    ...partial,
  };
}

function addPlainText() {
  workbench.value?.insertText("请解释这段代码的作用。");
}

function addMultilineText() {
  workbench.value?.insertText("先看入口文件，再核对 webhook 监听。\n如果路径不对，告诉我哪里需要改。");
}

function addLineChip() {
  workbench.value?.insertChip(chip({
    path: "src/App.vue",
    startLine: 12,
    endLine: 12,
    text: "import ChatPanel from \"../components/ChatPanel.vue\";",
  }));
}

function addRangeChip() {
  workbench.value?.insertChip(chip({
    path: "src/composer.ts",
    startLine: 24,
    endLine: 31,
    text: "export function formatAgentPrompt(nodes) {\n  return nodes.map((node) => node.value).join(\"\");\n}",
  }));
}

function addFileChip() {
  workbench.value?.attachWorkspaceFile({
    workspaceId: PREVIEW_WORKSPACE_ID,
    path: "package.json",
  });
}

function addFolderChip() {
  workbench.value?.attachWorkspaceFile({
    workspaceId: PREVIEW_WORKSPACE_ID,
    path: "src",
    isDirectory: true,
  });
}

function addManyChips() {
  addLineChip();
  addRangeChip();
  addFolderChip();
}

function addPastedImage() {
  workbench.value?.addPastedImage();
}

function addAttachment() {
  workbench.value?.addAttachment("requirements.pdf", "application/pdf");
}

function addMixed() {
  addPlainText();
  addRangeChip();
  addPastedImage();
  addAttachment();
}

function clearComposer() {
  workbench.value?.clearComposer();
}

watch(mode, () => {
  reset();
});
</script>

<template>
  <div class="chat-preview-page">
    <header class="chat-preview-header">
      <div>
        <h1>对话样式预览</h1>
        <p>逐步回放发送、改标题和每一帧响应，方便对照调试。</p>
      </div>
      <div class="preview-debug-menu">
        <UiSelect
          v-model="mode"
          aria-label="选择预览场景"
          :options="[
            { value: 'structure', label: '项目结构探查（逐步调试）' },
            { value: 'protocol', label: '两轮原始数据回放' },
            { value: 'gallery', label: '完整消息样式图库' },
            { value: 'composer', label: '输入框类型测试' },
          ]"
        />
        <template v-if="isReplay">
          <UiButton v-if="step === 0" class="primary" @click="start">开始</UiButton>
          <UiButton v-else tip="上一步" :disabled="step === 0" @click="previous">
            <ChevronLeft :size="14" />上一步
          </UiButton>
          <UiButton tip="下一步" :disabled="step >= stepCount" @click="next">
            下一步<ChevronRight :size="14" />
          </UiButton>
          <UiButton class="icon-button" tip="重置" @click="reset">
            <RotateCcw :size="14" />
          </UiButton>
        </template>
      </div>
    </header>
    <div class="preview-debug-status">
      <template v-if="isReplay">
        <div>
          <strong>{{ snapshot.title }}</strong>
          <span>{{ snapshot.detail }}</span>
          <span v-if="snapshot.original">原始提问：{{ snapshot.original }}</span>
          <span v-if="snapshot.sessionTitle">Tab 标题：{{ snapshot.sessionTitle }}</span>
        </div>
        <span>{{ step }} / {{ stepCount }}</span>
      </template>
      <template v-else-if="mode === 'gallery'">
        <div>
          <strong>完整消息样式图库</strong>
          <span>集中查看 Markdown、附件、失败态和运行中工具调用。</span>
        </div>
      </template>
      <template v-else>
        <div>
          <strong>输入框类型测试</strong>
          <span>点按钮插入内容，或从左侧工作台文件树拖到输入框。</span>
        </div>
        <div class="preview-composer-actions">
          <UiButton size="small" @click="addPlainText">纯文本</UiButton>
          <UiButton size="small" @click="addMultilineText">多行文本</UiButton>
          <UiButton size="small" @click="addLineChip">单行引用</UiButton>
          <UiButton size="small" @click="addRangeChip">行范围引用</UiButton>
          <UiButton size="small" @click="addFileChip">整文件</UiButton>
          <UiButton size="small" @click="addFolderChip">文件夹</UiButton>
          <UiButton size="small" @click="addManyChips">多个引用</UiButton>
          <UiButton size="small" @click="addPastedImage">粘贴图片</UiButton>
          <UiButton size="small" @click="addAttachment">文件附件</UiButton>
          <UiButton size="small" @click="addMixed">混合内容</UiButton>
          <UiButton size="small" @click="clearComposer">清空</UiButton>
        </div>
      </template>
    </div>
    <pre v-if="isReplay && snapshot.sent" class="preview-debug-payload">{{ snapshot.sent }}</pre>
    <WorkspaceWorkbench
      preview
      :preview-interactive="mode === 'composer'"
      :preview-title="mode === 'structure' ? 'Multara' : '样式预览'"
      :preview-messages="mode === 'structure' || mode === 'protocol' ? snapshot.messages : mode === 'gallery' ? previewMessages : []"
      :preview-composer-text="isReplay ? snapshot.composer : ''"
      :preview-session-title="isReplay ? snapshot.sessionTitle : '样式预览'"
      :preview-busy="isReplay && snapshot.busy"
      ref="workbench"
    />
  </div>
</template>
