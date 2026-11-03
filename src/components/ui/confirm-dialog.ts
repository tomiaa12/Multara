import { t } from "../../i18n";

export type ConfirmDialogRequest = {
  message: string;
  detail?: string;
  primary?: string;
  cancel?: string;
  type?: "warning" | "info";
  danger?: boolean;
};

type Pending = {
  request: ConfirmDialogRequest;
  resolve: (confirmed: boolean) => void;
};

let pending: Pending | null = null;
const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) listener();
}

export function getConfirmDialogRequest(): ConfirmDialogRequest | null {
  return pending?.request ?? null;
}

export function onConfirmDialogChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** 应用内二次确认；替代 window.confirm，避免右键菜单/失焦把原生弹窗点掉。 */
export function confirmDialog(
  message: string,
  options: Omit<ConfirmDialogRequest, "message"> = {},
): Promise<boolean> {
  pending?.resolve(false);
  const danger = options.danger === true;
  const request: ConfirmDialogRequest = {
    message,
    detail: options.detail,
    type: options.type ?? (danger ? "warning" : "info"),
    danger,
    primary: options.primary ?? (danger ? t("common.delete") : t("common.confirm")),
    cancel: options.cancel ?? t("common.cancel"),
  };
  return new Promise((resolve) => {
    pending = { request, resolve };
    notify();
  });
}

export function finishConfirmDialog(confirmed: boolean) {
  const current = pending;
  pending = null;
  notify();
  current?.resolve(confirmed);
}
