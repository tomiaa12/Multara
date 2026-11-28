import { readStorage, removeStorage, writeStorage } from "../utils/safe-storage";

const REMOTE_HOST_KEY = "multara.remote-host";

/** 当前界面操作的主机；整页生命周期内不变，切换主机时重载页面，让所有 store 按新主机重新初始化。 */
const currentHostId = readStorage(REMOTE_HOST_KEY) || null;

export function remoteHostId(): string | null {
  return currentHostId;
}

export function switchHost(hostId: string | null) {
  if (hostId === currentHostId) return;
  if (hostId) writeStorage(REMOTE_HOST_KEY, hostId);
  else removeStorage(REMOTE_HOST_KEY);
  window.location.reload();
}
