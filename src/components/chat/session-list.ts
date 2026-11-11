export type SessionListGroupKey = "today" | "yesterday" | "week" | "older";

export interface SessionListEntry {
  id: string;
  title: string;
  activityAt: string;
}

export interface SessionListGroup<T extends SessionListEntry> {
  key: SessionListGroupKey;
  items: T[];
}

export type SessionListLayout = "side" | "drawer";

export const SESSION_LIST_DEFAULT_WIDTH = 220;
const SESSION_LIST_MIN_WIDTH = 168;
const SESSION_LIST_MAX_WIDTH = 320;
/** 并排时聊天区至少留这么宽，否则消息和输入框会挤坏 */
const CHAT_MIN_WIDTH = 340;

const GROUP_ORDER: SessionListGroupKey[] = ["today", "yesterday", "week", "older"];

/** 面板够宽时列表与聊天并排，否则收进抽屉 */
export function sessionListLayout(panelWidth: number): SessionListLayout {
  return panelWidth >= SESSION_LIST_DEFAULT_WIDTH + CHAT_MIN_WIDTH ? "side" : "drawer";
}

export function clampSessionListWidth(width: number, panelWidth: number) {
  const max = Math.max(SESSION_LIST_MIN_WIDTH, Math.min(SESSION_LIST_MAX_WIDTH, panelWidth - CHAT_MIN_WIDTH));
  return Math.round(Math.min(max, Math.max(SESSION_LIST_MIN_WIDTH, width)));
}

function dayStart(now: Date, offsetDays = 0) {
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() - offsetDays).getTime();
}

export function sessionListGroupKey(activityAt: string, now: Date): SessionListGroupKey {
  const time = Date.parse(activityAt);
  if (Number.isNaN(time)) return "older";
  if (time >= dayStart(now)) return "today";
  if (time >= dayStart(now, 1)) return "yesterday";
  if (time >= dayStart(now, 6)) return "week";
  return "older";
}

/** 取几个时间里最新的一个；会话的 updatedAt 只在改标题/对象时变，要和最后一条消息比 */
export function latestActivity(...values: Array<string | null | undefined>) {
  let best = "";
  let bestTime = Number.NEGATIVE_INFINITY;
  for (const value of values) {
    if (!value) continue;
    const time = Date.parse(value);
    if (Number.isNaN(time) || time <= bestTime) continue;
    best = value;
    bestTime = time;
  }
  return best;
}

/** 按标题过滤、按最近活动倒序，再分成今天 / 昨天 / 近 7 天 / 更早；空组不返回 */
export function groupSessionList<T extends SessionListEntry>(items: T[], query: string, now: Date): SessionListGroup<T>[] {
  const needle = query.trim().toLocaleLowerCase();
  const matched = needle ? items.filter((item) => item.title.toLocaleLowerCase().includes(needle)) : items;
  const sorted = [...matched].sort((a, b) => (Date.parse(b.activityAt) || 0) - (Date.parse(a.activityAt) || 0));
  const buckets = new Map<SessionListGroupKey, T[]>();
  for (const item of sorted) {
    const key = sessionListGroupKey(item.activityAt, now);
    const bucket = buckets.get(key);
    if (bucket) bucket.push(item);
    else buckets.set(key, [item]);
  }
  return GROUP_ORDER.flatMap((key) => {
    const bucket = buckets.get(key);
    return bucket ? [{ key, items: bucket }] : [];
  });
}
