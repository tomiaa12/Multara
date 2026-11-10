// Canvas 实例状态（方案 §5.7）：按会话分组；URL 只在内存，重启后一律作废。
// 同一实例同时只在一个视图里活着（R16）：claims 记录谁在显示它，其它位置显示占位。
import { defineStore } from "pinia";
import { ref, shallowRef } from "vue";
import { listen } from "@tauri-apps/api/event";
import type { CanvasDefinition, CanvasInstance } from "../domain/types";
import * as backend from "../services/backend";
import { fillComposerFromUi, type UiSource } from "../components/chat/ui-outbound";

interface UiOutboundEvent {
  sessionId: string;
  source: UiSource;
  sourceId: string;
  title?: string;
  kind: "message" | "context";
  text?: string | null;
  data?: unknown;
  requestId?: string;
}

export function canvasKey(sessionId: string, instanceId: string) {
  return `${sessionId}/${instanceId}`;
}

/** 后台（切走后）最多保留几个 Canvas 页面继续连着 */
export const MAX_BACKGROUND_FRAMES = 3;
/** 后台页面隐藏超过这么久就卸载；下次显示重新加载，数据在 storage 里不丢 */
export const BACKGROUND_TTL_MS = 10 * 60 * 1000;

export interface LiveFrame {
  key: string;
  sessionId: string;
  instanceId: string;
  title: string;
  url: string;
  /** 变了就整页重载（刷新按钮、重连） */
  frameKey: number;
  /** 加载时地址上带的主题 */
  theme: string;
  loadedFrameKey: number;
  /** 开始隐藏的时间；正在显示为 0 */
  hiddenSince: number;
}

/** 页面挂上来的上下文：下一轮随用户消息带上，同一来源只保留最新一份（§6） */
export interface UiContextItem {
  id: string;
  sessionId: string;
  source: "canvas" | "mcp-app";
  sourceId: string;
  label: string;
  text: string | null;
  data: unknown;
}

function inTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const useCanvasStore = defineStore("canvas", () => {
  const instances = ref<Record<string, CanvasInstance[]>>({});
  const available = ref<Record<string, CanvasDefinition[]>>({});
  const enabled = ref<Record<string, boolean>>({});
  const urls = ref<Record<string, string>>({});
  /** 实例 → 当前认领它的视图 id */
  const claims = ref<Record<string, string>>({});
  /** 请求在某处显示一个实例（pill、「添加上下文 → Canvas...」） */
  const showRequest = ref<{ sessionId: string; instanceId: string; nonce: number } | null>(null);
  /** 能显示 Canvas 的宿主（工作台）个数；为 0 时由聊天面板弹窗显示 */
  const hosts = ref(0);
  const uiContexts = ref<UiContextItem[]>([]);
  let listening = false;

  function listFor(sessionId: string | null | undefined) {
    return sessionId ? instances.value[sessionId] ?? [] : [];
  }

  /** 这个会话的列表有没有成功拉回来过；没拉回来之前，界面不要据此删任何东西 */
  function isLoaded(sessionId: string | null | undefined) {
    return Boolean(sessionId && sessionId in instances.value);
  }

  const retryTimers = new Map<string, ReturnType<typeof setTimeout>>();

  async function refresh(sessionId: string, attempt = 0) {
    const listing = await backend.listCanvasInstances(sessionId).catch(() => null);
    if (!listing) {
      // 刚启动时后端可能还没就绪：退避重试几次，拉到了就展示
      if (attempt < 5 && !retryTimers.has(sessionId)) {
        const timer = setTimeout(() => {
          retryTimers.delete(sessionId);
          void refresh(sessionId, attempt + 1);
        }, 500 * 2 ** attempt);
        retryTimers.set(sessionId, timer);
      }
      return;
    }
    // 设置晚到时后端回 enabled=false + 空列表：别把它当成「已加载、没有 Canvas」
    if (!listing.enabled) {
      enabled.value = { ...enabled.value, [sessionId]: false };
      return;
    }
    enabled.value = { ...enabled.value, [sessionId]: listing.enabled };
    instances.value = { ...instances.value, [sessionId]: listing.instances };
    available.value = { ...available.value, [sessionId]: listing.available };
    // 已关闭的实例：丢掉 URL 和认领
    const alive = new Set(listing.instances.map((item) => canvasKey(sessionId, item.instanceId)));
    for (const key of Object.keys(urls.value)) {
      if (key.startsWith(`${sessionId}/`) && !alive.has(key)) delete urls.value[key];
    }
    for (const key of Object.keys(claims.value)) {
      if (key.startsWith(`${sessionId}/`) && !alive.has(key)) delete claims.value[key];
    }
    for (const key of Object.keys(live.value)) {
      if (key.startsWith(`${sessionId}/`) && !alive.has(key)) dropLive(key);
    }
  }

  function startListening() {
    if (listening || !inTauri()) return;
    listening = true;
    void listen<{ sessionId: string; pinned?: boolean }>("canvas-changed", (event) => {
      // 固定状态变了：所有已加载的会话列表都可能变
      const sessions = event.payload.pinned ? Object.keys(instances.value) : [];
      for (const sessionId of new Set([event.payload.sessionId, ...sessions])) void refresh(sessionId);
    });
    // 页面经 __multara/message / __multara/context 发来的东西（canvas_bridge.rs）
    void listen<UiOutboundEvent>("ui-outbound", (event) => {
      const payload = event.payload;
      if (payload.kind === "message") {
        const result = fillComposerFromUi({
          sessionId: payload.sessionId,
          source: payload.source,
          sourceId: payload.sourceId,
          text: payload.text ?? "",
        });
        if (payload.requestId) void backend.resolveUiOutbound(payload.requestId, result);
        return;
      }
      attachContext({
        sessionId: payload.sessionId,
        source: payload.source,
        sourceId: payload.sourceId,
        label: payload.title || payload.sourceId,
        text: payload.text ?? null,
        data: payload.data ?? null,
      });
    });
  }

  async function ensureUrl(sessionId: string, instanceId: string, force = false) {
    const key = canvasKey(sessionId, instanceId);
    if (!force && urls.value[key]) return urls.value[key];
    const url = await backend.ensureCanvasHosted(sessionId, instanceId);
    urls.value = { ...urls.value, [key]: url };
    return url;
  }

  /** 探测服务是否还在；不在时清掉缓存的 URL */
  async function probe(sessionId: string, instanceId: string) {
    const key = canvasKey(sessionId, instanceId);
    const url = await backend.canvasHostedUrl(sessionId, instanceId).catch(() => null);
    if (!url) {
      const next = { ...urls.value };
      delete next[key];
      urls.value = next;
    }
    return url;
  }

  function claim(sessionId: string, instanceId: string, viewId: string) {
    claims.value = { ...claims.value, [canvasKey(sessionId, instanceId)]: viewId };
  }

  function release(sessionId: string, instanceId: string, viewId: string) {
    const key = canvasKey(sessionId, instanceId);
    if (claims.value[key] !== viewId) return;
    const next = { ...claims.value };
    delete next[key];
    claims.value = next;
  }

  function claimedBy(sessionId: string, instanceId: string) {
    return claims.value[canvasKey(sessionId, instanceId)] ?? null;
  }

  async function setVisible(sessionId: string, instanceId: string, visible: boolean) {
    const list = instances.value[sessionId];
    const item = list?.find((entry) => entry.instanceId === instanceId);
    if (item) item.visible = visible;
    await backend.setCanvasVisible(sessionId, instanceId, visible);
  }

  async function setPinned(sessionId: string, instanceId: string, pinned: boolean) {
    for (const list of Object.values(instances.value)) {
      const item = list.find((entry) => entry.sessionId === sessionId && entry.instanceId === instanceId);
      if (item) item.pinned = pinned;
    }
    await backend.setCanvasPinned(sessionId, instanceId, pinned);
  }

  /** 这个会话的视图能不能显示某个实例（本会话的，或别处固定过来的） */
  function shownIn(viewSessionId: string | null | undefined, sessionId: string, instanceId: string) {
    return listFor(viewSessionId).some((item) => item.sessionId === sessionId && item.instanceId === instanceId);
  }

  /** 显示一个实例：标成可见，交给工作台（或聊天面板的弹窗）去显示 */
  async function show(sessionId: string, instanceId: string) {
    await setVisible(sessionId, instanceId, true).catch(() => undefined);
    showRequest.value = { sessionId, instanceId, nonce: Date.now() };
  }

  /** 「关闭 Canvas」：结束实例（关标签只调 setVisible(false)） */
  async function close(sessionId: string, instanceId: string) {
    await backend.closeCanvas(sessionId, instanceId);
    const key = canvasKey(sessionId, instanceId);
    const nextUrls = { ...urls.value };
    delete nextUrls[key];
    urls.value = nextUrls;
    dropLive(key);
    // 固定的实例可能出现在多个会话的列表里，全部去掉
    instances.value = Object.fromEntries(
      Object.entries(instances.value).map(([key, list]) => [
        key,
        list.filter((item) => !(item.sessionId === sessionId && item.instanceId === instanceId)),
      ]),
    );
    uiContexts.value = uiContexts.value.filter((item) => !(item.sessionId === sessionId && item.sourceId === instanceId));
  }

  /** 打开已注册的定义（instanceId = canvasId），然后显示 */
  async function openRegistered(sessionId: string, canvasId: string) {
    const instance = await backend.openRegisteredCanvas(sessionId, canvasId);
    await refresh(sessionId);
    await show(sessionId, instance.instanceId);
  }

  function registerHost() {
    hosts.value += 1;
    return () => {
      hosts.value = Math.max(0, hosts.value - 1);
    };
  }

  // ---------- 活页面（全局 iframe 层） ----------
  // keep-alive 把隐藏的页面挪进脱离文档的容器，iframe 一脱离就会卸载、断开连接。
  // 所以 Canvas 的 iframe 统一放在 keep-alive 外面的 CanvasFrameHost 里，按显示位置（slot）定位：
  // 切走后页面在后台继续连着，智能体的 canvas action 照样能执行。
  // 后台页面有上限：最多 MAX_BACKGROUND_FRAMES 个、隐藏超过 BACKGROUND_TTL_MS 就卸载，防止开很多需求时内存上涨。

  /** 显示位置：viewId → 是否在前台（keep-alive 停用时为 false） */
  const viewActive = ref<Record<string, boolean>>({});
  /** 显示位置的占位元素，iframe 定位到它上面 */
  const slots = shallowRef(new Map<string, HTMLElement>());
  const live = ref<Record<string, LiveFrame>>({});
  /** 页面状态由 UiFrame 决定：重连 / 不可用 / 在别处显示时 iframe 要藏起来，让外壳的提示露出来 */
  const veiled = ref<Record<string, boolean>>({});

  function setViewActive(viewId: string, active: boolean | null) {
    const next = { ...viewActive.value };
    if (active === null) delete next[viewId];
    else next[viewId] = active;
    viewActive.value = next;
  }

  function isViewActive(viewId: string | null | undefined) {
    return Boolean(viewId && viewActive.value[viewId]);
  }

  function registerSlot(viewId: string, el: HTMLElement | null) {
    const next = new Map(slots.value);
    if (el) next.set(viewId, el);
    else next.delete(viewId);
    slots.value = next;
  }

  function setLive(sessionId: string, instanceId: string, patch: { url: string; title: string; frameKey: number; theme: string }) {
    const key = canvasKey(sessionId, instanceId);
    const previous = live.value[key];
    live.value = {
      ...live.value,
      [key]: {
        key,
        sessionId,
        instanceId,
        title: patch.title,
        url: patch.url,
        frameKey: patch.frameKey,
        // 地址上的主题只在重新加载时更新，切主题本身走 postMessage，不整页重载
        theme: previous && previous.frameKey === patch.frameKey && previous.url === patch.url ? previous.theme : patch.theme,
        loadedFrameKey: previous?.frameKey === patch.frameKey && previous.url === patch.url ? previous.loadedFrameKey : -1,
        hiddenSince: previous?.hiddenSince ?? 0,
      },
    };
    // 只按个数修剪；按时长的修剪由 CanvasFrameHost 定时调 pruneLive()
    pruneLive(undefined, { ttl: false });
  }

  function markLoaded(key: string, frameKey: number) {
    const entry = live.value[key];
    if (!entry || entry.frameKey !== frameKey) return;
    live.value = { ...live.value, [key]: { ...entry, loadedFrameKey: frameKey } };
  }

  function dropLive(key: string) {
    if (!live.value[key]) return;
    const next = { ...live.value };
    delete next[key];
    live.value = next;
  }

  function setVeiled(key: string, value: boolean) {
    if (Boolean(veiled.value[key]) === value) return;
    veiled.value = { ...veiled.value, [key]: value };
  }

  /** 这个活页面当前该显示在哪个 slot 上；后台时返回 null */
  function visibleSlot(entry: LiveFrame): HTMLElement | null {
    const owner = claims.value[entry.key];
    if (!owner || !isViewActive(owner) || veiled.value[entry.key]) return null;
    return slots.value.get(owner) ?? null;
  }

  /** 后台预算：超过上限先卸最久没显示的；隐藏太久的也卸掉 */
  function pruneLive(now = Date.now(), options: { ttl?: boolean } = {}) {
    const useTtl = options.ttl !== false;
    const entries = Object.values(live.value);
    let changed = false;
    const next = { ...live.value };
    const background: LiveFrame[] = [];
    for (const entry of entries) {
      const shown = Boolean(visibleSlot(entry)) || isViewActive(claims.value[entry.key]);
      if (shown) {
        if (entry.hiddenSince) {
          next[entry.key] = { ...entry, hiddenSince: 0 };
          changed = true;
        }
        continue;
      }
      const hiddenSince = entry.hiddenSince || now;
      if (!entry.hiddenSince) {
        next[entry.key] = { ...entry, hiddenSince };
        changed = true;
      }
      if (useTtl && now - hiddenSince > BACKGROUND_TTL_MS) {
        delete next[entry.key];
        changed = true;
        continue;
      }
      background.push({ ...entry, hiddenSince });
    }
    background.sort((a, b) => b.hiddenSince - a.hiddenSince);
    for (const entry of background.slice(MAX_BACKGROUND_FRAMES)) {
      delete next[entry.key];
      changed = true;
    }
    if (changed) live.value = next;
  }

  // ---------- 页面挂上来的上下文 ----------

  function attachContext(item: Omit<UiContextItem, "id">) {
    // 同一来源整体替换旧的（照 VS Code mcpui-context-<hash>- 前缀替换）
    const id = `ui-context-${item.source}-${item.sourceId}`;
    uiContexts.value = [
      ...uiContexts.value.filter((entry) => !(entry.sessionId === item.sessionId && entry.id === id)),
      { ...item, id },
    ];
  }

  function contextsFor(sessionId: string | null | undefined) {
    return sessionId ? uiContexts.value.filter((item) => item.sessionId === sessionId) : [];
  }

  function removeContext(sessionId: string, id: string) {
    uiContexts.value = uiContexts.value.filter((item) => !(item.sessionId === sessionId && item.id === id));
  }

  /** 发送时取走这个会话的上下文 */
  function takeContexts(sessionId: string) {
    const taken = contextsFor(sessionId);
    uiContexts.value = uiContexts.value.filter((item) => item.sessionId !== sessionId);
    return taken;
  }

  return {
    instances,
    available,
    enabled,
    urls,
    claims,
    showRequest,
    hosts,
    uiContexts,
    listFor,
    isLoaded,
    refresh,
    startListening,
    ensureUrl,
    probe,
    claim,
    release,
    claimedBy,
    setVisible,
    setPinned,
    shownIn,
    show,
    close,
    openRegistered,
    registerHost,
    viewActive,
    slots,
    live,
    veiled,
    setViewActive,
    isViewActive,
    registerSlot,
    setLive,
    markLoaded,
    dropLive,
    setVeiled,
    visibleSlot,
    pruneLive,
    attachContext,
    contextsFor,
    removeContext,
    takeContexts,
  };
});
