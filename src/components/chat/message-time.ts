import { t } from "../../i18n";
import { formatAbsoluteTime as formatAbsoluteLocale } from "../ui/relative-time";

export function formatAbsoluteTime(iso: string): string {
  return formatAbsoluteLocale(iso);
}

export function formatRelativeTime(iso: string, now = Date.now()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const elapsed = Math.max(0, now - date.getTime());
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 1) return t("time.justNow");
  if (minutes < 60) return t("time.minutesAgo", { n: minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t("time.hoursAgo", { n: hours });
  const days = Math.floor(hours / 24);
  if (days === 1) return t("time.yesterday");
  if (days < 30) return t("time.daysAgo", { n: days });
  return formatAbsoluteLocale(iso);
}
