type LeaveHandler = () => Promise<boolean>;
type DirtyChecker = () => boolean;
type DirtyPathsGetter = () => string[];
export type UnsavedAction = "save" | "discard" | "cancel";

export interface UnsavedPrompt {
  title: string;
  message: string;
}

let leaveHandler: LeaveHandler | null = null;
let dirtyChecker: DirtyChecker | null = null;
let dirtyPathsGetter: DirtyPathsGetter | null = null;
let confirming = false;
let pending: {
  prompt: UnsavedPrompt;
  resolve: (action: UnsavedAction) => void;
} | null = null;
const listeners = new Set<() => void>();

export function registerUnsavedLeaveHandler(handler: LeaveHandler | null) {
  leaveHandler = handler;
}

export function registerUnsavedDirtyChecker(checker: DirtyChecker | null) {
  dirtyChecker = checker;
}

export function registerDirtyEditorPaths(getter: DirtyPathsGetter | null) {
  dirtyPathsGetter = getter;
}

export function listDirtyEditorPaths(): string[] {
  try {
    return [...new Set(dirtyPathsGetter?.() ?? [])].filter(Boolean);
  } catch {
    return [];
  }
}

export function formatRollbackDirtyMessage(paths: string[]) {
  return `以下文件有未保存修改，Rollback 将覆盖：${paths.join("、")}`;
}

export async function confirmRollbackIfDirty(
  paths: string[],
  ask: (message: string) => Promise<boolean>,
): Promise<boolean> {
  if (!paths.length) return true;
  return ask(formatRollbackDirtyMessage(paths));
}

export function hasUnsavedEditors() {
  try {
    return dirtyChecker?.() ?? false;
  } catch {
    return false;
  }
}

export function getUnsavedPrompt(): UnsavedPrompt | null {
  return pending?.prompt ?? null;
}

export function onUnsavedPromptChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function notifyPrompt() {
  for (const listener of listeners) listener();
}

export function askUnsaved(message: string, title = "文件尚未保存"): Promise<UnsavedAction> {
  pending?.resolve("cancel");
  return new Promise((resolve) => {
    pending = { prompt: { title, message }, resolve };
    notifyPrompt();
  });
}

export function finishUnsavedPrompt(action: UnsavedAction) {
  const current = pending;
  pending = null;
  notifyPrompt();
  current?.resolve(action);
}

export async function confirmLeaveUnsavedEditors() {
  if (confirming) return false;
  confirming = true;
  try {
    if (!hasUnsavedEditors()) return true;
    if (leaveHandler) return await leaveHandler();
    const action = await askUnsaved(
      "有文件尚未保存。关闭窗口将丢失未保存的更改。",
      "有未保存的更改",
    );
    return action !== "cancel";
  } catch {
    return true;
  } finally {
    confirming = false;
  }
}
