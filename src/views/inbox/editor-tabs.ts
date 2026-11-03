export function placeEditorTab<T extends { pinned: boolean }>(
  tabs: readonly T[],
  incoming: T,
  dirty: (tab: T) => boolean,
): T[] {
  const pinnedDirty = tabs.map((tab) => (!tab.pinned && dirty(tab) ? { ...tab, pinned: true } : tab));
  const index = pinnedDirty.findIndex((tab) => !tab.pinned);
  if (index < 0 || incoming.pinned) return [...pinnedDirty, incoming];
  return pinnedDirty.map((tab, at) => (at === index ? incoming : tab));
}
