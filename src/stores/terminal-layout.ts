/** 工作台 / 执行模式按需求记住终端面板是否打开。 */

const WORKBENCH_PREFIX = "multara.workbench.terminalOpen.";
const INBOX_PREFIX = "multara.inbox.terminalOpen.";
/** 旧版全局开关，迁移到按需求后仍可作缺省。 */
const INBOX_LEGACY_KEY = "multara.inbox.terminalOpen";

export function readProjectTerminalOpen(
  surface: "workbench" | "inbox",
  projectId: string,
): boolean {
  if (!projectId) return false;
  const key = surface === "workbench" ? WORKBENCH_PREFIX + projectId : INBOX_PREFIX + projectId;
  const stored = localStorage.getItem(key);
  if (stored === "1") return true;
  if (stored === "0") return false;
  if (surface === "inbox") return localStorage.getItem(INBOX_LEGACY_KEY) === "1";
  return false;
}

export function writeProjectTerminalOpen(
  surface: "workbench" | "inbox",
  projectId: string,
  open: boolean,
) {
  if (!projectId) return;
  const key = surface === "workbench" ? WORKBENCH_PREFIX + projectId : INBOX_PREFIX + projectId;
  localStorage.setItem(key, open ? "1" : "0");
  if (surface === "inbox") {
    localStorage.setItem(INBOX_LEGACY_KEY, open ? "1" : "0");
  }
}
