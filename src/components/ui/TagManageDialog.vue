<script setup lang="ts">
import { reactive } from "vue";
import { X } from "@lucide/vue";
import { TAG_COLORS, type RequirementCategory } from "../../views/dashboard/requirement-categories";
import { t } from "../../i18n";

defineProps<{
  tags: RequirementCategory[];
}>();

const emit = defineEmits<{
  close: [];
  create: [name: string];
  rename: [id: string, name: string];
  recolor: [id: string, color: string];
  remove: [id: string];
}>();

const name = reactive({ value: "" });
const drafts = reactive<Record<string, string>>({});

function submitCreate() {
  const next = name.value.trim();
  if (!next) return;
  emit("create", next);
  name.value = "";
}

function commit(id: string, fallback: string) {
  emit("rename", id, drafts[id] ?? fallback);
}
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')">
    <div class="modal">
      <div class="modal-heading">
        <h2>{{ t('dashboard.manageTags') }}</h2>
        <UiButton class="icon-button" size="icon" :title="t('common.close')" @click="emit('close')">
          <X :size="15" />
        </UiButton>
      </div>
      <form class="tag-manage-create" @submit.prevent="submitCreate">
        <span>{{ t('tags.createNew') }}</span>
        <UiInput v-model="name.value" :placeholder="t('tags.namePlaceholder')" />
        <UiButton type="submit" :disabled="!name.value.trim()">{{ t('tags.add') }}</UiButton>
      </form>
      <div v-if="tags.length" class="tag-pop-list">
        <div v-for="tag in tags" :key="tag.id" class="tag-manage-row">
          <i class="tag-dot" :style="{ background: tag.color }" />
          <UiInput
            :model-value="drafts[tag.id] ?? tag.name"
            @update:model-value="drafts[tag.id] = String($event ?? '')"
            @blur="commit(tag.id, tag.name)"
            @keydown.enter.prevent="commit(tag.id, tag.name)"
          />
          <UiButton size="small" variant="danger" @click="emit('remove', tag.id)">{{ t('common.delete') }}</UiButton>
          <div class="tag-color-row">
            <button
              v-for="color in TAG_COLORS"
              :key="color"
              type="button"
              class="tag-color-swatch"
              :class="{ 'is-on': tag.color === color }"
              :style="{ background: color }"
              :aria-label="color"
              @click="emit('recolor', tag.id, color)"
            />
          </div>
        </div>
      </div>
      <p v-else class="tag-empty">{{ t('tags.emptyHint') }}</p>
    </div>
  </div>
</template>
