import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type CheckOptions, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";

const PENDING_KEY = "multara.pendingUpdateVersion";

/** 仅用于 updater 的 check/download；空字符串表示走系统代理（reqwest system-proxy）。 */
let updateProxyUrl = "";

export function setUpdateProxy(proxy: string): void {
  updateProxyUrl = proxy.trim();
}

export function getUpdateProxy(): string {
  return updateProxyUrl;
}

function checkOptions(): CheckOptions | undefined {
  if (!updateProxyUrl) return undefined;
  return { proxy: updateProxyUrl };
}

export type AppUpdate = {
  version: string;
  /** 已后台下载完成，下次启动会自动安装 */
  ready: boolean;
};

export type UpdateProgress = {
  downloaded: number;
  total: number;
  percent: number;
};

export type LaunchUpdateResult =
  | { kind: "installing"; version: string }
  | { kind: "available"; version: string; ready: boolean };

let pending: Update | null = null;
let pendingDownloaded = false;
let backgroundDownload: Promise<void> | null = null;
let inFlightCheck: Promise<Update | null> | null = null;
let readyListener: ((version: string) => void) | null = null;
let progressListener: ((progress: UpdateProgress | null) => void) | null = null;

function inTauri() {
  return "__TAURI_INTERNALS__" in window;
}

export async function currentVersion(): Promise<string> {
  if (!inTauri()) return "0.1.0";
  return getVersion();
}

function readPendingVersion(): string {
  try {
    return localStorage.getItem(PENDING_KEY) ?? "";
  } catch {
    return "";
  }
}

function writePendingVersion(version: string): void {
  try {
    localStorage.setItem(PENDING_KEY, version);
  } catch {
    /* ignore quota / private mode */
  }
}

function clearPendingVersion(): void {
  try {
    localStorage.removeItem(PENDING_KEY);
  } catch {
    /* ignore */
  }
}

function toAppUpdate(update: Update): AppUpdate {
  const ready = pendingDownloaded || readPendingVersion() === update.version;
  return { version: update.version, ready };
}

export function setUpdateReadyListener(listener: ((version: string) => void) | null): void {
  readyListener = listener;
}

export function setUpdateProgressListener(
  listener: ((progress: UpdateProgress | null) => void) | null,
): void {
  progressListener = listener;
}

function emitProgress(progress: UpdateProgress | null): void {
  progressListener?.(progress);
}

/** 累计 DownloadEvent，同时推给全局 listener 与可选回调。 */
function createProgressTracker(onProgress?: (progress: UpdateProgress) => void) {
  let downloaded = 0;
  let total = 0;
  return (event: DownloadEvent) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
      downloaded = 0;
      const progress = { downloaded, total, percent: 0 };
      emitProgress(progress);
      onProgress?.(progress);
      return;
    }
    if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      const percent = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
      const progress = { downloaded, total, percent };
      emitProgress(progress);
      onProgress?.(progress);
      return;
    }
    if (event.event === "Finished") {
      const progress = { downloaded: total || downloaded, total, percent: total > 0 ? 100 : 0 };
      emitProgress(progress);
      onProgress?.(progress);
    }
  };
}

function startBackgroundDownload(update: Update): void {
  if (pendingDownloaded || readPendingVersion() === update.version) {
    pendingDownloaded = true;
    emitProgress(null);
    readyListener?.(update.version);
    return;
  }
  if (backgroundDownload) return;
  const onChunk = createProgressTracker();
  backgroundDownload = update
    .download((event) => onChunk(event))
    .then(() => {
      pendingDownloaded = true;
      writePendingVersion(update.version);
      emitProgress(null);
      readyListener?.(update.version);
    })
    .catch((error) => {
      emitProgress(null);
      console.error(error);
    })
    .finally(() => {
      backgroundDownload = null;
    });
}

async function checkOnce(): Promise<Update | null> {
  // updater 会把可执行文件所在目录当成安装目录，dev 下就是 target/debug，安装会整个替换掉编译缓存
  if (import.meta.env.DEV) return null;
  if (inFlightCheck) return inFlightCheck;
  const task = check(checkOptions())
    .catch((error) => {
      console.error(error);
      throw error;
    })
    .finally(() => {
      if (inFlightCheck === task) inFlightCheck = null;
    });
  inFlightCheck = task;
  return task;
}

export async function checkForUpdate(): Promise<AppUpdate | null> {
  if (!inTauri()) return null;
  try {
    if (pending && (pendingDownloaded || backgroundDownload)) {
      return toAppUpdate(pending);
    }
    const next = await checkOnce();
    pending = next;
    pendingDownloaded = false;
    if (!next) {
      clearPendingVersion();
      return null;
    }
    startBackgroundDownload(next);
    return toAppUpdate(next);
  } catch (error) {
    pending = null;
    pendingDownloaded = false;
    throw error;
  }
}

/**
 * 启动时：已标记待装的版本自动安装。
 * `autoDownload` 为真时才后台预下载新版本；关闭自动更新时仍会安装已下载包。
 */
export async function prepareUpdateOnLaunch(
  autoDownload = true,
): Promise<LaunchUpdateResult | null> {
  if (!inTauri()) return null;
  const update = await checkOnce();
  pending = update;
  pendingDownloaded = false;
  if (!update) {
    clearPendingVersion();
    return null;
  }
  if (readPendingVersion() === update.version) {
    return { kind: "installing", version: update.version };
  }
  if (!autoDownload) return null;
  startBackgroundDownload(update);
  return {
    kind: "available",
    version: update.version,
    ready: false,
  };
}

export async function installPendingUpdate(
  onProgress?: (progress: UpdateProgress) => void,
): Promise<void> {
  const target = pending ?? (await checkOnce());
  if (!target) throw new Error("没有可用更新");
  pending = target;
  if (pendingDownloaded) {
    await target.install();
  } else {
    await target.downloadAndInstall(createProgressTracker(onProgress));
  }
  pendingDownloaded = false;
  clearPendingVersion();
  emitProgress(null);
  await relaunch();
}

export async function installUpdate(
  _update: AppUpdate,
  onProgress?: (progress: UpdateProgress) => void,
): Promise<void> {
  await installPendingUpdate(onProgress);
}
