import { toRaw } from "vue";

function sameValue(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (typeof left !== "object" || typeof right !== "object" || !left || !right) return false;
  if (Array.isArray(left) !== Array.isArray(right)) return false;
  const leftKeys = Object.keys(left);
  if (leftKeys.length !== Object.keys(right).length) return false;
  return leftKeys.every((key) =>
    Object.prototype.hasOwnProperty.call(right, key)
    && sameValue((left as Record<string, unknown>)[key], (right as Record<string, unknown>)[key]),
  );
}

/**
 * 按 id 合并刷新结果：内容没变的条目沿用旧对象，整体没变时直接返回旧数组，
 * 避免整数组替换让所有消费者重渲染。
 */
export function mergeById<T extends { id: string }>(previous: T[], next: T[]): T[] {
  const rawPrevious = toRaw(previous);
  const byId = new Map(rawPrevious.map((item) => [item.id, toRaw(item)]));
  let changed = rawPrevious.length !== next.length;
  const merged = next.map((item, index) => {
    const old = byId.get(item.id);
    const kept = old && sameValue(old, item) ? old : item;
    if (kept !== toRaw(rawPrevious[index])) changed = true;
    return kept;
  });
  return changed ? merged : previous;
}
