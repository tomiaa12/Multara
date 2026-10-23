<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { Check, Plus, Settings2, Tag, X } from "@lucide/vue";
import { t } from "../../i18n";

export interface TagOption {
  id: string;
  name: string;
  color: string;
}

const props = defineProps<{
  tags: TagOption[];
  modelValue: string[];
  placeholder?: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [ids: string[]];
  manage: [];
  create: [name: string];
}>();

const POPOVER_GAP = 4;
const VIEWPORT_MARGIN = 12;
/** 与 UiSelect 一致：下方空间不足时改向上展开 */
const FLIP_THRESHOLD = 320;

const open = ref(false);
const query = ref("");
const root = ref<HTMLElement | null>(null);
const popover = ref<HTMLElement | null>(null);
const placement = ref<"bottom" | "top">("bottom");
const popoverStyle = ref<Record<string, string>>({});

const selected = computed(() =>
  props.modelValue
    .map((id) => props.tags.find((tag) => tag.id === id))
    .filter((tag): tag is TagOption => Boolean(tag)),
);

const filtered = computed(() => {
  const text = query.value.trim().toLowerCase();
  if (!text) return props.tags;
  return props.tags.filter((tag) => tag.name.toLowerCase().includes(text));
});

const canCreate = computed(() => {
  const text = query.value.trim().toLowerCase();
  return Boolean(text) && !props.tags.some((tag) => tag.name.toLowerCase() === text);
});

function toggle(id: string) {
  const next = props.modelValue.includes(id)
    ? props.modelValue.filter((item) => item !== id)
    : [...props.modelValue, id];
  emit("update:modelValue", next);
}

function remove(id: string, event: MouseEvent) {
  event.stopPropagation();
  emit("update:modelValue", props.modelValue.filter((item) => item !== id));
}

function createTag() {
  const name = query.value.trim();
  if (!name || !canCreate.value) return;
  emit("create", name);
  query.value = "";
}

function updatePosition() {
  const trigger = root.value;
  if (!trigger) return;
  const rect = trigger.getBoundingClientRect();
  const below = window.innerHeight - rect.bottom;
  placement.value = below < FLIP_THRESHOLD && rect.top > below ? "top" : "bottom";
  const width = popover.value?.offsetWidth ?? Math.max(rect.width, 200);
  const left = Math.max(VIEWPORT_MARGIN, Math.min(rect.left, window.innerWidth - width - VIEWPORT_MARGIN));
  popoverStyle.value = {
    left: `${left}px`,
    minWidth: `${Math.max(rect.width, 200)}px`,
    ...(placement.value === "top"
      ? { bottom: `${window.innerHeight - rect.top + POPOVER_GAP}px`, top: "auto" }
      : { top: `${rect.bottom + POPOVER_GAP}px`, bottom: "auto" }),
  };
}

function onPointerDown(event: PointerEvent) {
  const target = event.target instanceof Node ? event.target : null;
  if (root.value?.contains(target) || popover.value?.contains(target)) return;
  open.value = false;
}

function bindPopover() {
  document.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("resize", updatePosition);
  window.addEventListener("scroll", updatePosition, true);
}

function unbindPopover() {
  document.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("resize", updatePosition);
  window.removeEventListener("scroll", updatePosition, true);
}

watch(open, (value) => {
  if (!value) {
    query.value = "";
    unbindPopover();
    return;
  }
  updatePosition();
  bindPopover();
  nextTick(updatePosition);
});

onBeforeUnmount(unbindPopover);
</script>

<template>
  <div ref="root" class="tag-anchor" @click.stop>
    <div class="tag-trigger" role="button" tabindex="0" @click="open = !open" @keydown.enter.prevent="open = !open">
      <template v-if="selected.length">
        <span v-for="tag in selected" :key="tag.id" class="tag-chip">
          <i class="tag-dot" :style="{ background: tag.color }" />
          <span>{{ tag.name }}</span>
          <button type="button" class="tag-chip-remove" :aria-label="t('tags.remove', { name: tag.name })" @click="remove(tag.id, $event)">
            <X :size="12" />
          </button>
        </span>
      </template>
      <template v-else>
        <Tag :size="14" />
        <span>{{ placeholder || t("tasks.selectTags") }}</span>
      </template>
    </div>
    <Teleport to="body">
    <div v-if="open" ref="popover" class="tag-pop is-floating" :data-placement="placement" :style="popoverStyle">
      <label class="ds-search">
        <input
          v-model="query"
          class="ui-input"
          :placeholder="t('tasks.searchTags')"
          @keydown.enter.prevent="createTag"
        />
      </label>
      <div class="tag-pop-list">
        <button
          v-for="tag in filtered"
          :key="tag.id"
          type="button"
          class="tag-option"
          @click="toggle(tag.id)"
        >
          <i class="tag-dot" :style="{ background: tag.color }" />
          <span>{{ tag.name }}</span>
          <Check v-if="modelValue.includes(tag.id)" class="tag-check" :size="14" />
        </button>
        <button v-if="canCreate" type="button" class="tag-option" @click="createTag">
          <Plus :size="14" />
          <span>{{ t('tags.createNamed', { name: query.trim() }) }}</span>
        </button>
        <p v-else-if="!filtered.length" class="tag-empty">{{ t('tags.empty') }}</p>
      </div>
      <button type="button" class="tag-pop-foot" @click="open = false; emit('manage')">
        <Settings2 :size="14" />
        {{ t('dashboard.manageTags') }}
      </button>
    </div>
    </Teleport>
  </div>
</template>
