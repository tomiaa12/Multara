<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { nextPopupZIndex } from "./popup-z-index";

export type ContextMenuItem = {
  id?: string;
  label?: string;
  key?: string;
  disabled?: boolean;
  checked?: boolean;
  type?: "sep";
  icon?: string;
};

const props = defineProps<{
  x: number;
  y: number;
  items: ContextMenuItem[];
}>();

const emit = defineEmits<{
  select: [id: string];
  close: [];
}>();

const menuEl = ref<HTMLElement | null>(null);
const left = ref(props.x);
const top = ref(props.y);
const zIndex = ref(nextPopupZIndex());
const style = computed(() => ({
  left: `${left.value}px`,
  top: `${top.value}px`,
  zIndex: String(zIndex.value),
}));

function place() {
  const el = menuEl.value;
  if (!el) return;
  left.value = Math.min(props.x, window.innerWidth - el.offsetWidth - 8);
  top.value = Math.min(props.y, window.innerHeight - el.offsetHeight - 8);
}

function onDoc(event: MouseEvent) {
  if (!menuEl.value?.contains(event.target as Node)) emit("close");
}

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") emit("close");
}

onMounted(() => {
  nextTick(place);
  document.addEventListener("mousedown", onDoc);
  document.addEventListener("keydown", onKey);
});

onBeforeUnmount(() => {
  document.removeEventListener("mousedown", onDoc);
  document.removeEventListener("keydown", onKey);
});

watch(() => [props.x, props.y], () => nextTick(place));
</script>

<template>
  <!-- Teleport 到 body，避免被 UiPane overflow / 容器裁剪（如 SCM 分支 … 菜单） -->
  <Teleport to="body">
    <div ref="menuEl" class="context-menu" :style="style" role="menu">
      <template v-for="(item, index) in items" :key="index">
        <hr v-if="item.type === 'sep'" />
        <button
          v-else
          type="button"
          role="menuitem"
          :disabled="item.disabled"
          @click="item.id && emit('select', item.id)"
        >
          <span v-if="item.checked != null" class="context-menu-check" aria-hidden="true">{{ item.checked ? "✓" : "" }}</span>
          <Codicon v-if="item.icon" :name="item.icon" :size="16" />
          <span class="context-menu-label">{{ item.label }}</span>
          <kbd v-if="item.key">{{ item.key }}</kbd>
        </button>
      </template>
    </div>
  </Teleport>
</template>

<style>
.context-menu[role="menu"] > button[role="menuitem"] {
  justify-content: flex-start;
  text-align: left;
}
.context-menu[role="menu"] > button[role="menuitem"] .context-menu-label {
  display: block;
  flex: 1 1 auto;
  text-align: left;
}
.context-menu-check {
  flex: 0 0 auto;
  width: 14px;
  color: var(--text);
}
</style>
