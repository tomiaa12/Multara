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
    <button type="button" :title="t('workbench.shortcut.zoomOut')" :disabled="!canZoomOut" @pointerdown.prevent.stop="zoomEditorOut">
      <Minus :size="13" />
    </button>
    <strong>{{ editorZoomLabel }}</strong>
    <button type="button" :title="t('workbench.shortcut.zoomIn')" :disabled="!canZoomIn" @pointerdown.prevent.stop="zoomEditorIn">
      <Plus :size="13" />
    </button>
    <button type="button" class="is-reset" :title="t('workbench.editorZoomReset')" @pointerdown.prevent.stop="resetEditorZoom">{{ t('workbench.editorZoomResetShort') }}</button>
  </div>
</template>
