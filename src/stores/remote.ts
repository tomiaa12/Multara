import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import type { RemoteConnState, RemoteHello, RemoteHost, RemoteHostConnStatus, RemotePage, RemotePerm } from "../domain/remote";
import { permLabel, roleLabel } from "../components/remote/remote-labels";
import { showToast } from "../components/ui/toast";
import { onAppEvent } from "../dev/dev-events";
import { t } from "../i18n";
import { notifyDesktop } from "../utils/desktop-notify";
import { inTauri } from "../services/dev-backend";
import * as remote from "../services/remote-backend";
import { remoteHostId } from "../services/remote-target";
import { useAppStore } from "./app";

const ONLINE: RemoteConnState = "online";
const PENDING = "pending";
/** 主机端：这些本机事件变化时重新统计待处理数。 */
const HOST_PENDING_EVENTS = [
  "remote-approvals-changed",
  "remote-requests-changed",
  "remote-pairing-request",
  "remote-pairing-changed",
];

export const useRemoteStore = defineStore("remote", () => {
  const hostId = remoteHostId();
  const isRemote = Boolean(hostId);
  const hosts = ref<RemoteHost[]>([]);
  const me = ref<RemoteHello | null>(null);
  /** 主机端待处理的审批、权限申请、配对确认总数。 */
  const pendingCount = ref(0);

  const currentHost = computed(() => hosts.value.find((host) => host.id === hostId) ?? null);
  const connState = computed<RemoteConnState>(() => currentHost.value?.status?.state ?? "connecting");

  /** 申请权限弹窗：`null` 关闭；字符串为预选的权限（空串为未指定）。 */
  const requesting = ref<string | null>(null);

  /** 只用于界面提示；真正的校验在主机端。 */
  function can(perm: RemotePerm) {
    return !isRemote || Boolean(me.value?.perms.includes(perm));
  }

  /** 主机配置类页面（工作流、自动化、统计）是否对当前分组开放；开放后也只读。 */
  function pageVisible(page: RemotePage) {
    return !isRemote || Boolean(me.value?.group.pages?.includes(page));
  }

  /** 远程模式下按钮是否要锁住：断线时写操作一律锁，角色不足时锁对应权限。 */
  function locked(perm: RemotePerm) {
    return isRemote && (connState.value !== ONLINE || !can(perm));
  }

  function lockTip(perm: RemotePerm) {
    if (!locked(perm)) return "";
    if (connState.value !== ONLINE) return t("remote.lock.offline");
    const group = me.value?.group;
    return t("remote.lock.need", { perm: permLabel(perm), group: group?.name ?? "", role: roleLabel(group?.role) });
  }

  function openRequest(perm = "") {
    requesting.value = perm;
  }

  async function loadHosts() {
    hosts.value = await remote.listHosts().catch(() => hosts.value);
  }

  async function loadMe() {
    if (!hostId) return;
    me.value = await remote.hello(hostId).catch(() => me.value);
  }

  /** 上次提醒时的审批 + 申请数；首次加载只记录不提醒。 */
  let notifiedCount = -1;
  async function loadPending() {
    if (isRemote) return;
    const [approvals, requests, pairings] = await Promise.all([
      remote.listApprovals(true).catch(() => []),
      remote.listRequests().catch(() => []),
      remote.pendingPairings().catch(() => []),
    ]);
    const actionable = approvals.length + requests.filter((item) => item.status === PENDING).length;
    if (notifiedCount >= 0 && actionable > notifiedCount) {
      showToast(t("remote.pending.notify"), "warning");
      notifyDesktop(t("remote.title"), t("remote.pending.notify"));
    }
    notifiedCount = actionable;
    pendingCount.value = actionable + pairings.length;
  }

  let wasOnline = false;
  function onHostStatus(status: RemoteHostConnStatus | null) {
    const host = hosts.value.find((item) => item.id === status?.hostId);
    if (!host || !status) return;
    host.status = status;
    if (status.hostId !== hostId || status.state !== ONLINE) return;
    // 断线期间主机上的变化用现有加载逻辑整页补齐。
    if (wasOnline) window.location.reload();
    wasOnline = true;
    void loadMe();
  }

  let started = false;
  async function start() {
    if (started || !inTauri()) return;
    started = true;
    await Promise.all([loadHosts(), loadMe(), loadPending()]);
    wasOnline = connState.value === ONLINE;
    void listen<RemoteHostConnStatus>("remote-host-status", ({ payload }) => onHostStatus(payload));
    if (isRemote) {
      void onAppEvent("remote-permissions-changed", () => window.location.reload());
      void onAppEvent("remote-data-changed", () => {
        void useAppStore().syncFromHost().catch((cause) => console.error("[remote] sync failed", cause));
      });
      return;
    }
    for (const event of HOST_PENDING_EVENTS) void listen(event, () => void loadPending());
  }

  return {
    hostId,
    isRemote,
    hosts,
    me,
    pendingCount,
    currentHost,
    connState,
    requesting,
    can,
    pageVisible,
    locked,
    lockTip,
    openRequest,
    loadHosts,
    loadMe,
    loadPending,
    start,
  };
});
