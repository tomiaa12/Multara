<script setup lang="ts">
import { computed, getCurrentInstance, inject, nextTick, onMounted, ref, watch } from "vue";
import { readStorage, writeStorage } from "../../utils/safe-storage";

const props = withDefaults(defineProps<{
  title: string;
  expanded?: boolean;
  fill?: boolean;
  actionsVisible?: boolean;
  storageKey?: string;
  defaultExpanded?: boolean;
  /** Initial height in pixels, including the header. Dragging stores a new size. */
  defaultSize?: number;
  sizeKey?: string;
  /** Smallest body height while expanded. VS Code's vertical pane floor is 120. */
  minBody?: number;
}>(), {
  fill: false,
  actionsVisible: false,
  defaultExpanded: true,
  minBody: 22,
});

const emit = defineEmits<{
  "update:expanded": [value: boolean];
  contextmenu: [event: MouseEvent];
}>();

const managed = ref(readExpanded());
// 未传入的布尔 prop 会被 Vue 收成 false，不能用 !== undefined 判断是否受控。
const controlled = computed(() => {
  const passed = getCurrentInstance()?.vnode.props ?? {};
  return Object.prototype.hasOwnProperty.call(passed, "expanded")
    || Object.prototype.hasOwnProperty.call(passed, "onUpdate:expanded");
});
const expanded = computed(() => (controlled.value ? Boolean(props.expanded) : managed.value));

function readExpanded() {
  if (props.storageKey) {
    const stored = readStorage(props.storageKey);
    if (stored === "1") return false;
    if (stored === "0") return true;
  }
  return props.defaultExpanded;
}

function setExpanded(value: boolean) {
  if (!controlled.value) managed.value = value;
  if (props.storageKey) writeStorage(props.storageKey, value ? "0" : "1");
  emit("update:expanded", value);
}

function toggle() {
  setExpanded(!expanded.value);
}

defineExpose({ setExpanded });

const showBody = ref(expanded.value);
let hideTimer = 0;
const paneView = inject<{ relayout: (animate?: boolean) => void } | null>("uiPaneView", null);

watch(expanded, (value) => {
  window.clearTimeout(hideTimer);
  if (value) showBody.value = true;
  else hideTimer = window.setTimeout(() => {
    if (!expanded.value) showBody.value = false;
  }, 200);
  nextTick(() => paneView?.relayout(true));
});

onMounted(() => {
  nextTick(() => paneView?.relayout(false));
});

function onKeydown(event: KeyboardEvent) {
  if (event.target !== event.currentTarget) return;
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    toggle();
  } else if (event.key === "ArrowLeft") {
    setExpanded(false);
  } else if (event.key === "ArrowRight") {
    setExpanded(true);
  }
}
</script>

<template>
  <section
    class="ui-pane"
    :class="{ 'is-collapsed': !expanded, 'is-fill': fill }"
    :data-fill="fill ? 'true' : 'false'"
    :data-expanded="expanded ? 'true' : 'false'"
    :data-size-key="sizeKey"
    :data-default-size="defaultSize"
    :data-min-body="minBody"
  >
    <div class="ui-pane-sash" role="separator" aria-orientation="horizontal" aria-label="调整高度" />
    <div
      class="ui-pane-header"
      :class="{ 'is-actions-visible': actionsVisible, 'is-expanded': expanded }"
      role="button"
      tabindex="0"
      :aria-expanded="expanded"
      :aria-label="title"
      @click="toggle"
      @keydown="onKeydown"
      @contextmenu.prevent.stop="emit('contextmenu', $event)"
    >
      <Codicon name="chevron-right" class="ui-pane-twistie" />
      <span class="ui-pane-title">{{ title }}</span>
      <slot name="title-extra" />
      <div class="ui-pane-actions" @click.stop @keydown.stop>
        <slot name="actions" />
      </div>
    </div>
    <div v-show="showBody" class="ui-pane-body vs-scroll">
      <slot />
    </div>
  </section>
</template>
