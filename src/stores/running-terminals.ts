import { ref, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { terminalServices } from "../services/backend";
import { decodeTerminalChunk } from "../utils/terminal-decode";

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

const scrollback = new Map<string, { chunks: Uint8Array[]; size: number }>();
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
    (count) => (count ? startPortPolling() : stopPortPolling()),
    { immediate: true },
  );
  document.addEventListener("visibilitychange", () => (document.hidden ? stopPortPolling() : startPortPolling()));
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

function startPortPolling() {
  if (pollTimer || document.hidden || !runningTerminals.value.length) return;
  void refreshTerminalPorts();
  pollTimer = setInterval(() => {
    void refreshTerminalPorts();
  }, PORT_POLL_INTERVAL_MS);
}

function stopPortPolling() {
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = undefined;
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
  const entry = scrollback.get(id) ?? { chunks: [], size: 0 };
  scrollback.set(id, entry);
  const { chunks } = entry;
  chunks.push(data);
  entry.size += data.length;
  while (entry.size > SCROLLBACK_BYTES && chunks.length > 1) {
    entry.size -= chunks.shift()?.length ?? 0;
  }
  const first = chunks[0];
  if (first && first.length > SCROLLBACK_BYTES) {
    chunks[0] = first.slice(first.length - SCROLLBACK_BYTES);
    entry.size = SCROLLBACK_BYTES;
  }
}

function snapshot(id: string) {
  const entry = scrollback.get(id);
  const out = new Uint8Array(entry?.size ?? 0);
  let offset = 0;
  for (const chunk of entry?.chunks ?? []) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}
