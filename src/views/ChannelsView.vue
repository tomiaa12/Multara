<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { Check, Copy, MessagesSquare, Pencil, Plus, RefreshCw, Trash2, X } from "@lucide/vue";
import type { ChannelInstallation, ChannelInstallationInput, ChannelUser } from "../domain/types";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import * as backend from "../services/backend";
import UiButton from "../components/ui/UiButton.vue";
import UiInput from "../components/ui/UiInput.vue";
import UiModal from "../components/ui/UiModal.vue";
import UiSelect from "../components/ui/UiSelect.vue";

const store = useAppStore();
const installations = ref<ChannelInstallation[]>([]);
const users = reactive<Record<string, ChannelUser[]>>({});
const expandedId = ref<string | null>(null);
const showForm = ref(false);
const busy = ref(false);
const error = ref("");
const notice = ref("");

const form = reactive({
  id: undefined as string | undefined,
  name: "",
  channelType: "feishu" as "feishu" | "lark",
  targetType: "agent" as "agent" | "squad",
  targetId: "",
  projectId: "",
  appId: "",
  appSecret: "",
  groupEnabled: false,
  enabled: true,
});

const targets = computed(() => [
  {
    label: t("channels.agentGroup"),
    options: (store.data?.agents ?? []).map((agent) => ({ value: `agent:${agent.id}`, label: agent.name })),
  },
  {
    label: t("channels.squadGroup"),
    options: (store.data?.squads ?? []).map((squad) => ({ value: `squad:${squad.id}`, label: squad.name })),
  },
].filter((group) => group.options.length));

const targetValue = computed({
  get: () => (form.targetId ? `${form.targetType}:${form.targetId}` : ""),
  set: (value: string) => {
    const [type, id] = value.split(":");
    form.targetType = type === "squad" ? "squad" : "agent";
    form.targetId = id ?? "";
  },
});

async function refresh() {
  try {
    installations.value = await backend.listChannelInstallations();
  } catch (cause) {
    error.value = String(cause);
  }
}

onMounted(async () => {
  if (!store.data) await store.initialize();
  await refresh();
});

function statusTone(status: ChannelInstallation["status"]) {
  if (status === "connected") return "success";
  if (status === "error") return "error";
  return undefined;
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
  form.targetType = item?.targetType ?? "agent";
  form.targetId = item?.targetId ?? "";
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
    if (!form.targetId) throw new Error(t("channels.targetRequired"));
    const input: ChannelInstallationInput = {
      id: form.id,
      name: form.name.trim(),
      channelType: form.channelType,
      targetType: form.targetType,
      targetId: form.targetId,
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

async function toggle(item: ChannelInstallation) {
  try {
    await backend.setChannelEnabled(item.id, !item.enabled);
    await refresh();
  } catch (cause) {
    error.value = String(cause);
  }
}

async function remove(item: ChannelInstallation) {
  if (!window.confirm(t("channels.deleteConfirm", { name: item.name }))) return;
  try {
    await backend.deleteChannelInstallation(item.id);
    if (expandedId.value === item.id) expandedId.value = null;
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

async function toggleUsers(item: ChannelInstallation) {
  if (expandedId.value === item.id) {
    expandedId.value = null;
    return;
  }
  expandedId.value = item.id;
  try {
    users[item.id] = await backend.listChannelAllowedUsers(item.id);
  } catch (cause) {
    error.value = String(cause);
  }
}

async function removeUser(item: ChannelInstallation, userId: string) {
  try {
    await backend.removeChannelAllowedUser(item.id, userId);
    users[item.id] = await backend.listChannelAllowedUsers(item.id);
  } catch (cause) {
    error.value = String(cause);
  }
}

async function rotateCode(item: ChannelInstallation) {
  try {
    const code = await backend.rotateChannelPairingCode(item.id);
    item.pairingCode = code;
    await navigator.clipboard?.writeText(code).catch(() => {});
    notice.value = t("channels.codeRotated", { code });
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

function targetName(item: ChannelInstallation) {
  const list = item.targetType === "squad" ? store.data?.squads : store.data?.agents;
  return list?.find((target) => target.id === item.targetId)?.name ?? item.targetId;
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
          <span class="channel-status" :class="item.status" />
          <div class="grow">
            <strong>{{ item.name }}</strong>
            <small>
              {{ item.channelType === "lark" ? "Lark" : t("channels.feishu") }} ·
              {{ item.targetType === "squad" ? t("channels.squad") : t("channels.agent") }}：{{ targetName(item) }} ·
              {{ projectName(item) }}
            </small>
            <p class="channel-meta">
              <span class="ds-badge" :data-tone="statusTone(item.status)">{{ statusLabel(item.status) }}</span>
              <span v-if="item.botName">Bot：{{ item.botName }}</span>
              <span>{{ t("channels.groupEnabled", { state: item.groupEnabled ? t("common.enabledOn") : t("common.enabledOff") }) }}</span>
            </p>
            <p v-if="item.lastError" class="form-error">{{ item.lastError }}</p>
            <p class="channel-code">
              {{ t("channels.pairingCode") }}
              <code>{{ item.pairingCode }}</code>
              <UiButton variant="ghost" size="icon" :tip="t('channels.copyCode')" @click="copyCode(item)"><Copy :size="13" /></UiButton>
              <UiButton variant="ghost" size="icon" :tip="t('channels.rotateCode')" @click="rotateCode(item)"><RefreshCw :size="13" /></UiButton>
            </p>
          </div>
          <span class="ds-badge" :data-tone="item.enabled ? 'success' : undefined">
            {{ item.enabled ? t("common.enabledOn") : t("common.enabledOff") }}
          </span>
          <div class="channel-actions">
            <UiButton variant="ghost" size="small" :disabled="busy" @click="test(item)"><Check :size="14" /> {{ t("channels.test") }}</UiButton>
            <UiButton variant="ghost" size="small" @click="toggle(item)">{{ item.enabled ? t("channels.disable") : t("channels.enable") }}</UiButton>
            <UiButton variant="ghost" size="small" @click="toggleUsers(item)">{{ t("channels.users") }}</UiButton>
            <UiButton variant="ghost" size="icon" :tip="t('common.edit')" @click="edit(item)"><Pencil :size="14" /></UiButton>
            <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(item)"><Trash2 :size="14" /></UiButton>
          </div>
        </div>
        <div v-if="expandedId === item.id" class="channel-users">
          <div v-if="!users[item.id]?.length" class="empty-inline">{{ t("channels.noUsers") }}</div>
          <div v-for="user in users[item.id]" :key="user.channelUserId" class="history-row">
            <strong>{{ user.displayName || user.channelUserId }}</strong>
            <code>{{ user.channelUserId }}</code>
            <span class="ds-badge">{{ user.role }}</span>
            <UiButton variant="ghost" size="icon" :tip="t('common.delete')" @click="removeUser(item, user.channelUserId)"><X :size="13" /></UiButton>
          </div>
        </div>
      </article>
    </div>

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
          <label>{{ t("channels.target") }}<UiSelect v-model="targetValue" required :placeholder="t('channels.targetPlaceholder')" :groups="targets" /></label>
          <label>
            {{ t("channels.project") }}
            <UiSelect
              v-model="form.projectId"
              :options="[{ value: '', label: t('channels.noProject') }, ...(store.data?.projects ?? []).map((project) => ({ value: project.id, label: project.name }))]"
            />
          </label>
          <label>{{ t("channels.appId") }}<UiInput v-model="form.appId" required placeholder="cli_xxxxxxxx" /></label>
          <label>
            {{ t("channels.appSecret") }}
            <UiInput v-model="form.appSecret" type="password" :placeholder="form.id && !form.appSecret ? t('channels.secretUnchanged') : ''" />
          </label>
          <label class="channel-check"><input v-model="form.groupEnabled" type="checkbox" /> {{ t("channels.allowGroup") }}</label>
          <label class="channel-check"><input v-model="form.enabled" type="checkbox" /> {{ t("channels.enableNow") }}</label>
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
.channel-status { width: 8px; height: 8px; border-radius: 50%; margin-top: 8px; background: var(--text-muted, #888); }
.channel-status.connected { background: var(--success, #3fb950); }
.channel-status.error { background: var(--danger, #f85149); }
.channel-meta { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; color: var(--text-muted); margin: 6px 0; }
.channel-code { display: flex; gap: 6px; align-items: center; color: var(--text-muted); }
.channel-code code { font-size: 14px; letter-spacing: 2px; color: var(--text-primary); }
.channel-actions { display: flex; gap: 4px; flex-wrap: wrap; justify-content: flex-end; }
.channel-users { padding: 10px 16px; display: flex; flex-direction: column; gap: 6px; }
.channel-dialog { width: min(560px, 92vw); max-height: 86vh; overflow: auto; }
.channel-dialog-head, .channel-dialog-foot { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.channel-dialog-foot { justify-content: flex-end; }
.channel-dialog-body { display: flex; flex-direction: column; gap: 12px; margin: 12px 0; }
.channel-dialog-body label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; color: var(--text-muted); }
.channel-check { flex-direction: row !important; align-items: center; gap: 8px !important; }
.channel-hint { font-size: 12px; color: var(--text-muted); line-height: 1.6; }
.form-error { color: var(--danger, #f85149); font-size: 13px; }
.form-notice { color: var(--success, #3fb950); font-size: 13px; }
.empty-inline { color: var(--text-muted); font-size: 13px; }
</style>
