<script setup lang="ts">
import { computed, ref, watch } from "vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import { fileNameOf } from "../workbench/workspace-mode";
import { gitStatusClass } from "../workbench/tree-git-status";
import GitDiffEditor from "./GitDiffEditor.vue";

export type CommitDiffFile = {
  path: string;
  status: string;
  original: string;
  modified: string;
  language: string;
  binary: boolean;
};

const props = defineProps<{
  diffId: string;
  files: CommitDiffFile[];
  theme: string;
}>();

const activePath = ref(props.files[0]?.path ?? "");

watch(
  () => props.files.map((file) => file.path).join("\0"),
  () => {
    if (!props.files.some((file) => file.path === activePath.value)) {
      activePath.value = props.files[0]?.path ?? "";
    }
  },
);

const active = computed(
  () => props.files.find((file) => file.path === activePath.value) ?? props.files[0] ?? null,
);

function letter(status: string) {
  return status === "?" ? "U" : status || "M";
}
</script>

<template>
  <div class="commit-multi-diff">
    <div class="commit-multi-diff-files vs-scroll">
      <button
        v-for="file in files"
        :key="file.path"
        type="button"
        class="scm-file"
        :class="[gitStatusClass(letter(file.status)), { 'is-selected': file.path === active?.path }]"
        @click="activePath = file.path"
      >
        <FileTypeIcon :name="fileNameOf(file.path)" />
        <span class="scm-file-name">{{ fileNameOf(file.path) }}</span>
        <span class="scm-file-path">{{ file.path }}</span>
        <b>{{ letter(file.status) }}</b>
      </button>
    </div>
    <GitDiffEditor
      v-if="active"
      :key="`${diffId}:${active.path}`"
      :diff-id="`${diffId}:${active.path}`"
      :original="active.original"
      :modified="active.modified"
      :language="active.language"
      :file-path="active.path"
      :theme="theme"
      :binary="active.binary"
    />
    <p v-else class="git-diff-binary">没有可比较的文件。</p>
  </div>
</template>
