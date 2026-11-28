import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { reactive, shallowRef } from "vue";
import type { RemoteApproval, RemoteError, RemoteEventEnvelope } from "../domain/remote";
import { showToast } from "../components/ui/toast";
import { t } from "../i18n";

export const REMOTE_EVENT = "remote-event";
const APPROVAL_EVENT = "remote-approval";
const APPROVED = "approved";

export type ApprovalWait = {
  id: string;
  summary: string;
  expiresAt: string;
  cancel: () => void;
};

/** 正在等主机端审批的操作，全局等待条显示。 */
export const approvalWaits = reactive<ApprovalWait[]>([]);
/** 最近一次因权限不足被拒的请求，状态栏据此提供申请入口。 */
export const lastDenied = shallowRef<RemoteError | null>(null);
const toastedPerms = new Set<string>();

export class RemoteCallError extends Error {
  constructor(readonly detail: RemoteError) {
    super(detail.message);
  }
}

function asRemoteError(cause: unknown): RemoteError {
  const value = cause as Partial<RemoteError> | null;
  if (typeof value?.code !== "string") return { code: "internal", message: String(cause) };
  return { code: value.code, message: value.message ?? value.code, perm: value.perm, approvalId: value.approvalId };
}

function call<T>(hostId: string, cmd: string, args?: Record<string, unknown>, approval?: string) {
  return tauriInvoke<T>("remote_invoke", { hostId, cmd, args: args ?? {}, approval: approval ?? null })
    .catch((cause: unknown) => {
      throw new RemoteCallError(asRemoteError(cause));
    });
}

const DECISION_MESSAGES: Record<string, string> = {
  rejected: "remote.approval.rejected",
  expired: "remote.approval.expired",
  cancelled: "remote.approval.cancelled",
};

function waitForDecision(hostId: string, ticket: RemoteApproval): Promise<string> {
  return new Promise((resolve) => {
    let done = false;
    let unlisten: (() => void) | undefined;
    const timer = window.setTimeout(() => finish("expired"), Math.max(0, Date.parse(ticket.expiresAt) - Date.now()));
    function finish(status: string) {
      if (done) return;
      done = true;
      window.clearTimeout(timer);
      unlisten?.();
      const index = approvalWaits.findIndex((wait) => wait.id === ticket.id);
      if (index >= 0) approvalWaits.splice(index, 1);
      resolve(status);
    }
    approvalWaits.push({ id: ticket.id, summary: ticket.summary, expiresAt: ticket.expiresAt, cancel: () => finish("cancelled") });
    void listen<RemoteEventEnvelope>(REMOTE_EVENT, ({ payload }) => {
      if (payload?.hostId !== hostId || payload.event !== APPROVAL_EVENT) return;
      const decided = payload.payload as Partial<RemoteApproval> | null;
      if (decided?.id === ticket.id && decided.status !== "pending") finish(decided.status ?? "rejected");
    }).then((stop) => {
      if (done) stop();
      else unlisten = stop;
    });
  });
}

async function approveAndRetry<T>(hostId: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  const ticket = await call<RemoteApproval>(hostId, "remote.request_approval", { cmd, args: args ?? {} });
  const status = await waitForDecision(hostId, ticket);
  if (status !== APPROVED) {
    throw new RemoteCallError({ code: `approval_${status}`, message: t(DECISION_MESSAGES[status] ?? DECISION_MESSAGES.rejected) });
  }
  return call<T>(hostId, cmd, args, ticket.id);
}

function noteDenied(error: RemoteError) {
  lastDenied.value = error;
  const perm = error.perm ?? error.code;
  if (toastedPerms.has(perm)) return;
  toastedPerms.add(perm);
  showToast(error.message, "warning");
}

/** 发往远程主机；需要审批时自动申请、等待并带票据重试。 */
export async function remoteInvoke<T>(hostId: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await call<T>(hostId, cmd, args);
  } catch (cause) {
    const error = (cause as RemoteCallError).detail;
    if (error?.code === "approval_required") return approveAndRetry<T>(hostId, cmd, args);
    if (error?.code === "forbidden") noteDenied(error);
    throw cause;
  }
}
