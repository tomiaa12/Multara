<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { X } from "@lucide/vue";
import { nextPopupZIndex } from "../ui/popup-z-index";

const props = defineProps<{
  images: Array<{ key: string; src?: string; name: string }>;
  removable?: boolean;
}>();

const emit = defineEmits<{
  remove: [key: string];
  open: [key: string];
}>();

const preview = ref<{ src: string; name: string } | null>(null);
const overlayZ = ref(0);

function openPreview(image: { key: string; src?: string; name: string }) {
  emit("open", image.key);
  if (!image.src) return;
  overlayZ.value = nextPopupZIndex();
  preview.value = { src: image.src, name: image.name };
}

function closePreview() {
  preview.value = null;
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && preview.value) {
    event.preventDefault();
    event.stopPropagation();
    closePreview();
  }
}

watch(preview, (value, _, onCleanup) => {
  if (!value) return;
  window.addEventListener("keydown", onKeydown, true);
  onCleanup(() => window.removeEventListener("keydown", onKeydown, true));
});

onBeforeUnmount(closePreview);
</script>

<template>
  <div v-if="images.length" class="composer-paste-images vs-scroll">
    <template v-for="image in images" :key="image.key">
      <UiTooltip :content="image.name">
        <div class="composer-paste-image">
          <button
            type="button"
            class="composer-paste-image-thumb"
            @click="openPreview(image)"
          >
            <img v-if="image.src" :src="image.src" :alt="image.name" />
          </button>
          <UiButton
            v-if="removable"
            type="button"
            variant="ghost"
            size="icon"
            tip="移除图片"
            @click.stop="emit('remove', image.key)"
          >
            <X :size="10" />
          </UiButton>
        </div>
      </UiTooltip>
    </template>
  </div>
  <Teleport to="body">
    <div
      v-if="preview"
      class="composer-image-lightbox"
      role="dialog"
      aria-modal="true"
      :aria-label="preview.name"
      :style="{ zIndex: overlayZ }"
      @click="closePreview"
    >
      <img :src="preview.src" :alt="preview.name" @click.stop />
    </div>
  </Teleport>
</template>
