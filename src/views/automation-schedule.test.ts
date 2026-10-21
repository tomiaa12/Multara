import { describe, expect, it } from "vitest";
import {
  buildSchedulePreview,
  cronMatches,
  describeSchedule,
  fromCron,
  nextCronDates,
  parseTime,
  toCron,
  zonedParts,
  type ScheduleDraft,
} from "./automation-schedule";

function draft(partial: Partial<ScheduleDraft> = {}): ScheduleDraft {
  return {
    mode: "at",
    time: "09:00",
    everyHours: 1,
    repeat: "daily",
    weekdays: [1, 2, 3, 4, 5],
    monthDay: 1,
    timezone: "UTC",
    raw: null,
    ...partial,
  };
}

describe("automation-schedule", () => {
  it("parses HH:MM", () => {
    expect(parseTime("09:00")).toEqual({ hour: 9, minute: 0 });
    expect(parseTime("23:59")).toEqual({ hour: 23, minute: 59 });
    expect(parseTime("24:00")).toBeNull();
    expect(parseTime("9:00")).toEqual({ hour: 9, minute: 0 });
  });

  it("maps daily, weekly days and monthly to cron", () => {
    expect(toCron(draft({ time: "09:00", repeat: "daily" }))).toBe("0 9 * * *");
    expect(toCron(draft({ time: "18:30", repeat: "weekly", weekdays: [1, 2, 3, 4, 5] }))).toBe("30 18 * * 1-5");
    expect(toCron(draft({ time: "10:15", repeat: "weekly", weekdays: [1, 3, 5] }))).toBe("15 10 * * 1,3,5");
    expect(toCron(draft({ time: "08:00", repeat: "monthly", monthDay: 15 }))).toBe("0 8 15 * *");
  });

  it("maps every-N-hours to cron", () => {
    expect(toCron(draft({ mode: "every", everyHours: 1 }))).toBe("0 * * * *");
    expect(toCron(draft({ mode: "every", everyHours: 6 }))).toBe("0 */6 * * *");
  });

  it("round-trips simple cron expressions", () => {
    expect(fromCron("0 9 * * *", "UTC")).toMatchObject({
      mode: "at",
      time: "09:00",
      repeat: "daily",
      raw: null,
    });
    expect(fromCron("30 18 * * 1-5", "UTC")).toMatchObject({
      mode: "at",
      time: "18:30",
      repeat: "weekly",
      weekdays: [1, 2, 3, 4, 5],
      raw: null,
    });
    expect(fromCron("0 8 15 * *", "UTC")).toMatchObject({
      mode: "at",
      time: "08:00",
      repeat: "monthly",
      monthDay: 15,
      raw: null,
    });
    expect(fromCron("0 */3 * * *", "UTC")).toMatchObject({
      mode: "every",
      everyHours: 3,
      raw: null,
    });
  });

  it("keeps unsupported cron as raw", () => {
    const parsed = fromCron("15 10 1 * 1", "UTC");
    expect(parsed.raw).toBe("15 10 1 * 1");
  });

  it("describes schedule in Chinese", () => {
    expect(describeSchedule(draft({ time: "09:00", repeat: "daily" }))).toBe("09:00 · 每天");
    expect(describeSchedule(draft({ time: "09:00", repeat: "weekly", weekdays: [1, 3, 5] }))).toBe("09:00 · 每周 一、三、五");
    expect(describeSchedule(draft({ time: "10:30", repeat: "monthly", monthDay: 15 }))).toBe("10:30 · 每月 15 日");
    expect(describeSchedule(draft({ mode: "every", everyHours: 2 }))).toBe("每 2 小时");
  });

  it("matches cron against zoned parts", () => {
    const parts = zonedParts(new Date("2026-09-28T09:00:00.000Z"), "UTC");
    expect(cronMatches("0 9 * * *", parts)).toBe(true);
    expect(cronMatches("0 10 * * *", parts)).toBe(false);
  });

  it("previews next daily runs", () => {
    const from = new Date("2026-09-28T08:00:00.000Z");
    const next = nextCronDates("0 9 * * *", "UTC", 3, from);
    expect(next).toHaveLength(3);
    expect(next[0].toISOString()).toBe("2026-09-28T09:00:00.000Z");
    expect(next[1].toISOString()).toBe("2026-09-29T09:00:00.000Z");
    expect(next[2].toISOString()).toBe("2026-09-30T09:00:00.000Z");
  });

  it("previews weekday-only runs", () => {
    // 2026-09-25 is Friday; next weekday after Sat morning should be Mon
    const from = new Date("2026-09-26T08:00:00.000Z"); // Saturday
    const next = nextCronDates("0 9 * * 1-5", "UTC", 2, from);
    expect(next[0].toISOString()).toBe("2026-09-28T09:00:00.000Z"); // Monday
    expect(next[1].toISOString()).toBe("2026-09-29T09:00:00.000Z"); // Tuesday
  });

  it("builds a preview payload", () => {
    const preview = buildSchedulePreview(
      draft({ time: "09:00", repeat: "daily" }),
      new Date("2026-09-28T08:00:00.000Z"),
    );
    expect(preview.cron).toBe("0 9 * * *");
    expect(preview.label).toBe("09:00 · 每天");
    expect(preview.next).toHaveLength(3);
    expect(preview.next[0].relative).toContain("后");
  });
});
