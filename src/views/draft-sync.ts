function sameField(left: unknown, right: unknown) {
  if (Array.isArray(left) && Array.isArray(right)) {
    return left.length === right.length && left.every((item, index) => Object.is(item, right[index]));
  }
  return Object.is(left, right);
}

function copyField<V>(value: V): V {
  return (Array.isArray(value) ? [...value] : value) as V;
}

/**
 * 把最新的已保存对象同步进编辑草稿：只覆盖用户没改过的字段（草稿值仍等于上次同步的基线），
 * `reset` 为 true（切换到另一个对象）时整份覆盖。基线随之更新为最新值。
 */
export function syncDraft<T extends object>(draft: T, baseline: T, next: T, reset: boolean) {
  for (const key of Object.keys(next) as Array<keyof T>) {
    if (reset || sameField(draft[key], baseline[key])) draft[key] = copyField(next[key]);
    baseline[key] = copyField(next[key]);
  }
}
