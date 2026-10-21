/** 数字气泡上限，超过后展示为「99+」。 */
export const COUNT_BADGE_LIMIT = 99;

/** 将计数格式化为气泡文案；超过上限显示 99+。 */
export function formatCountBadge(count: number, limit = COUNT_BADGE_LIMIT): string {
  if (!Number.isFinite(count)) return "";
  if (count > limit) return `${limit}+`;
  return String(Math.trunc(count));
}

/** badge 为 number 时格式化；已是 string 则原样展示。 */
export function resolveCountBadgeLabel(badge: string | number): string {
  if (typeof badge === "number") return formatCountBadge(badge);
  return badge;
}

/** 单数字气泡用正圆（border-radius: 50%）。 */
export function countBadgeDigitsAttr(label: string): "1" | undefined {
  if (label.length === 1) return "1";
  return undefined;
}
