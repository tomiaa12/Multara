// 界面块的局部状态（表格排序、表单草稿、图表是否展开数据表）。
// 正文继续流式增长、长会话滚动导致组件重建时，从这里恢复，不丢用户刚填的东西。
const MAX_ENTRIES = 500;
const cache = new Map<string, Record<string, unknown>>();

/** key：消息 id / 块序号 / type */
export function genUiStateKey(messageId: string, index: number, type: string, sub = 0) {
  return `${messageId}/${index}${sub ? `.${sub}` : ""}/${type}`;
}

export function readGenUiState<T>(key: string, field: string): T | undefined {
  const entry = cache.get(key);
  if (!entry) return undefined;
  // 读也算最近使用
  cache.delete(key);
  cache.set(key, entry);
  return entry[field] as T | undefined;
}

export function writeGenUiState(key: string, field: string, value: unknown) {
  const entry = cache.get(key) ?? {};
  entry[field] = value;
  cache.delete(key);
  cache.set(key, entry);
  while (cache.size > MAX_ENTRIES) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
}

export function clearGenUiStateForTest() {
  cache.clear();
}
