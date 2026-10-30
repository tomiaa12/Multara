import { t } from "../../i18n";

type LeaveHandler = () => Promise<boolean>;
type DirtyChecker = () => boolean;
type DirtyPathsGetter = () => string[];
export type UnsavedAction = "save" | "discard" | "cancel";

export interface UnsavedPrompt {
  title: string;
  message: string;
}

export interface UnsavedEditorOwner {
  leave: LeaveHandler;
  dirty: DirtyChecker;
  paths: DirtyPathsGetter;
}

/** 每个工作台实例一项，末尾是最近激活的实例；keep-alive 下多个工作台并存，只有当前实例参与检查。 */
const owners: UnsavedEditorOwner[] = [];
let confirming = false;
let pending: {
  prompt: UnsavedPrompt;
  resolve: (action: UnsavedAction) => void;
} | null = null;
const listeners = new Set<() => void>();

function currentOwner(): UnsavedEditorOwner | undefined {
  return owners.at(-1);
}

function removeOwner(owner: UnsavedEditorOwner) {
  const index = owners.indexOf(owner);
  if (index >= 0) owners.splice(index, 1);
}

/** 把该实例置为当前实例（挂载、激活时调用）。 */
export function activateUnsavedEditor(owner: UnsavedEditorOwner) {
  removeOwner(owner);
  owners.push(owner);
}

/** 登记并置为当前实例；返回的函数只移除自己，不影响其它工作台。 */
export function registerUnsavedEditor(owner: UnsavedEditorOwner): () => void {
  activateUnsavedEditor(owner);
  return () => removeOwner(owner);
}

export function listDirtyEditorPaths(): string[] {
  try {
    return [...new Set(currentOwner()?.paths() ?? [])].filter(Boolean);
  } catch {
    return [];
  }
}

export function formatRollbackDirtyMessage(paths: string[]) {
  return t("unsavedEditor.rollbackDirty", { paths: paths.join(t("common.listSep")) });
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
    return currentOwner()?.dirty() ?? false;
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

export function askUnsaved(message: string, title = t("unsavedEditor.fileNotSaved")): Promise<UnsavedAction> {
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
    const owner = currentOwner();
    if (owner) return await owner.leave();
    const action = await askUnsaved(
      t("unsavedEditor.closeLoseChanges"),
      t("unsavedEditor.unsavedChangesTitle"),
    );
    return action !== "cancel";
  } catch {
    return true;
  } finally {
    confirming = false;
  }
}
