<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { terminalOptions } from "./terminal-theme";
import * as backend from "../../services/backend";
import {
  formatTerminalPorts,
  subscribeTerminalOutput,
  unsubscribeTerminalOutput,
} from "../../stores/running-terminals";

const props = defineProps<{
  id: string;
  title: string;
  ports: number[];
}>();

const emit = defineEmits<{
  close: [];
  openWorkbench: [];
}>();

const host = ref<HTMLElement | null>(null);
let term: Terminal | undefined;
let fitAddon: FitAddon | undefined;
let write: ((data: Uint8Array) => void) | undefined;

function refit() {
  if (!fitAddon || !host.value?.clientWidth || !host.value.clientHeight) return;
  // 只改这块预览的显示，不调用 terminalResize，避免把工作台里那个终端的列数改掉。
  fitAddon.fit();
}

onMounted(async () => {
  await nextTick();
  const element = host.value;
  if (!element) return;
  term = new Terminal(terminalOptions());
  fitAddon = new FitAddon();
  term.loadAddon(fitAddon);
  term.open(element);
  element.querySelector(".xterm-viewport")?.classList.add("vs-scroll");
  refit();
  write = (data) => term?.write(data);
  term.write(subscribeTerminalOutput(props.id, write));
  term.onData((data) => {
    void backend.terminalWrite(props.id, data);
  });
  term.focus();
  window.addEventListener("resize", refit);
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", refit);
  if (write) unsubscribeTerminalOutput(props.id, write);
  term?.dispose();
  term = undefined;
  fitAddon = undefined;
});
</script>

<template>
  <section class="terminal-peek" role="dialog" aria-label="终端预览">
    <header class="terminal-peek-bar">
      <Codicon name="terminal" :size="14" />
      <strong>{{ title }}</strong>
      <span v-if="ports.length">{{ formatTerminalPorts(ports) }}</span>
      <div class="terminal-peek-actions">
        <button type="button" @click="emit('openWorkbench')">工作台</button>
        <button type="button" title="关闭" @click="emit('close')">
          <Codicon name="close" :size="14" />
        </button>
      </div>
    </header>
    <div ref="host" class="terminal-body terminal-peek-body" />
  </section>
</template>

<style>
.terminal-peek {
  position: fixed;
  right: var(--spacing-3);
  bottom: var(--layout-statusbar-height);
  z-index: var(--z-floating);
  display: flex;
  flex-direction: column;
  width: min(640px, calc(100vw - var(--spacing-6)));
  height: 280px;
  overflow: hidden;
  border: 1px solid var(--color-border);
  border-bottom: 0;
  border-radius: var(--radius-md) var(--radius-md) 0 0;
  background: var(--color-surface-raised);
  box-shadow: var(--shadow-lg);
}
.terminal-peek-bar {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  height: 35px;
  padding: 0 var(--spacing-3);
  border-bottom: 1px solid var(--color-border-light);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}
.terminal-peek-bar strong {
  min-width: 0;
  overflow: hidden;
  color: var(--color-text);
  font-weight: var(--font-weight-medium);
  text-overflow: ellipsis;
  white-space: nowrap;
}
.terminal-peek-bar span {
  flex: 0 0 auto;
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}
.terminal-peek-actions {
  display: flex;
  align-items: center;
  gap: var(--spacing-1);
  margin-left: auto;
}
.terminal-peek-actions button {
  display: inline-flex;
  align-items: center;
  height: var(--control-height-sm);
  padding: 0 var(--spacing-2);
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-secondary);
  font: inherit;
  cursor: pointer;
}
.terminal-peek-actions button:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}
.terminal-peek-body {
  flex: 1;
  min-height: 0;
}
</style>
