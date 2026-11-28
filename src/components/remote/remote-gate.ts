import type { RemotePerm } from "../../domain/remote";
import type { ContextMenuItem } from "../ui/ContextMenu.vue";

/** 菜单项 id → 所需权限；`null` 表示远程永不可用（会在主机上打开窗口、绑定目录等），直接隐藏。 */
export type MenuGate = Record<string, RemotePerm | null>;

type RemoteGateStore = {
  isRemote: boolean;
  locked: (perm: RemotePerm) => boolean;
};

const SEPARATOR = "sep";

function dropStraySeparators(items: ContextMenuItem[]) {
  return items.filter((item, index) => {
    if (item.type !== SEPARATOR) return true;
    const previous = items[index - 1];
    return index > 0 && index < items.length - 1 && previous?.type !== SEPARATOR;
  });
}

/** 远程模式下按权限隐藏 / 禁用菜单项；本机原样返回。 */
export function gateMenuItems(items: ContextMenuItem[], gate: MenuGate, remote: RemoteGateStore) {
  if (!remote.isRemote) return items;
  const visible = items
    .filter((item) => !item.id || gate[item.id] !== null)
    .map((item) => {
      const perm = item.id ? gate[item.id] : undefined;
      return perm && remote.locked(perm) ? { ...item, disabled: true } : item;
    });
  return dropStraySeparators(visible);
}
