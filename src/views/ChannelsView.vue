<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { AlertTriangle, Check, ChevronDown, Copy, MessagesSquare, Pencil, Plus, RefreshCw, Trash2, X } from "@lucide/vue";
import type { ChannelInstallation, ChannelInstallationInput, ChannelStatus, ChannelType, ChannelUser } from "../domain/types";
import { channelSourceLabel } from "../components/chat/message-speaker";
import { confirmDialog } from "../components/ui/confirm-dialog";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import * as backend from "../services/backend";
import UiButton from "../components/ui/UiButton.vue";
import UiInput from "../components/ui/UiInput.vue";
import UiModal from "../components/ui/UiModal.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import UiRelativeTime from "../components/ui/UiRelativeTime.vue";
import AgentAvatar from "../components/AgentAvatar.vue";
import ChatTargetSelect from "../components/ChatTargetSelect.vue";
import ChannelMark from "../components/channels/ChannelMark.vue";
import ChannelGrantDialog from "../components/channels/ChannelGrantDialog.vue";
import { defaultChannelGrant, normalizeGrant, type ChannelGrant } from "../domain/channel-grant";

const store = useAppStore();
const installations = ref<ChannelInstallation[]>([]);
const users = reactive<Record<string, ChannelUser[]>>({});
const grantEditor = ref<{
  item: ChannelInstallation;
  user: ChannelUser | null;
  grant: ChannelGrant;
  targetValue: string;
} | null>(null);
const usersOpen = ref<Record<string, boolean>>({});

function toggleUsers(id: string) {
  usersOpen.value = { ...usersOpen.value, [id]: !usersOpen.value[id] };
}
const showForm = ref(false);
const step = ref<"pick" | "form">("pick");
const busy = ref(false);
const error = ref("");
const notice = ref("");

const form = reactive({
  id: undefined as string | undefined,
  name: "",
  channelType: "feishu" as ChannelType,
  projectId: "",
  appId: "",
  appSecret: "",
  appToken: "",
  groupEnabled: false,
  enabled: true,
});
const weixin = reactive({
  sessionId: "",
  imageUrl: "",
  status: "",
  code: "",
});
let weixinPoll = 0;

function userTargetValue(user: ChannelUser) {
  return user.targetType && user.targetId ? `${user.targetType}:${user.targetId}` : "";
}

function parseTargetValue(value: string): { targetType: "agent" | "squad" | null; targetId: string | null } {
  if (!value) return { targetType: null, targetId: null };
  const [type, id] = value.split(":");
  if (!id) return { targetType: null, targetId: null };
  return { targetType: type === "squad" ? "squad" : "agent", targetId: id };
}

async function refresh() {
  try {
    installations.value = await backend.listChannelInstallations();
    await Promise.all(installations.value.map(async (item) => {
      try {
        users[item.id] = await backend.listChannelAllowedUsers(item.id);
      } catch {
        users[item.id] = users[item.id] ?? [];
      }
    }));
  } catch (cause) {
    error.value = String(cause);
  }
}

let unlistenPaired: UnlistenFn | null = null;
let unlistenProfile: UnlistenFn | null = null;
let unlistenStatus: UnlistenFn | null = null;
let unmounted = false;

/** 有人配对成功：后端已换新码，同步到卡片上；授权用户弹窗开着就顺便刷新列表。 */
async function onPaired(payload: { installationId: string; channelUserId: string; pairingCode: string }) {
  const item = installations.value.find((entry) => entry.id === payload.installationId);
  if (item) item.pairingCode = payload.pairingCode;
  try {
    users[payload.installationId] = await backend.listChannelAllowedUsers(payload.installationId);
  } catch {
    /* 列表刷新失败不影响换码提示 */
  }
  const name = users[payload.installationId]?.find((user) => user.channelUserId === payload.channelUserId)?.displayName
    || payload.channelUserId;
  notice.value = t("channels.paired", { name, code: payload.pairingCode });
}

onMounted(async () => {
  if (!store.data) await store.initialize();
  await refresh();
  if (!backend.inTauri()) return;
  const unlisten = await listen<{ installationId: string; channelUserId: string; pairingCode: string }>(
    "channel-paired",
    (event) => {
      if (event.payload?.installationId) void onPaired(event.payload);
    },
  );
  const unlistenBot = await listen<{ installationId: string; botName: string; botAvatar: string }>(
    "channel-profile",
    (event) => {
      const item = installations.value.find((entry) => entry.id === event.payload?.installationId);
      if (!item) return;
      item.botName = event.payload.botName || null;
      item.botAvatar = event.payload.botAvatar || null;
    },
  );
  const unlistenState = await listen<{ installationId: string; status: ChannelStatus; lastError?: string | null }>(
    "channel-status",
    (event) => {
      const item = installations.value.find((entry) => entry.id === event.payload?.installationId);
      if (!item || !event.payload?.status) return;
      item.status = event.payload.status;
      item.lastError = event.payload.lastError ?? null;
    },
  );
  if (unmounted) {
    unlisten();
    unlistenBot();
    unlistenState();
  } else {
    unlistenPaired = unlisten;
    unlistenProfile = unlistenBot;
    unlistenStatus = unlistenState;
  }
});

onBeforeUnmount(() => {
  unmounted = true;
  weixinPoll += 1;
  unlistenPaired?.();
  unlistenPaired = null;
  unlistenProfile?.();
  unlistenProfile = null;
  unlistenStatus?.();
  unlistenStatus = null;
});

function identityHint(item: ChannelInstallation) {
  if (item.botAvatar) return "";
  if (item.channelType === "dingtalk") return t("channels.identityDingtalk");
  if (item.channelType === "weixin") return t("channels.identityWeixin");
  return "";
}

function statusLabel(status: ChannelInstallation["status"]) {
  if (status === "connected") return t("channels.connected");
  if (status === "connecting") return t("channels.connecting");
  if (status === "error") return t("channels.error");
  return t("channels.disconnected");
}

function edit(item?: ChannelInstallation) {
  error.value = "";
  notice.value = "";
  form.id = item?.id;
  form.name = item?.name ?? "";
  form.channelType = item?.channelType ?? "feishu";
  form.projectId = item?.projectId ?? "";
  form.appId = item && (item.channelType === "slack" || item.channelType === "telegram" || item.channelType === "weixin")
    ? ""
    : item?.appId ?? "";
  form.appSecret = "";
  form.appToken = "";
  form.groupEnabled = item?.groupEnabled ?? false;
  weixin.sessionId = "";
  weixin.imageUrl = "";
  weixin.status = "";
  weixin.code = "";
  weixinPoll += 1;
  form.enabled = item?.enabled ?? true;
  step.value = item ? "form" : "pick";
  showForm.value = true;
}

function choosePlatform(type: ChannelType) {
  if (form.channelType !== type) {
    form.appId = "";
    form.appSecret = "";
    form.appToken = "";
    form.name = channelSourceLabel(type);
    weixin.sessionId = "";
    weixin.imageUrl = "";
    weixin.status = "";
    weixin.code = "";
    weixinPoll += 1;
  } else if (!form.name.trim()) {
    form.name = channelSourceLabel(type);
  }
  form.channelType = type;
  if (!form.id) form.groupEnabled = type === "qq";
  error.value = "";
  step.value = "form";
}

async function save() {
  busy.value = true;
  error.value = "";
  try {
    const input: ChannelInstallationInput = {
      id: form.id,
      name: form.name.trim(),
      channelType: form.channelType,
      projectId: null,
      appId: form.channelType === "slack" || form.channelType === "telegram" || form.channelType === "weixin" ? "" : form.appId.trim(),
      appSecret: form.channelType === "slack" ? form.appId.trim() || undefined : form.appSecret.trim() || undefined,
      appToken: form.channelType === "slack" ? form.appSecret.trim() || undefined : undefined,
      region: form.channelType === "lark" ? "lark" : "feishu",
      groupEnabled: form.groupEnabled,
      enabled: form.enabled,
    };
    await backend.saveChannelInstallation(input);
    showForm.value = false;
    await refresh();
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  } finally {
    busy.value = false;
  }
}

/** 正在保存的开关，`<id>:group` / `<id>:enabled`；保存返回前禁用，防止连点。 */
const switching = ref(new Set<string>());

async function runSwitch(key: string, action: () => Promise<void>) {
  if (switching.value.has(key)) return;
  switching.value = new Set(switching.value).add(key);
  try {
    await action();
    await refresh();
  } catch (cause) {
    error.value = String(cause);
  } finally {
    const next = new Set(switching.value);
    next.delete(key);
    switching.value = next;
  }
}

function setEnabled(item: ChannelInstallation, enabled: boolean) {
  return runSwitch(`${item.id}:enabled`, () => backend.setChannelEnabled(item.id, enabled));
}

function setGroup(item: ChannelInstallation, enabled: boolean) {
  return runSwitch(`${item.id}:group`, () => backend.setChannelGroupEnabled(item.id, enabled));
}

async function remove(item: ChannelInstallation) {
  if (!(await confirmDialog(t("channels.deleteConfirm", { name: item.name }), { danger: true }))) return;
  try {
    await backend.deleteChannelInstallation(item.id);
    await refresh();
  } catch (cause) {
    error.value = String(cause);
  }
}

async function test(item: ChannelInstallation) {
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    const result = await backend.testChannelInstallation(item.id);
    notice.value = t("channels.testOk", { name: result.name || item.name });
    await refresh();
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  } finally {
    busy.value = false;
  }
}

function openDefaultGrant(item: ChannelInstallation) {
  grantEditor.value = {
    item,
    user: null,
    grant: normalizeGrant(item.channelType, item.defaultGrant ?? defaultChannelGrant(item.channelType)),
    targetValue: item.defaultTargetType && item.defaultTargetId ? `${item.defaultTargetType}:${item.defaultTargetId}` : "",
  };
}

function openUserGrant(item: ChannelInstallation, user: ChannelUser) {
  grantEditor.value = {
    item,
    user,
    grant: normalizeGrant(item.channelType, user.permissions ?? defaultChannelGrant(item.channelType)),
    targetValue: "",
  };
}

async function saveGrant() {
  const current = grantEditor.value;
  if (!current) return;
  try {
    if (current.user) {
      await backend.setChannelUserGrant(current.item.id, current.user.channelUserId, current.grant);
    } else {
      const target = parseTargetValue(current.targetValue);
      await backend.setChannelDefaults(current.item.id, target.targetType, target.targetId, current.grant);
    }
    grantEditor.value = null;
    await refresh();
  } catch (cause) {
    error.value = String(cause);
  }
}

async function setPairing(item: ChannelInstallation, required: boolean) {
  item.pairingRequired = required;
  try {
    await backend.setChannelPairingRequired(item.id, required);
  } catch (cause) {
    item.pairingRequired = !required;
    error.value = String(cause);
  }
}

/** 同一原因（如缺权限）只提示一次。 */
function nameErrors(installationId: string) {
  const messages = (users[installationId] ?? [])
    .filter((user) => !user.displayName && user.nameError)
    .map((user) => user.nameError as string);
  return [...new Set(messages)];
}

async function removeUser(item: ChannelInstallation | null, userId: string) {
  if (!item) return;
  try {
    await backend.removeChannelAllowedUser(item.id, userId);
    users[item.id] = await backend.listChannelAllowedUsers(item.id);
  } catch (cause) {
    error.value = String(cause);
  }
}

async function saveNote(item: ChannelInstallation, user: ChannelUser, event: Event) {
  const note = (event.target as HTMLInputElement).value.trim();
  if (note === (user.note ?? "")) return;
  const previous = user.note;
  user.note = note || null;
  try {
    await backend.setChannelUserNote(item.id, user.channelUserId, note);
  } catch (cause) {
    user.note = previous;
    error.value = String(cause);
  }
}

async function saveUserTarget(item: ChannelInstallation, user: ChannelUser, value: string) {
  const next = parseTargetValue(value);
  const previous = { targetType: user.targetType ?? null, targetId: user.targetId ?? null };
  if ((previous.targetType ?? null) === next.targetType && (previous.targetId ?? null) === next.targetId) return;
  user.targetType = next.targetType;
  user.targetId = next.targetId;
  try {
    await backend.setChannelUserTarget(item.id, user.channelUserId, next.targetType, next.targetId);
  } catch (cause) {
    user.targetType = previous.targetType;
    user.targetId = previous.targetId;
    error.value = String(cause);
  }
}

async function rotateCode(item: ChannelInstallation) {
  try {
    const code = await backend.rotateChannelPairingCode(item.id);
    item.pairingCode = code;
    await navigator.clipboard?.writeText(code).catch(() => {});
  } catch (cause) {
    error.value = String(cause);
  }
}

async function copyCode(item: ChannelInstallation) {
  try {
    await navigator.clipboard?.writeText(item.pairingCode);
    notice.value = t("channels.codeCopied");
  } catch {
    /* 剪贴板不可用时忽略 */
  }
}

const weixinImage = computed(() => weixin.imageUrl.trim());

const pickerPlatforms: ChannelType[] = ["feishu", "slack", "dingtalk", "wecom", "telegram", "weixin", "qq"];

const credential = computed(() => {
  switch (form.channelType) {
    case "slack":
      return {
        showId: true,
        showSecret: true,
        idLabel: t("channels.botToken"),
        secretLabel: t("channels.appToken"),
        idPlaceholder: "xoxb-",
        secretPlaceholder: "xapp-",
        secret: true,
        hint: t("channels.setupHintSlack"),
      };
    case "telegram":
      return {
        showId: false,
        showSecret: true,
        idLabel: "",
        secretLabel: t("channels.botToken"),
        idPlaceholder: "",
        secretPlaceholder: "123456:ABC...",
        secret: true,
        hint: t("channels.setupHintTelegram"),
      };
    case "dingtalk":
      return {
        showId: true,
        showSecret: true,
        idLabel: "AppKey",
        secretLabel: "AppSecret",
        idPlaceholder: "",
        secretPlaceholder: "",
        secret: true,
        hint: t("channels.setupHintDingtalk"),
      };
    case "wecom":
      return {
        showId: true,
        showSecret: true,
        idLabel: "Bot ID",
        secretLabel: "Secret",
        idPlaceholder: "",
        secretPlaceholder: "",
        secret: true,
        hint: t("channels.setupHintWecom"),
      };
    case "qq":
      return {
        showId: true,
        showSecret: true,
        idLabel: "AppID",
        secretLabel: "AppSecret",
        idPlaceholder: "",
        secretPlaceholder: "",
        secret: true,
        hint: t("channels.setupHintQq"),
      };
    case "weixin":
      return {
        showId: false,
        showSecret: false,
        idLabel: "",
        secretLabel: "",
        idPlaceholder: "",
        secretPlaceholder: "",
        secret: true,
        hint: t("channels.setupHintWeixin"),
      };
    default:
      return {
        showId: true,
        showSecret: true,
        idLabel: t("channels.appId"),
        secretLabel: t("channels.appSecret"),
        idPlaceholder: "cli_xxxxxxxx",
        secretPlaceholder: "",
        secret: true,
        hint: t("channels.setupHint"),
      };
  }
});

async function startWeixin() {
  error.value = "";
  const token = ++weixinPoll;
  try {
    const started = await backend.beginWeixinLogin({
      name: form.name.trim(),
      projectId: null,
    });
    if (token !== weixinPoll) return;
    weixin.sessionId = started.sessionId;
    weixin.imageUrl = started.imageUrl;
    weixin.status = started.status;
    while (token === weixinPoll && showForm.value && weixin.status !== "confirmed" && weixin.status !== "expired") {
      if (weixin.status === "need_verifycode" && !weixin.code.trim()) break;
      const next = await backend.pollWeixinLogin(weixin.sessionId);
      if (token !== weixinPoll) return;
      weixin.status = next.status;
      if (next.imageUrl) weixin.imageUrl = next.imageUrl;
      if (next.status === "confirmed") {
        showForm.value = false;
        await refresh();
        return;
      }
    }
  } catch (cause) {
    if (token === weixinPoll) error.value = String(cause).replace(/^Error:\s*/, "");
  }
}

async function sendWeixinCode() {
  if (!weixin.sessionId || !weixin.code.trim()) return;
  try {
    await backend.submitWeixinVerifyCode(weixin.sessionId, weixin.code.trim());
    weixin.status = "scanned";
    void startWeixinPollOnly();
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  }
}

async function startWeixinPollOnly() {
  const token = weixinPoll;
  const sessionId = weixin.sessionId;
  while (token === weixinPoll && showForm.value && weixin.status !== "confirmed" && weixin.status !== "expired" && weixin.status !== "need_verifycode") {
    const next = await backend.pollWeixinLogin(sessionId);
    if (token !== weixinPoll) return;
    weixin.status = next.status;
    if (next.status === "confirmed") {
      showForm.value = false;
      await refresh();
      return;
    }
  }
}

function projectOptions() {
  return (store.data?.projects ?? []).map((project) => ({ id: project.id, name: project.name }));
}
</script>

<template>
  <div class="page channels-page">
    <div class="page-header">
      <div>
        <h1>{{ t("channels.title") }}</h1>
        <p>{{ t("channels.subtitle") }}</p>
      </div>
      <div class="header-actions">
        <UiButton variant="primary" @click="edit()"><Plus :size="16" /> {{ t("channels.add") }}</UiButton>
      </div>
    </div>

    <p v-if="error" class="form-error">{{ error }}</p>
    <p v-if="notice" class="form-notice">{{ notice }}</p>

    <section v-if="!installations.length" class="panel placeholder">
      <span class="ds-empty-icon"><MessagesSquare :size="18" /></span>
      <h2>{{ t("channels.empty") }}</h2>
      <p>{{ t("channels.emptyHint") }}</p>
    </section>

    <div v-else class="card-list">
      <article v-for="item in installations" :key="item.id" class="channel-entry">
        <div class="list-card channel-card">
          <div class="channel-card-main">
          <span class="channel-card-logo" aria-hidden="true"><ChannelMark :type="item.channelType" /></span>
          <div class="grow">
            <strong>{{ item.name }}</strong>
            <small>{{ channelSourceLabel(item.channelType) }}</small>
          </div>
          <div class="channel-actions">
            <UiButton variant="ghost" size="small" :disabled="busy" @click="test(item)"><Check :size="14" /> {{ t("channels.test") }}</UiButton>
            <UiButton variant="ghost" size="small" @click="openDefaultGrant(item)">{{ t("channels.defaultPermissions") }}</UiButton>
            <UiButton variant="ghost" size="icon" :tip="t('common.edit')" @click="edit(item)"><Pencil :size="14" /></UiButton>
            <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(item)"><Trash2 :size="14" /></UiButton>
          </div>
          <div class="channel-switches">
            <UiTooltip v-if="item.channelType !== 'weixin'" :content="t('channels.allowGroup')">
              <label class="channel-switch">
                <span>{{ t("channels.groupChat") }}</span>
                <UiSwitch
                  :model-value="item.groupEnabled"
                  :busy="switching.has(`${item.id}:group`)"
                  :label="t('channels.groupChat')"
                  @update:model-value="setGroup(item, $event)"
                />
              </label>
            </UiTooltip>
            <label class="channel-switch">
              <span>{{ t("channels.enable") }}</span>
              <UiSwitch
                :model-value="item.enabled"
                :busy="switching.has(`${item.id}:enabled`)"
                :label="t('channels.enable')"
                @update:model-value="setEnabled(item, $event)"
              />
            </label>
          </div>
          </div>
          <div class="channel-status-row">
            <span class="channel-state">
              <span class="channel-status" :class="item.status" />
              {{ statusLabel(item.status) }}
            </span>
            <span v-if="item.botName" class="channel-bot">
              <AgentAvatar v-if="item.botAvatar" :value="item.botAvatar" :name="item.botName" :size="18" />
              <ChannelMark v-else :type="item.channelType" />
              {{ item.botName }}
            </span>
            <label class="channel-switch">
              <UiSwitch
                :model-value="item.pairingRequired !== false"
                :label="t('channels.pairingRequired')"
                @update:model-value="setPairing(item, $event)"
              />
              <span>{{ t("channels.pairingRequired") }}</span>
            </label>
            <span v-if="item.pairingRequired !== false" class="channel-code">
              {{ t("channels.pairingCode") }}
              <UiTooltip :content="t('channels.codeOneTime')"><code>{{ item.pairingCode }}</code></UiTooltip>
              <UiButton variant="ghost" size="icon" :tip="t('channels.copyCode')" @click="copyCode(item)"><Copy :size="13" /></UiButton>
              <UiButton variant="ghost" size="icon" :tip="t('channels.rotateCode')" @click="rotateCode(item)"><RefreshCw :size="13" /></UiButton>
            </span>
          </div>
          <p v-if="identityHint(item)" class="channel-hint">{{ identityHint(item) }}</p>
          <p v-if="item.lastError" class="form-error">{{ item.lastError }}</p>
          <button type="button" class="channel-users-toggle" :aria-expanded="Boolean(usersOpen[item.id])" @click="toggleUsers(item.id)">
            <ChevronDown class="channel-users-chevron" :class="{ 'is-open': usersOpen[item.id] }" :size="14" />
            <span>{{ t("channels.users") }}</span>
            <small>{{ users[item.id]?.length ?? 0 }}</small>
          </button>
          <section v-if="usersOpen[item.id]" class="channel-users">
          <p v-for="message in nameErrors(item.id)" :key="message" class="channel-name-hint" role="status">
            <AlertTriangle :size="14" />
            <span>{{ message }}</span>
          </p>
          <p v-if="!users[item.id]?.length" class="empty-inline">{{ t("channels.noUsers") }}</p>
          <div v-else class="channel-users-scroll">
            <table class="ds-table">
              <thead>
                <tr>
                  <th>{{ t("common.name") }}</th>
                  <th>{{ t("channels.userId") }}</th>
                  <th>{{ t("channels.authorizedAt") }}</th>
                  <th>{{ t("channels.userTarget") }}</th>
                  <th>{{ t("channels.note") }}</th>
                  <th>{{ t("common.actions") }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="user in users[item.id]" :key="user.channelUserId">
                  <td>
                    <UiTooltip v-if="!user.displayName && user.nameError" :content="user.nameError">
                      <span class="channel-name-missing">—</span>
                    </UiTooltip>
                    <template v-else>{{ user.displayName || "—" }}</template>
                  </td>
                  <td class="channel-user-id">{{ user.channelUserId }}</td>
                  <td class="channel-user-time"><UiRelativeTime :value="user.createdAt" /></td>
                  <td class="channel-user-target">
                    <ChatTargetSelect
                      size="sm"
                      :model-value="userTargetValue(user)"
                      :placeholder="t('channels.userTargetNone')"
                      :none-label="t('channels.userTargetNone')"
                      :aria-label="t('channels.userTarget')"
                      @update:model-value="(value) => saveUserTarget(item, user, String(value ?? ''))"
                    />
                  </td>
                  <td class="channel-user-note">
                    <UiInput
                      :model-value="user.note ?? ''"
                      :placeholder="t('channels.notePlaceholder')"
                      :aria-label="t('channels.note')"
                      maxlength="200"
                      @change="saveNote(item, user, $event)"
                      @keydown.enter="($event.target as HTMLInputElement).blur()"
                    />
                  </td>
                  <td class="channel-user-actions">
                    <UiButton variant="ghost" size="small" @click="openUserGrant(item, user)">{{ t("channels.permissions") }}</UiButton>
                    <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="removeUser(item, user.channelUserId)"><X :size="13" /></UiButton>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>
        </div>
      </article>
    </div>

    <UiModal v-if="showForm" @click.self="showForm = false">
      <form class="modal channel-dialog" @submit.prevent="save">
        <header class="channel-dialog-head">
          <h2 class="channel-dialog-title">
            <UiButton
              v-if="step === 'form' && !form.id"
              type="button"
              variant="ghost"
              size="small"
              @click="step = 'pick'"
            >{{ t("common.back") }}</UiButton>
            <ChannelMark v-else-if="step === 'form'" :type="form.channelType" />
            {{ step === "pick" ? t("channels.add") : form.id ? t("channels.edit") : channelSourceLabel(form.channelType) }}
          </h2>
          <UiButton variant="ghost" size="icon" :tip="t('common.close')" @click="showForm = false"><X :size="16" /></UiButton>
        </header>
        <div v-if="step === 'pick'" class="channel-picker" role="listbox" :aria-label="t('channels.platform')">
          <button
            v-for="type in pickerPlatforms"
            :key="type"
            type="button"
            class="channel-picker-row"
            @click="choosePlatform(type)"
          >
            <span class="channel-picker-icon"><ChannelMark :type="type" /></span>
            <span class="channel-picker-copy">
              <span class="channel-picker-title">{{ channelSourceLabel(type) }}</span>
              <span class="channel-picker-desc">{{ t(`channels.desc.${type}`) }}</span>
            </span>
          </button>
        </div>
        <div v-else class="channel-dialog-body">
          <label>{{ t("channels.name") }}<UiInput v-model="form.name" required :placeholder="t('channels.namePlaceholder')" /></label>
          <label v-if="form.channelType === 'feishu' || form.channelType === 'lark'">
            {{ t("channels.region") }}
            <UiSelect v-model="form.channelType" :options="[{ value: 'feishu', label: t('channels.feishu') }, { value: 'lark', label: 'Lark' }]" />
          </label>
          <p class="channel-hint">{{ t("channels.userTargetHint") }}</p>
          <label v-if="credential.showId">
            {{ credential.idLabel }}
            <UiInput v-model="form.appId" :type="form.channelType === 'slack' ? 'password' : 'text'" :required="!form.id" :placeholder="credential.idPlaceholder" />
          </label>
          <label v-if="credential.showSecret">
            {{ credential.secretLabel }}
            <UiInput v-model="form.appSecret" type="password" :placeholder="form.id && !form.appSecret ? t('channels.secretUnchanged') : credential.secretPlaceholder" />
          </label>
          <p class="channel-hint">{{ credential.hint }}</p>
          <div v-if="form.channelType === 'weixin'" class="weixin-login">
            <UiButton type="button" :disabled="busy || !form.name.trim()" @click="startWeixin">{{ t("channels.weixinScan") }}</UiButton>
            <img v-if="weixinImage" class="weixin-qr" :src="weixinImage" alt="" />
            <p v-if="weixin.status === 'scanned'">{{ t("channels.weixinScanned") }}</p>
            <p v-if="weixin.status === 'expired'">{{ t("channels.weixinExpired") }}</p>
            <label v-if="weixin.status === 'need_verifycode'">
              {{ t("channels.verifyCode") }}
              <UiInput v-model="weixin.code" @keydown.enter.prevent="sendWeixinCode" />
            </label>
            <UiButton v-if="weixin.status === 'need_verifycode'" type="button" @click="sendWeixinCode">{{ t("common.confirm") }}</UiButton>
          </div>
        </div>
        <footer v-if="step === 'form'" class="channel-dialog-foot">
          <UiButton type="button" @click="showForm = false">{{ t("common.cancel") }}</UiButton>
          <UiButton v-if="form.channelType !== 'weixin' || form.id" type="submit" variant="primary" :disabled="busy">{{ t("common.save") }}</UiButton>
        </footer>
      </form>
    </UiModal>
    <ChannelGrantDialog
      v-if="grantEditor"
      :title="grantEditor.user ? t('channels.permissions') : t('channels.defaultPermissions')"
      :channel-type="grantEditor.item.channelType"
      :grant="grantEditor.grant"
      :show-target="!grantEditor.user"
      :target-value="grantEditor.targetValue"
      :projects="projectOptions()"
      @update:grant="grantEditor && (grantEditor.grant = $event)"
      @update:target-value="grantEditor && (grantEditor.targetValue = $event)"
      @close="grantEditor = null"
      @save="saveGrant"
    />
  </div>
</template>

<style scoped>
.channels-page { display: flex; flex-direction: column; gap: 16px; }
.channel-card { align-items: flex-start; }
.grow { flex: 1; min-width: 0; }
.channel-state, .channel-bot { display: inline-flex; align-items: center; gap: 6px; }
.channel-status { flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted, #888); }
.channel-switches { display: flex; gap: 16px; align-items: center; align-self: center; flex: none; }
.channel-switch { display: inline-flex; align-items: center; gap: 8px; color: var(--color-text-secondary); font-size: var(--font-size-sm); white-space: nowrap; cursor: pointer; }
.channel-status.connected { background: var(--success, #3fb950); }
.channel-status.error { background: var(--danger, #f85149); }
.channel-status-row { display: flex; flex: 1 1 100%; flex-wrap: wrap; align-items: center; gap: 8px 16px; margin: 6px 0; color: var(--text-muted); }
.channel-hint, .channel-card > .form-error { flex: 1 1 100%; }
.channel-code { display: inline-flex; flex: 0 1 auto; gap: 6px; align-items: center; flex-wrap: nowrap; }
.channel-hint { margin: 0 0 6px; color: var(--text-muted); font-size: var(--font-size-sm); }
.channel-card { flex-wrap: wrap; align-items: stretch; }
.channel-card-main { display: flex; flex: 1 1 100%; align-items: flex-start; gap: var(--spacing-3); min-width: 0; }
.channel-card-logo { flex: none; width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; background: var(--color-fill-secondary, rgba(0, 0, 0, 0.05)); color: var(--color-text-secondary); }
.channel-users-toggle { display: flex; align-items: center; gap: 6px; width: 100%; margin: 0; padding: 8px 0 0; border: 0; background: transparent; color: inherit; font: inherit; cursor: pointer; text-align: left; }
.channel-users-chevron { transition: transform 0.15s ease; }
.channel-users-chevron.is-open { transform: rotate(180deg); }
.channel-users { flex: 1 1 100%; width: 100%; min-height: 0; margin: 0; }
.channel-users-scroll { max-height: 280px; overflow: auto; }
.channel-code code { font-size: 14px; letter-spacing: 2px; color: var(--text-primary); }
.weixin-login { display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
.weixin-qr { width: 180px; height: 180px; object-fit: contain; background: white; border-radius: 8px; }
.channel-actions { display: flex; gap: 4px; flex-wrap: wrap; align-items: center; align-self: center; justify-content: flex-end; }
.channel-users-dialog { width: min(920px, 94vw); }
.channel-user-time { white-space: nowrap; color: var(--color-text-secondary); }
.channel-user-note { min-width: 140px; }
.channel-user-note .ui-input { width: 100%; }
.channel-user-target { min-width: 180px; }
.channel-user-target :deep(.ds-select) { width: 100%; }
.channel-user-id { font-family: var(--font-family-mono); font-weight: var(--font-weight-regular); overflow-wrap: anywhere; }
.channel-user-actions { width: 1%; text-align: right; white-space: nowrap; }
.channel-dialog { width: min(560px, 92vw); max-height: 86vh; overflow: auto; }
.channel-dialog-head, .channel-dialog-foot { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.channel-dialog-title { display: inline-flex; align-items: center; gap: 8px; }
.channel-picker { border: 1px solid var(--color-border, rgba(0, 0, 0, 0.08)); border-radius: 12px; overflow: hidden; margin: 8px 0 4px; }
.channel-picker-row { display: flex; align-items: flex-start; justify-content: flex-start; gap: 12px; width: 100%; padding: 14px 16px; text-align: start; background: transparent; border: 0; border-bottom: 1px solid var(--color-border, rgba(0, 0, 0, 0.08)); cursor: pointer; color: inherit; }
.channel-picker-row:last-child { border-bottom: 0; }
.channel-picker-row:hover { background: var(--color-fill-secondary, rgba(0, 0, 0, 0.03)); }
.channel-picker-icon { flex: none; width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; background: var(--color-fill-secondary, rgba(0, 0, 0, 0.05)); color: var(--color-text-secondary, #444); }
.channel-picker-copy { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.channel-picker-title { font-weight: 600; font-size: 14px; }
.channel-picker-desc { color: var(--text-muted, #888); font-weight: 400; font-size: 13px; line-height: 1.5; }
.channel-dialog-foot { justify-content: flex-end; }
.channel-dialog-body { display: flex; flex-direction: column; gap: 12px; margin: 12px 0; }
.channel-dialog-body label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--text-muted); }
.channel-hint { font-size: 12px; color: var(--text-muted); line-height: 1.6; }
.form-error { color: var(--danger, #f85149); font-size: 13px; }
.form-notice { color: var(--success, #3fb950); font-size: 13px; }
.empty-inline { color: var(--text-muted); font-size: 13px; }
</style>
