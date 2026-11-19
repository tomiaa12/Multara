<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { AlertTriangle, Check, Copy, MessagesSquare, Pencil, Plus, RefreshCw, Trash2, X } from "@lucide/vue";
import type { ChannelInstallation, ChannelInstallationInput, ChannelUser } from "../domain/types";
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

const store = useAppStore();
const installations = ref<ChannelInstallation[]>([]);
const users = reactive<Record<string, ChannelUser[]>>({});
const usersTarget = ref<ChannelInstallation | null>(null);
const showForm = ref(false);
const busy = ref(false);
const error = ref("");
const notice = ref("");

const form = reactive({
  id: undefined as string | undefined,
  name: "",
  channelType: "feishu" as "feishu" | "lark",
  projectId: "",
  appId: "",
  appSecret: "",
  groupEnabled: false,
  enabled: true,
});

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
  } catch (cause) {
    error.value = String(cause);
  }
}

let unlistenPaired: UnlistenFn | null = null;
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
  if (unmounted) unlisten();
  else unlistenPaired = unlisten;
});

onBeforeUnmount(() => {
  unmounted = true;
  unlistenPaired?.();
  unlistenPaired = null;
});

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
  form.appId = item?.appId ?? "";
  form.appSecret = "";
  form.groupEnabled = item?.groupEnabled ?? false;
  form.enabled = item?.enabled ?? true;
  showForm.value = true;
}

async function save() {
  busy.value = true;
  error.value = "";
  try {
    const input: ChannelInstallationInput = {
      id: form.id,
      name: form.name.trim(),
      channelType: form.channelType,
      projectId: form.projectId || null,
      appId: form.appId.trim(),
      appSecret: form.appSecret.trim() || undefined,
      region: form.channelType,
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
    if (usersTarget.value?.id === item.id) usersTarget.value = null;
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

function roleLabel(role: string) {
  if (role === "owner") return t("channels.roleOwner");
  return role;
}

/** 同一原因（如缺权限）只提示一次。 */
function nameErrors(installationId: string) {
  const messages = (users[installationId] ?? [])
    .filter((user) => !user.displayName && user.nameError)
    .map((user) => user.nameError as string);
  return [...new Set(messages)];
}

async function openUsers(item: ChannelInstallation) {
  usersTarget.value = item;
  try {
    users[item.id] = await backend.listChannelAllowedUsers(item.id);
  } catch (cause) {
    error.value = String(cause);
  }
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

function projectName(item: ChannelInstallation) {
  if (!item.projectId) return t("channels.noProject");
  return store.data?.projects.find((project) => project.id === item.projectId)?.name ?? item.projectId;
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
          <div class="grow">
            <strong>{{ item.name }}</strong>
            <small>
              {{ item.channelType === "lark" ? "Lark" : t("channels.feishu") }} ·
              {{ projectName(item) }}
            </small>
            <p class="channel-meta">
              <span class="channel-state">
                <span class="channel-status" :class="item.status" />
                {{ statusLabel(item.status) }}
              </span>
              <span v-if="item.botName" class="channel-bot">
                <AgentAvatar :value="item.botAvatar" :name="item.botName" :size="18" />
                {{ item.botName }}
              </span>
            </p>
            <p v-if="item.lastError" class="form-error">{{ item.lastError }}</p>
            <p class="channel-code">
              {{ t("channels.pairingCode") }}
              <UiTooltip :content="t('channels.codeOneTime')"><code>{{ item.pairingCode }}</code></UiTooltip>
              <UiButton variant="ghost" size="icon" :tip="t('channels.copyCode')" @click="copyCode(item)"><Copy :size="13" /></UiButton>
              <UiButton variant="ghost" size="icon" :tip="t('channels.rotateCode')" @click="rotateCode(item)"><RefreshCw :size="13" /></UiButton>
            </p>
          </div>
          <div class="channel-actions">
            <UiButton variant="ghost" size="small" :disabled="busy" @click="test(item)"><Check :size="14" /> {{ t("channels.test") }}</UiButton>
            <UiButton variant="ghost" size="small" @click="openUsers(item)">{{ t("channels.users") }}</UiButton>
            <UiButton variant="ghost" size="icon" :tip="t('common.edit')" @click="edit(item)"><Pencil :size="14" /></UiButton>
            <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(item)"><Trash2 :size="14" /></UiButton>
          </div>
          <div class="channel-switches">
            <UiTooltip :content="t('channels.allowGroup')">
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
      </article>
    </div>

    <UiModal v-if="usersTarget" @click.self="usersTarget = null">
      <div class="modal channel-users-dialog">
        <header class="channel-dialog-head">
          <h2>{{ t("channels.users") }}</h2>
          <UiButton variant="ghost" size="icon" :tip="t('common.close')" @click="usersTarget = null"><X :size="16" /></UiButton>
        </header>
        <p v-for="message in nameErrors(usersTarget.id)" :key="message" class="channel-name-hint" role="status">
          <AlertTriangle :size="14" />
          <span>{{ message }}</span>
        </p>
        <p v-if="!users[usersTarget.id]?.length" class="empty-inline">{{ t("channels.noUsers") }}</p>
        <div v-else class="ds-table-wrap">
          <table class="ds-table">
            <thead>
              <tr>
                <th>{{ t("common.name") }}</th>
                <th>{{ t("channels.userId") }}</th>
                <th>{{ t("channels.userRole") }}</th>
                <th>{{ t("channels.authorizedAt") }}</th>
                <th>{{ t("channels.userTarget") }}</th>
                <th>{{ t("channels.note") }}</th>
                <th>{{ t("common.actions") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="user in users[usersTarget.id]" :key="user.channelUserId">
                <td>
                  <UiTooltip v-if="!user.displayName && user.nameError" :content="user.nameError">
                    <span class="channel-name-missing">—</span>
                  </UiTooltip>
                  <template v-else>{{ user.displayName || "—" }}</template>
                </td>
                <td class="channel-user-id">{{ user.channelUserId }}</td>
                <td>{{ roleLabel(user.role) }}</td>
                <td class="channel-user-time"><UiRelativeTime :value="user.createdAt" /></td>
                <td class="channel-user-target">
                  <ChatTargetSelect
                    size="sm"
                    :model-value="userTargetValue(user)"
                    :placeholder="t('channels.userTargetNone')"
                    :none-label="t('channels.userTargetNone')"
                    :aria-label="t('channels.userTarget')"
                    @update:model-value="(value) => saveUserTarget(usersTarget!, user, String(value ?? ''))"
                  />
                </td>
                <td class="channel-user-note">
                  <UiInput
                    :model-value="user.note ?? ''"
                    :placeholder="t('channels.notePlaceholder')"
                    :aria-label="t('channels.note')"
                    maxlength="200"
                    @change="saveNote(usersTarget, user, $event)"
                    @keydown.enter="($event.target as HTMLInputElement).blur()"
                  />
                </td>
                <td class="channel-user-actions">
                  <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="removeUser(usersTarget, user.channelUserId)"><X :size="13" /></UiButton>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </UiModal>

    <UiModal v-if="showForm" @click.self="showForm = false">
      <form class="modal channel-dialog" @submit.prevent="save">
        <header class="channel-dialog-head">
          <h2>{{ form.id ? t("channels.edit") : t("channels.add") }}</h2>
          <UiButton variant="ghost" size="icon" :tip="t('common.close')" @click="showForm = false"><X :size="16" /></UiButton>
        </header>
        <div class="channel-dialog-body">
          <label>{{ t("channels.name") }}<UiInput v-model="form.name" required :placeholder="t('channels.namePlaceholder')" /></label>
          <label>
            {{ t("channels.platform") }}
            <UiSelect v-model="form.channelType" :options="[{ value: 'feishu', label: t('channels.feishu') }, { value: 'lark', label: 'Lark' }]" />
          </label>
          <label>
            {{ t("channels.project") }}
            <UiSelect
              v-model="form.projectId"
              :options="[{ value: '', label: t('channels.noProject') }, ...(store.data?.projects ?? []).map((project) => ({ value: project.id, label: project.name }))]"
            />
          </label>
          <p class="channel-hint">{{ t("channels.userTargetHint") }}</p>
          <label>{{ t("channels.appId") }}<UiInput v-model="form.appId" required placeholder="cli_xxxxxxxx" /></label>
          <label>
            {{ t("channels.appSecret") }}
            <UiInput v-model="form.appSecret" type="password" :placeholder="form.id && !form.appSecret ? t('channels.secretUnchanged') : ''" />
          </label>
          <p class="channel-hint">{{ t("channels.setupHint") }}</p>
        </div>
        <footer class="channel-dialog-foot">
          <UiButton type="button" @click="showForm = false">{{ t("common.cancel") }}</UiButton>
          <UiButton type="submit" variant="primary" :disabled="busy">{{ t("common.save") }}</UiButton>
        </footer>
      </form>
    </UiModal>
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
.channel-meta { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; color: var(--text-muted); margin: 6px 0; }
.channel-code { display: flex; gap: 6px; align-items: center; color: var(--text-muted); }
.channel-code code { font-size: 14px; letter-spacing: 2px; color: var(--text-primary); }
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
.channel-dialog-foot { justify-content: flex-end; }
.channel-dialog-body { display: flex; flex-direction: column; gap: 12px; margin: 12px 0; }
.channel-dialog-body label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--text-muted); }
.channel-hint { font-size: 12px; color: var(--text-muted); line-height: 1.6; }
.form-error { color: var(--danger, #f85149); font-size: 13px; }
.form-notice { color: var(--success, #3fb950); font-size: 13px; }
.empty-inline { color: var(--text-muted); font-size: 13px; }
</style>
