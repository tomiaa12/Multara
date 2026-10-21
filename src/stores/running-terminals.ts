import { ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { terminalServices } from "../services/backend";

const PORT_POLL_INTERVAL_MS = 2000;
const SCROLLBACK_BYTES = 256 * 1024;

export interface RunningTerminal {
  id: string;
  title: string;
  projectId: string;
  workspaceId: string;
  ports: number[];
}

export const runningTerminals = ref<RunningTerminal[]>([]);
export const terminalFocus = ref<{ id: string; nonce: number } | null>(null);

const scrollback = new Map<string, Uint8Array[]>();
const mirrors = new Map<string, Set<(data: Uint8Array) => void>>();
let focusNonce = 0;
let pollTimer: ReturnType<typeof setInterval> | undefined;
let watchReady = false;

export function formatTerminalPorts(ports: number[]) {
  return ports.map((port) => `:${port}`).join(" ");
}

export function trackTerminal(input: Omit<RunningTerminal, "ports">) {
  if (!input.id || !input.projectId) return;
  const existing = runningTerminals.value.find((item) => item.id === input.id);
  if (existing) {
    existing.title = input.title || existing.title;
    existing.projectId = input.projectId;
    existing.workspaceId = input.workspaceId;
    return;
  }
  runningTerminals.value = [
    ...runningTerminals.value,
    {
      id: input.id,
      title: input.title || "终端",
      projectId: input.projectId,
      workspaceId: input.workspaceId,
      ports: [],
    },
  ];
}

export function forgetTerminal(id: string) {
  runningTerminals.value = runningTerminals.value.filter((item) => item.id !== id);
  scrollback.delete(id);
  mirrors.delete(id);
  if (terminalFocus.value?.id === id) terminalFocus.value = null;
}

export function requestTerminalFocus(id: string) {
  focusNonce += 1;
  terminalFocus.value = { id, nonce: focusNonce };
}

export function subscribeTerminalOutput(id: string, write: (data: Uint8Array) => void) {
  const set = mirrors.get(id) ?? new Set();
  mirrors.set(id, set);
  set.add(write);
  return snapshot(id);
}

export function unsubscribeTerminalOutput(id: string, write: (data: Uint8Array) => void) {
  mirrors.get(id)?.delete(write);
}

export function startRunningTerminalWatch() {
  if (watchReady) return;
  watchReady = true;
  watch(
    () => runningTerminals.value.length,
    (count) => {
      if (!count) {
        if (pollTimer) clearInterval(pollTimer);
        pollTimer = undefined;
        return;
      }
      if (pollTimer) return;
      void refreshTerminalPorts();
      pollTimer = setInterval(() => {
        void refreshTerminalPorts();
      }, PORT_POLL_INTERVAL_MS);
    },
    { immediate: true },
  );
  if (!("__TAURI_INTERNALS__" in window)) return;
  void listen<{ id?: string; data?: string }>("terminal-data", (event) => {
    const id = event.payload?.id;
    const data = event.payload?.data;
    if (!id || !data) return;
    if (!runningTerminals.value.some((item) => item.id === id)) return;
    const bytes = decodeTerminalChunk(data);
    if (!bytes.length) return;
    pushScrollback(id, bytes);
    mirrors.get(id)?.forEach((write) => write(bytes));
  });
}

async function refreshTerminalPorts() {
  try {
    applyTerminalPorts(await terminalServices());
  } catch {
    // 下一轮再查。端口没变时状态栏保持上一次的结果。
  }
}

function applyTerminalPorts(services: Array<{ id: string; ports: number[] }>) {
  const portsById = new Map(services.map((item) => [item.id, [...item.ports].sort((left, right) => left - right)]));
  for (const item of runningTerminals.value) {
    const next = portsById.get(item.id) ?? [];
    if (item.ports.join() === next.join()) continue;
    item.ports = next;
  }
}

function pushScrollback(id: string, data: Uint8Array) {
  const chunks = scrollback.get(id) ?? [];
  scrollback.set(id, chunks);
  chunks.push(data);
  let size = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  while (size > SCROLLBACK_BYTES && chunks.length > 1) {
    size -= chunks[0]?.length ?? 0;
    chunks.shift();
  }
  const first = chunks[0];
  if (first && first.length > SCROLLBACK_BYTES) {
    chunks[0] = first.slice(first.length - SCROLLBACK_BYTES);
  }
}

function snapshot(id: string) {
  const chunks = scrollback.get(id) ?? [];
  const size = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const out = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

function decodeTerminalChunk(data: string) {
  try {
    const binary = atob(data);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  } catch {
    return new Uint8Array();
  }
}
