<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { X } from "@lucide/vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { Runtime } from "../domain/types";
import { t } from "../i18n";
import { terminalOptions } from "../components/workbench/terminal-theme";
import { decodeTerminalChunk } from "../utils/terminal-decode";
import {
  getRuntimeAccountStatus,
  getRuntimeLoginCommand,
  terminalClose,
  terminalOpen,
  terminalResize,
  terminalWrite,
  type RuntimeAccountStatus,
} from "../services/backend";

const LOGIN_POLL_MS = 3000;
/** 最多等 10 分钟 */
const LOGIN_POLL_LIMIT = 200;

const props = defineProps<{ runtime: Runtime }>();
const emit = defineEmits<{ close: []; done: [status: RuntimeAccountStatus] }>();

const host = ref<HTMLElement | null>(null);
const error = ref("");
let term: Terminal | undefined;
let fitAddon: FitAddon | undefined;
let terminalId = "";
let pollTimer = 0;
let unmounted = false;
const unlisteners: UnlistenFn[] = [];

function refit() {
  if (!fitAddon || !term || !host.value?.clientWidth) return;
  fitAddon.fit();
  if (terminalId) void terminalResize(terminalId, term.cols, term.rows);
}

/** 当前登录换成另一个账号就算登录完成。 */
function waitForLogin(before: string | null, attempt: number) {
  if (unmounted || attempt >= LOGIN_POLL_LIMIT) return;
  pollTimer = window.setTimeout(async () => {
    const next = await getRuntimeAccountStatus(props.runtime.id).catch(() => null);
    if (unmounted) return;
    if (next?.live.signedIn && next.live.accountId !== before) {
      emit("done", next);
      return;
    }
    waitForLogin(before, attempt + 1);
  }, LOGIN_POLL_MS);
}

async function start() {
  const login = await getRuntimeLoginCommand(props.runtime.id);
  const before = await getRuntimeAccountStatus(props.runtime.id).catch(() => null);
  const opened = await terminalOpen(login.cwd);
  if (unmounted) {
    void terminalClose(opened.id);
    return;
  }
  terminalId = opened.id;
  unlisteners.push(
    await listen<{ id: string; data: string }>("terminal-data", (event) => {
      if (event.payload.id === terminalId) term?.write(decodeTerminalChunk(event.payload.data));
    }),
    await listen<{ id: string; exitCode?: number }>("terminal-exit", (event) => {
      if (event.payload.id !== terminalId) return;
      term?.write(`\r\n\x1b[90m${t("terminal.processExited", { code: event.payload.exitCode ?? 1 })}\x1b[0m\r\n`);
    }),
  );
  refit();
  void terminalWrite(terminalId, `${login.command}\r`);
  waitForLogin(before?.live.accountId ?? null, 0);
}

onMounted(async () => {
  await nextTick();
  if (!host.value) return;
  term = new Terminal(terminalOptions());
  fitAddon = new FitAddon();
  term.loadAddon(fitAddon);
  term.open(host.value);
  host.value.querySelector(".xterm-viewport")?.classList.add("vs-scroll");
  term.onData((data) => {
    if (terminalId) void terminalWrite(terminalId, data);
  });
  fitAddon.fit();
  term.focus();
  window.addEventListener("resize", refit);
  try {
    await start();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
});

onBeforeUnmount(() => {
  unmounted = true;
  window.clearTimeout(pollTimer);
  window.removeEventListener("resize", refit);
  unlisteners.forEach((stop) => stop());
  if (terminalId) void terminalClose(terminalId);
  term?.dispose();
});
</script>

<template>
  <UiModal @click.self="emit('close')">
    <section class="modal domain-modal runtime-login-terminal">
      <header class="modal-heading">
        <h2>{{ t("runtimeAccount.terminalTitle", { name: runtime.name }) }}</h2>
        <UiButton class="icon-button" @click="emit('close')"><X :size="16" /></UiButton>
      </header>
      <p class="skill-create-hint">{{ t("runtimeAccount.terminalHint") }}</p>
      <p v-if="error" class="form-error">{{ error }}</p>
      <div ref="host" class="terminal-body runtime-login-terminal-body" />
    </section>
  </UiModal>
</template>

<style scoped>
.runtime-login-terminal {
  width: min(760px, calc(100vw - var(--spacing-6)));
}
.runtime-login-terminal-body {
  height: 360px;
  overflow: hidden;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
}
</style>
