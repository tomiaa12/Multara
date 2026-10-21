import { defineStore } from "pinia";
import { ref } from "vue";
import {
  checkForUpdate,
  installPendingUpdate,
  prepareUpdateOnLaunch,
  setUpdateReadyListener,
  type AppUpdate,
} from "../services/update";

const STARTUP_CHECK_DELAY_MS = 5_000;
const PERIODIC_CHECK_INTERVAL_MS = 60 * 60 * 1000;

export const useUpdaterStore = defineStore("updater", () => {
  const readyVersion = ref("");
  const installing = ref(false);
  const dismissed = ref(false);
  const lastChecked = ref<AppUpdate | null>(null);

  let automaticUpdates = true;
  let startupTimer = 0;
  let periodicTimer = 0;
  let startupCheckElapsed = false;

  function showReady(version: string) {
    readyVersion.value = version;
    dismissed.value = false;
    if (lastChecked.value?.version === version) {
      lastChecked.value = { version, ready: true };
    } else {
      lastChecked.value = { version, ready: true };
    }
  }

  setUpdateReadyListener((version) => {
    showReady(version);
  });

  function cancelBackgroundChecks() {
    window.clearTimeout(startupTimer);
    window.clearInterval(periodicTimer);
    startupTimer = 0;
    periodicTimer = 0;
  }

  async function runAutomaticCheck() {
    if (!automaticUpdates) return;
    try {
      await checkForUpdate();
    } catch {
      /* 开发模式或未发布时忽略 */
    }
  }

  function scheduleBackgroundChecks() {
    if (!startupCheckElapsed && !startupTimer) {
      startupTimer = window.setTimeout(() => {
        startupTimer = 0;
        startupCheckElapsed = true;
        void runAutomaticCheck();
      }, STARTUP_CHECK_DELAY_MS);
    }
    if (!periodicTimer) {
      periodicTimer = window.setInterval(() => {
        void runAutomaticCheck();
      }, PERIODIC_CHECK_INTERVAL_MS);
    }
  }

  /** 启动：有待装包则立刻安装；否则延迟后台检查（不挡启动） */
  async function boot(autoUpdateEnabled: boolean) {
    automaticUpdates = autoUpdateEnabled;
    let hasPending = false;
    try {
      hasPending = Boolean(localStorage.getItem("multara.pendingUpdateVersion"));
    } catch {
      hasPending = false;
    }
    if (hasPending) {
      try {
        const update = await prepareUpdateOnLaunch(autoUpdateEnabled);
        if (update?.kind === "installing") {
          installing.value = true;
          await installPendingUpdate();
          return;
        }
        if (update?.ready) showReady(update.version);
      } catch {
        /* 开发模式或未发布时忽略 */
      }
    }
    if (autoUpdateEnabled) scheduleBackgroundChecks();
    else cancelBackgroundChecks();
  }

  function setAutomaticUpdates(enabled: boolean) {
    const wasEnabled = automaticUpdates;
    automaticUpdates = enabled;
    if (!enabled) {
      cancelBackgroundChecks();
      return;
    }
    if (!wasEnabled) {
      if (startupCheckElapsed) void runAutomaticCheck();
      scheduleBackgroundChecks();
    }
  }

  async function checkNow(): Promise<AppUpdate | null> {
    const next = await checkForUpdate();
    lastChecked.value = next;
    if (next?.ready) showReady(next.version);
    return next;
  }

  async function installNow(onProgress?: (progress: { percent: number; total: number }) => void) {
    installing.value = true;
    try {
      await installPendingUpdate(onProgress);
    } catch (error) {
      installing.value = false;
      throw error;
    }
  }

  function dismiss() {
    dismissed.value = true;
  }

  return {
    readyVersion,
    installing,
    dismissed,
    lastChecked,
    boot,
    setAutomaticUpdates,
    checkNow,
    installNow,
    dismiss,
  };
});
