<script setup lang="ts">
import { t } from "../../i18n";
import UiTooltip from "../ui/UiTooltip.vue";
import type { MarkdownViewMode } from "./markdown-source";

const mode = defineModel<MarkdownViewMode>({ required: true });
withDefaults(defineProps<{ sourceLabel?: string }>(), { sourceLabel: "Markdown" });
</script>

<template>
  <div class="md-editor-switch" role="tablist" :aria-label="t('workbench.markdownView')">
    <UiTooltip :content="t('workbench.markdownPreviewTitle')">
      <button
        type="button"
        role="tab"
        :class="{ active: mode === 'preview' }"
        :aria-selected="mode === 'preview'"
        @click="mode = 'preview'"
      >
        Preview
      </button>
    </UiTooltip>
    <UiTooltip :content="t('workbench.markdownSourceTitle')">
      <button
        type="button"
        role="tab"
        :class="{ active: mode === 'source' }"
        :aria-selected="mode === 'source'"
        @click="mode = 'source'"
      >
        {{ sourceLabel }}
      </button>
    </UiTooltip>
  </div>
</template>
