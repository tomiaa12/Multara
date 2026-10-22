import { locale, t } from "../../i18n";

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;
const MONTH_MS = 30 * DAY_MS;
const YEAR_MS = 365 * DAY_MS;

/** 解析 ISO / 毫秒时间戳 / Date；无效返回 NaN。 */
export function parseTimeMs(input: string | number | Date | null | undefined): number {
  if (input == null || input === "") return Number.NaN;
  if (input instanceof Date) return input.getTime();
  if (typeof input === "number") return Number.isFinite(input) ? input : Number.NaN;
  return new Date(input).getTime();
}

/**
 * 相对时间：刚刚 / N 分钟前 / N 小时前 / N 天前 / N 个月前 / N 年前。
 */
export function formatRelativeTime(
  input: string | number | Date | null | undefined,
  now = Date.now(),
): string {
  const time = parseTimeMs(input);
  if (!Number.isFinite(time)) return "";
  const delta = Math.max(0, now - time);
  if (delta < MINUTE_MS) return t("time.justNow");
  if (delta < HOUR_MS) return t("time.minutesAgo", { n: Math.floor(delta / MINUTE_MS) });
  if (delta < DAY_MS) return t("time.hoursAgo", { n: Math.floor(delta / HOUR_MS) });
  if (delta < MONTH_MS) return t("time.daysAgo", { n: Math.floor(delta / DAY_MS) });
  if (delta < YEAR_MS) return t("time.monthsAgo", { n: Math.floor(delta / MONTH_MS) });
  return t("time.yearsAgo", { n: Math.floor(delta / YEAR_MS) });
}

/** 悬停用的绝对时间（本地时区）。 */
export function formatAbsoluteTime(
  input: string | number | Date | null | undefined,
): string {
  const time = parseTimeMs(input);
  if (!Number.isFinite(time)) return "";
  return new Date(time).toLocaleString(locale.value, { hour12: false });
}
