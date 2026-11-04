<script setup lang="ts">
import { ref } from "vue";
import { X } from "@lucide/vue";
import ImagePreviewLightbox from "../ui/ImagePreviewLightbox.vue";

defineProps<{
  images: Array<{ key: string; src?: string; name: string }>;
  removable?: boolean;
}>();

const emit = defineEmits<{
  remove: [key: string];
  open: [key: string];
}>();

const preview = ref<{ src: string; name: string } | null>(null);

function openPreview(image: { key: string; src?: string; name: string }) {
  emit("open", image.key);
  if (!image.src) return;
  preview.value = { src: image.src, name: image.name };
}

function closePreview() {
  preview.value = null;
}
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
  <ImagePreviewLightbox
    v-if="preview"
    :src="preview.src"
    :name="preview.name"
    @close="closePreview"
  />
</template>
