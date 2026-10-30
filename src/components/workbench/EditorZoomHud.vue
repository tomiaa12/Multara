<script setup lang="ts">
import { Minus, Plus } from "@lucide/vue";
import {
  canZoomIn,
  canZoomOut,
  editorZoomHudVisible,
  editorZoomLabel,
  resetEditorZoom,
  zoomEditorIn,
  zoomEditorOut,
} from "./monaco-zoom";
import { t } from "../../i18n";
</script>

<template>
  <div
    v-if="editorZoomHudVisible"
    class="editor-zoom-hud"
    role="status"
    @mousedown.stop
    @pointerdown.stop
    @click.stop
  >
    <UiTooltip :content="t('workbench.shortcut.zoomOut')"><button type="button" :disabled="!canZoomOut" @pointerdown.prevent.stop="zoomEditorOut">
      <Minus :size="13" />
    </button></UiTooltip>
    <strong>{{ editorZoomLabel }}</strong>
    <UiTooltip :content="t('workbench.shortcut.zoomIn')"><button type="button" :disabled="!canZoomIn" @pointerdown.prevent.stop="zoomEditorIn">
      <Plus :size="13" />
    </button></UiTooltip>
    <UiTooltip :content="t('workbench.editorZoomReset')"><button type="button" class="is-reset" @pointerdown.prevent.stop="resetEditorZoom">{{ t('workbench.editorZoomResetShort') }}</button></UiTooltip>
  </div>
</template>
