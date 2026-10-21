<script setup lang="ts">
defineProps<{
  kind: "image" | "audio" | "video";
  src: string;
  name: string;
}>();
</script>

<template>
  <div class="media-preview">
    <img v-if="kind === 'image'" :src="src" :alt="name" />
    <audio v-else-if="kind === 'audio'" :src="src" :aria-label="name" controls />
    <video v-else :src="src" :aria-label="name" controls />
  </div>
</template>
