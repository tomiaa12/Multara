<script setup lang="ts">
import { X } from "@lucide/vue";

defineProps<{
  images: Array<{ key: string; src?: string; name: string }>;
  removable?: boolean;
}>();

const emit = defineEmits<{
  remove: [key: string];
  open: [key: string];
}>();
</script>

<template>
  <div v-if="images.length" class="composer-paste-images vs-scroll">
    <component
      :is="removable ? 'div' : 'button'"
      v-for="image in images"
      :key="image.key"
      class="composer-paste-image"
      :type="removable ? undefined : 'button'"
      :title="image.name"
      @click="removable ? undefined : emit('open', image.key)"
    >
      <img v-if="image.src" :src="image.src" :alt="image.name" />
      <UiButton
        v-if="removable"
        type="button"
        variant="ghost"
        size="icon"
        title="移除图片"
        @click.stop="emit('remove', image.key)"
      >
        <X :size="12" />
      </UiButton>
    </component>
  </div>
</template>
