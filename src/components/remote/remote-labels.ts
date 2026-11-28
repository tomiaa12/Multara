import type { RemoteConnState, RemoteRole } from "../../domain/remote";
import { t } from "../../i18n";

export function connStateLabel(state?: RemoteConnState | null) {
  return t(`remote.conn.${state ?? "offline"}`);
}

export function roleLabel(role?: RemoteRole | null) {
  return role ? t(`remote.role.${role}`) : "";
}

export function permLabel(perm?: string | null) {
  return perm ? t(`remote.perm.${perm}`) : "";
}

export function requestStatusLabel(status: string) {
  return t(`remote.requestStatus.${status}`);
}

export function approvalStatusLabel(status: string) {
  return t(`remote.approvalStatus.${status}`);
}

export function auditOutcomeLabel(outcome: string) {
  return t(`remote.outcome.${outcome}`);
}

export function auditActionLabel(action: string) {
  return t(`remote.action.${action}`);
}

/** 指纹按 4 位一组显示，便于两端人工核对。 */
export function formatFingerprint(fingerprint: string) {
  return fingerprint.match(/.{1,4}/g)?.join(" ") ?? "";
}
