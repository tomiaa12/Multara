/**
 * 资源管理器多选模型（对应 VS Code 列表的 selection/focus/anchor）。
 * key 形如 `${workspaceId}:${path}`，工作区根的 path 为空串。
 */
export interface SelectionState {
  selected: string[];
  focus: string | null;
  anchor: string | null;
}

export const EMPTY_SELECTION: SelectionState = { selected: [], focus: null, anchor: null };

export function selectOnly(key: string): SelectionState {
  return { selected: [key], focus: key, anchor: key };
}

/** Ctrl/Cmd+点击：切换单项，锚点移到该项。 */
export function toggleSelection(state: SelectionState, key: string): SelectionState {
  const has = state.selected.includes(key);
  const selected = has ? state.selected.filter((item) => item !== key) : [...state.selected, key];
  return { selected, focus: key, anchor: key };
}

/**
 * Shift+点击/Shift+方向键：选中锚点到目标的连续区间。
 * `additive`（Ctrl+Shift）时保留原有选择。
 */
export function selectRange(
  state: SelectionState,
  order: readonly string[],
  key: string,
  additive = false,
): SelectionState {
  const anchor = state.anchor && order.includes(state.anchor) ? state.anchor : key;
  const from = order.indexOf(anchor);
  const to = order.indexOf(key);
  if (from === -1 || to === -1) return selectOnly(key);
  const range = order.slice(Math.min(from, to), Math.max(from, to) + 1);
  const selected = additive ? [...new Set([...state.selected, ...range])] : range;
  return { selected, focus: key, anchor };
}

export function selectAll(order: readonly string[], focus: string | null): SelectionState {
  return { selected: [...order], focus: focus ?? order[0] ?? null, anchor: focus ?? order[0] ?? null };
}

export type FocusMove = "next" | "previous" | "first" | "last" | "pageDown" | "pageUp";

export function moveFocus(
  order: readonly string[],
  current: string | null,
  move: FocusMove,
  pageSize = 10,
): string | null {
  if (!order.length) return null;
  const index = current ? order.indexOf(current) : -1;
  const clamp = (value: number) => Math.max(0, Math.min(order.length - 1, value));
  switch (move) {
    case "next":
      return order[index === -1 ? 0 : clamp(index + 1)];
    case "previous":
      return order[index === -1 ? order.length - 1 : clamp(index - 1)];
    case "first":
      return order[0];
    case "last":
      return order[order.length - 1];
    case "pageDown":
      return order[clamp((index === -1 ? 0 : index) + pageSize)];
    case "pageUp":
      return order[clamp((index === -1 ? 0 : index) - pageSize)];
  }
}

/** 删除/移动后去掉已不存在的 key；焦点丢失时落到原位置附近的项。 */
export function pruneSelection(state: SelectionState, order: readonly string[], previousOrder: readonly string[]): SelectionState {
  const alive = new Set(order);
  const selected = state.selected.filter((key) => alive.has(key));
  let focus = state.focus && alive.has(state.focus) ? state.focus : null;
  if (!focus && state.focus) {
    const at = previousOrder.indexOf(state.focus);
    for (let index = at; index >= 0 && !focus; index -= 1) {
      if (alive.has(previousOrder[index])) focus = previousOrder[index];
    }
    focus ??= order[0] ?? null;
  }
  const anchor = state.anchor && alive.has(state.anchor) ? state.anchor : focus;
  return { selected, focus, anchor };
}

/** 跟 VS Code `distinctParents` 一样：父文件夹已选中时去掉它下面的子项。 */
export function distinctParents<T extends { workspaceId: string; path: string }>(items: readonly T[]): T[] {
  return items.filter(
    (item) =>
      !items.some(
        (other) =>
          other !== item &&
          other.workspaceId === item.workspaceId &&
          (other.path === "" ? item.path !== "" : item.path.startsWith(`${other.path}/`)),
      ),
  );
}

/** 按“键入即定位”：从焦点后面开始找第一个以前缀开头的名字，循环查找。 */
export function typeNavigate(
  order: readonly string[],
  names: ReadonlyMap<string, string>,
  current: string | null,
  prefix: string,
): string | null {
  if (!prefix || !order.length) return null;
  const lower = prefix.toLowerCase();
  const start = current ? order.indexOf(current) : -1;
  const sameChar = lower.length > 1 && [...lower].every((char) => char === lower[0]);
  const step = sameChar ? 1 : 0;
  for (let offset = step; offset < order.length + step; offset += 1) {
    const key = order[(start + offset + order.length) % order.length];
    const name = names.get(key)?.toLowerCase() ?? "";
    if (sameChar ? name.startsWith(lower[0]) : name.startsWith(lower)) return key;
  }
  return null;
}
