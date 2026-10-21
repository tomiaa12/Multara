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
    <button type="button" title="缩小" :disabled="!canZoomOut" @pointerdown.prevent.stop="zoomEditorOut">
      <Minus :size="13" />
    </button>
    <strong>{{ editorZoomLabel }}</strong>
    <button type="button" title="放大" :disabled="!canZoomIn" @pointerdown.prevent.stop="zoomEditorIn">
      <Plus :size="13" />
    </button>
    <button type="button" class="is-reset" title="重置为 100%" @pointerdown.prevent.stop="resetEditorZoom">重置</button>
  </div>
</template>
