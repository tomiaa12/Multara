/** 轻量 toast：替代无反馈的操作（如不能删除触发器） */

export type ToastTone = "info" | "warning" | "error" | "success";

export type ToastItem = {
  id: number;
  message: string;
  tone: ToastTone;
};

let seq = 0;
let items: ToastItem[] = [];
const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) listener();
}

export function getToasts(): ToastItem[] {
  return items;
}

export function onToastsChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function showToast(message: string, tone: ToastTone = "info", durationMs = 3200) {
  const id = ++seq;
  items = [...items, { id, message, tone }];
  notify();
  window.setTimeout(() => dismissToast(id), durationMs);
}

export function dismissToast(id: number) {
  const next = items.filter((item) => item.id !== id);
  if (next.length === items.length) return;
  items = next;
  notify();
}
