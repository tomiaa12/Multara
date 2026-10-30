<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import UiButton from "../ui/UiButton.vue";
import TerminalPeek from "./TerminalPeek.vue";
import {
  formatTerminalPorts,
  requestTerminalFocus,
  runningTerminals,
  type RunningTerminal,
} from "../../stores/running-terminals";
import { t } from "../../i18n";

const VIEWPORT_GAP_PX = 8;

const router = useRouter();
const open = ref(false);
const anchor = ref<DOMRect | null>(null);
const menu = ref<HTMLElement | null>(null);
const menuStyle = ref<{ left: string; top: string }>({ left: "0px", top: "0px" });
const peekId = ref("");

const serving = computed(() => runningTerminals.value.filter((item) => item.ports.length > 0));
const peeking = computed(() => serving.value.find((item) => item.id === peekId.value) ?? null);

function placeMenu() {
  const el = menu.value;
  const rect = anchor.value;
  if (!el || !rect) return;
  const left = Math.max(
    VIEWPORT_GAP_PX,
    Math.min(rect.right - el.offsetWidth, window.innerWidth - el.offsetWidth - VIEWPORT_GAP_PX),
  );
  const top = Math.max(VIEWPORT_GAP_PX, rect.top - el.offsetHeight - VIEWPORT_GAP_PX);
  menuStyle.value = { left: `${left}px`, top: `${top}px` };
}

async function toggle(event: MouseEvent) {
  const button = event.currentTarget;
  if (!(button instanceof HTMLElement)) return;
  open.value = !open.value;
  if (!open.value) return;
  anchor.value = button.getBoundingClientRect();
  await nextTick();
  placeMenu();
}

function onDocumentPointer(event: PointerEvent) {
  if (!open.value) return;
  const target = event.target;
  if (!(target instanceof Node)) return;
  if (menu.value?.contains(target)) return;
  if (target instanceof Element && target.closest(".terminal-activity-button")) return;
  open.value = false;
}

function onKey(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  if (event.target instanceof Element && event.target.closest(".xterm")) return;
  open.value = false;
  peekId.value = "";
}

function openPeekWorkbench() {
  if (!peeking.value) return;
  openWorkbench(peeking.value);
}

function preview(item: RunningTerminal) {
  peekId.value = item.id;
  open.value = false;
}

function openWorkbench(item: RunningTerminal) {
  open.value = false;
  peekId.value = "";
  requestTerminalFocus(item.id);
  void router.push(`/projects/${item.projectId}`);
}

watch(serving, (list) => {
  if (peekId.value && !list.some((item) => item.id === peekId.value)) peekId.value = "";
  if (!list.length) open.value = false;
});

watch(open, (value) => {
  if (value) document.addEventListener("pointerdown", onDocumentPointer, true);
  else document.removeEventListener("pointerdown", onDocumentPointer, true);
});

onMounted(() => {
  document.addEventListener("keydown", onKey);
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onDocumentPointer, true);
  document.removeEventListener("keydown", onKey);
});
</script>

<template>
  <UiTooltip :content="t('terminal.activity.monitoringTitle')"><button
    v-if="serving.length"
    type="button"
    class="statusbar-item terminal-activity-button"
    :aria-expanded="open"
   
    @click="toggle"
  >
    <i class="ds-dot" data-tone="success" />
    {{ t('terminal.activity.runningCount', { count: serving.length }) }}
  </button></UiTooltip>
  <div
    v-if="open"
    ref="menu"
    class="terminal-activity-popover"
    :style="menuStyle"
    role="dialog"
    :aria-label="t('terminal.activity.dialogAria')"
  >
    <ul>
      <li v-for="item in serving" :key="item.id">
        <div class="terminal-activity-name">
          <strong>{{ item.title }}</strong>
          <span>{{ formatTerminalPorts(item.ports) }}</span>
        </div>
        <UiButton size="small" variant="ghost" @click="preview(item)">{{ t('terminal.activity.view') }}</UiButton>
        <UiButton size="small" @click="openWorkbench(item)">{{ t('terminal.activity.workbench') }}</UiButton>
      </li>
    </ul>
  </div>
  <TerminalPeek
    v-if="peeking"
    :id="peeking.id"
    :title="peeking.title"
    :ports="peeking.ports"
    @close="peekId = ''"
    @open-workbench="openPeekWorkbench"
  />
</template>

<style>
.terminal-activity-popover {
  position: fixed;
  z-index: var(--z-dropdown);
  width: 360px;
  max-width: calc(100vw - var(--spacing-4));
  padding: var(--spacing-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  box-shadow: var(--shadow-md);
}
.terminal-activity-popover ul {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-1);
  margin: 0;
  padding: 0;
  list-style: none;
}
.terminal-activity-popover li {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  min-width: 0;
  padding: var(--spacing-1) var(--spacing-2);
}
.terminal-activity-name {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--spacing-0-5);
  min-width: 0;
}
.terminal-activity-name strong,
.terminal-activity-name span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.terminal-activity-name strong {
  color: var(--color-text);
  font-size: var(--font-size-sm);
  font-weight: var(--font-weight-medium);
}
.terminal-activity-name span {
  color: var(--color-text-tertiary);
  font-size: var(--font-size-xs);
  font-variant-numeric: tabular-nums;
}
</style>
