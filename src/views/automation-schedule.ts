import { t } from "../i18n";

export type ScheduleTimeMode = "at" | "every";
/** 每天 / 每周指定星期 / 每月某一天。星期 0=周日 … 6=周六，与 cron 一致。 */
export type ScheduleRepeat = "daily" | "weekly" | "monthly";

export const WEEKDAY_DAYS = [0, 1, 2, 3, 4, 5, 6] as const;

export function weekdayLabel(day: number): string {
  return t(`automation.weekday.${day}`);
}

export type ScheduleDraft = {
  mode: ScheduleTimeMode;
  /** HH:MM for "at" mode */
  time: string;
  /** Hours between runs for "every" mode (1–23) */
  everyHours: number;
  repeat: ScheduleRepeat;
  /** 每周选中的星期，至少一天。 */
  weekdays: number[];
  /** 每月的日，1–31。 */
  monthDay: number;
  timezone: string;
  /** Non-null when cron cannot be represented by the structured editor */
  raw: string | null;
};

export type SchedulePreview = {
  label: string;
  cron: string;
  next: Array<{ at: Date; relative: string }>;
};

const WEEKDAY_MAP: Record<string, number> = {
  Sun: 0,
  Mon: 1,
  Tue: 2,
  Wed: 3,
  Thu: 4,
  Fri: 5,
  Sat: 6,
};

export function defaultScheduleDraft(timezone = browserTimezone()): ScheduleDraft {
  return {
    mode: "at",
    time: "09:00",
    everyHours: 1,
    repeat: "daily",
    weekdays: [1, 2, 3, 4, 5],
    monthDay: 1,
    timezone,
    raw: null,
  };
}

export function browserTimezone() {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
}

export const COMMON_TIMEZONES = [
  "Asia/Shanghai",
  "Asia/Hong_Kong",
  "Asia/Tokyo",
  "Asia/Singapore",
  "UTC",
  "America/Los_Angeles",
  "America/New_York",
  "Europe/London",
];

export function pad2(n: number) {
  return String(n).padStart(2, "0");
}

export function parseTime(value: string): { hour: number; minute: number } | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
  if (!match) return null;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  if (!Number.isInteger(hour) || !Number.isInteger(minute)) return null;
  if (hour < 0 || hour > 23 || minute < 0 || minute > 59) return null;
  return { hour, minute };
}

/** Build a 5-field cron (min hour day month dow) from the structured draft. */
export function toCron(draft: ScheduleDraft): string {
  if (draft.raw?.trim()) return draft.raw.trim();
  if (draft.mode === "every") {
    const hours = Math.min(23, Math.max(1, Math.floor(draft.everyHours) || 1));
    return hours === 1 ? "0 * * * *" : `0 */${hours} * * *`;
  }
  const parts = parseTime(draft.time) ?? { hour: 9, minute: 0 };
  if (draft.repeat === "monthly") {
    const day = Math.min(31, Math.max(1, Math.floor(draft.monthDay) || 1));
    return `${parts.minute} ${parts.hour} ${day} * *`;
  }
  if (draft.repeat === "weekly") {
    return `${parts.minute} ${parts.hour} * * ${dowField(draft.weekdays)}`;
  }
  return `${parts.minute} ${parts.hour} * * *`;
}

/** 把选中的星期收成 cron 的日字段：1,2,3,4,5 → 1-5。 */
export function dowField(days: number[]): string {
  const unique = [...new Set(days.filter((day) => day >= 0 && day <= 6))].sort((a, b) => a - b);
  if (!unique.length) return "1";
  const runs: string[] = [];
  let start = unique[0]!;
  let prev = start;
  for (const day of unique.slice(1)) {
    if (day === prev + 1) {
      prev = day;
      continue;
    }
    runs.push(start === prev ? `${start}` : `${start}-${prev}`);
    start = day;
    prev = day;
  }
  runs.push(start === prev ? `${start}` : `${start}-${prev}`);
  return runs.join(",");
}

function parseDow(field: string): number[] | null {
  if (field === "*") return null;
  const days = new Set<number>();
  for (const part of field.split(",")) {
    const range = /^(\d)(?:-(\d))?$/.exec(part);
    if (!range) return null;
    const lo = Number(range[1]);
    const hi = range[2] ? Number(range[2]) : lo;
    if (lo > 6 || hi > 6 || hi < lo) return null;
    for (let day = lo; day <= hi; day += 1) days.add(day);
  }
  return days.size ? [...days].sort((a, b) => a - b) : null;
}

/** Parse a known simple cron back into a draft; complex expressions become raw. */
export function fromCron(expression: string | null | undefined, timezone: string): ScheduleDraft {
  const base = defaultScheduleDraft(timezone);
  const cron = expression?.trim() ?? "";
  if (!cron) return base;

  const everyHour = /^0 \*\/([1-9]|1\d|2[0-3]) \* \* \*$/.exec(cron);
  if (everyHour) {
    return { ...base, mode: "every", everyHours: Number(everyHour[1]), raw: null };
  }
  if (cron === "0 * * * *") {
    return { ...base, mode: "every", everyHours: 1, raw: null };
  }

  const at = /^(\d{1,2}) (\d{1,2}) (\S+) \* (\S+)$/.exec(cron);
  if (at) {
    const minute = Number(at[1]);
    const hour = Number(at[2]);
    const dom = at[3]!;
    const dow = at[4]!;
    if (minute <= 59 && hour <= 23) {
      const time = `${pad2(hour)}:${pad2(minute)}`;
      if (dom === "*" && dow === "*") {
        return { ...base, mode: "at", time, repeat: "daily", raw: null };
      }
      if (dom === "*") {
        const weekdays = parseDow(dow);
        if (weekdays) {
          return { ...base, mode: "at", time, repeat: "weekly", weekdays, raw: null };
        }
      }
      if (/^\d{1,2}$/.test(dom) && dow === "*") {
        const monthDay = Number(dom);
        if (monthDay >= 1 && monthDay <= 31) {
          return { ...base, mode: "at", time, repeat: "monthly", monthDay, raw: null };
        }
      }
    }
  }

  return { ...base, raw: cron };
}

export function describeSchedule(draft: ScheduleDraft): string {
  if (draft.raw?.trim()) return draft.raw.trim();
  if (draft.mode === "every") {
    const hours = Math.min(23, Math.max(1, Math.floor(draft.everyHours) || 1));
    return hours === 1 ? t("automation.everyHour") : t("automation.everyNHours", { n: hours });
  }
  const time = parseTime(draft.time);
  const clock = time ? `${pad2(time.hour)}:${pad2(time.minute)}` : draft.time;
  if (draft.repeat === "monthly") {
    const day = Math.min(31, Math.max(1, Math.floor(draft.monthDay) || 1));
    return t("automation.scheduleMonthly", { time: clock, day });
  }
  if (draft.repeat === "weekly") {
    const labels = [...new Set(draft.weekdays.filter((day) => day >= 0 && day <= 6))]
      .sort((a, b) => a - b)
      .map((day) => weekdayLabel(day));
    return t("automation.scheduleWeekly", { time: clock, days: labels.join("、") || weekdayLabel(1) });
  }
  return t("automation.scheduleDaily", { time: clock });
}

type ZoneParts = {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
  dow: number;
};

export function zonedParts(date: Date, timeZone: string): ZoneParts {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
    weekday: "short",
  }).formatToParts(date);
  const get = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((part) => part.type === type)?.value ?? "0";
  return {
    year: Number(get("year")),
    month: Number(get("month")),
    day: Number(get("day")),
    hour: Number(get("hour")),
    minute: Number(get("minute")),
    second: Number(get("second")),
    dow: WEEKDAY_MAP[get("weekday")] ?? 0,
  };
}

function matchesField(field: string, value: number): boolean {
  if (field === "*") return true;
  if (/^\d+$/.test(field)) return Number(field) === value;
  if (field.includes("-")) {
    const [start, end] = field.split("-").map(Number);
    return value >= start && value <= end;
  }
  if (field.startsWith("*/")) {
    const step = Number(field.slice(2));
    return step > 0 && value % step === 0;
  }
  if (field.includes(",")) {
    return field.split(",").some((item) => matchesField(item, value));
  }
  return false;
}

export function cronMatches(expression: string, parts: ZoneParts): boolean {
  const fields = expression.trim().split(/\s+/);
  if (fields.length !== 5) return false;
  const [minute, hour, day, month, dow] = fields;
  return (
    matchesField(minute, parts.minute)
    && matchesField(hour, parts.hour)
    && matchesField(day, parts.day)
    && matchesField(month, parts.month)
    && matchesField(dow, parts.dow)
  );
}

/** Walk forward minute-by-minute (capped) to find the next matching instants. */
export function nextCronDates(
  expression: string,
  timeZone: string,
  count = 3,
  from = new Date(),
): Date[] {
  const fields = expression.trim().split(/\s+/);
  if (fields.length !== 5) return [];
  const found: Date[] = [];
  // Start at the next whole minute.
  let cursor = new Date(from.getTime());
  cursor.setSeconds(0, 0);
  cursor = new Date(cursor.getTime() + 60_000);
  // Cap search window: ~40 days of minutes.
  const limit = 40 * 24 * 60;
  for (let i = 0; i < limit && found.length < count; i += 1) {
    const parts = zonedParts(cursor, timeZone);
    if (cronMatches(expression, parts)) found.push(new Date(cursor.getTime()));
    cursor = new Date(cursor.getTime() + 60_000);
  }
  return found;
}

export function formatRelative(target: Date, now = new Date()): string {
  const diffMs = Math.max(0, target.getTime() - now.getTime());
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diffMs < minute) return t("automation.relativeUnderMinute");
  if (diffMs < hour) return t("automation.relativeMinutes", { n: Math.floor(diffMs / minute) });
  if (diffMs < day) {
    const hours = Math.floor(diffMs / hour);
    const minutes = Math.floor((diffMs % hour) / minute);
    return minutes
      ? t("automation.relativeHoursMinutes", { hours, minutes })
      : t("automation.relativeHours", { hours });
  }
  const days = Math.floor(diffMs / day);
  const hours = Math.floor((diffMs % day) / hour);
  const minutes = Math.floor((diffMs % hour) / minute);
  return t("automation.relativeDays", { days, hours, minutes });
}

/** Prefer a readable Chinese date like `9月29日 09:00`. */
export function formatNextLabel(date: Date, timeZone: string): string {
  const parts = zonedParts(date, timeZone);
  return t("automation.nextAt", {
    month: parts.month,
    day: parts.day,
    time: `${pad2(parts.hour)}:${pad2(parts.minute)}`,
  });
}

export function buildSchedulePreview(draft: ScheduleDraft, now = new Date()): SchedulePreview {
  const cron = toCron(draft);
  const next = nextCronDates(cron, draft.timezone, 3, now).map((at) => ({
    at,
    relative: formatRelative(at, now),
  }));
  return {
    label: describeSchedule(draft),
    cron,
    next,
  };
}

/** 新建自动化时的默认 runbook；文案走 i18n。 */
export function runbookTemplate(): string {
  return t("automation.runbookTemplate");
}
