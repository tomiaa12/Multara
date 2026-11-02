<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronDown, ChevronUp, LogIn, Plus } from "@lucide/vue";
import type { Runtime } from "../domain/types";
import { t } from "../i18n";
import {
  removeRuntimeAccount,
  renameRuntimeAccount,
  switchRuntimeAccount,
  type RuntimeAccount,
  type RuntimeAccountStatus,
} from "../services/backend";

/** 默认显示几个账号；每点一次「展开更多」再多显示这么多 */
const ACCOUNT_PAGE_SIZE = 3;

const props = defineProps<{ runtime: Runtime; status: RuntimeAccountStatus }>();
const emit = defineEmits<{ update: [status: RuntimeAccountStatus]; login: [] }>();

/** 进行中的操作，如 `switch:<id>` */
const busy = ref("");
const error = ref("");
const notice = ref("");
const editingId = ref("");
const draftName = ref("");
const visibleCount = ref(ACCOUNT_PAGE_SIZE);

const accounts = computed(() =>
  props.status.switchable
    ? [...props.status.accounts].sort((left, right) => Number(right.current) - Number(left.current))
    : [],
);
const visibleAccounts = computed(() => accounts.value.slice(0, visibleCount.value));
const hiddenCount = computed(() => accounts.value.length - visibleAccounts.value.length);
const signedIn = computed(() => props.status.live.signedIn === true || accounts.value.length > 0);
const hint = computed(() => {
  if (props.status.tier === "d") return t("runtimeAccount.unregistered");
  if (props.status.reason) return props.status.reason;
  if (accounts.value.length) return "";
  if (props.status.live.signedIn == null) return t("runtimeAccount.unknown");
  if (!props.status.live.signedIn) return t("runtimeAccount.signedOut");
  return t("runtimeAccount.currentOnly", { name: props.status.live.label || t("runtimeAccount.signedIn") });
});

async function act(key: string, task: () => Promise<RuntimeAccountStatus>) {
  busy.value = key;
  error.value = "";
  notice.value = "";
  try {
    emit("update", await task());
    return true;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    return false;
  } finally {
    busy.value = "";
  }
}

function switchTo(account: RuntimeAccount) {
  return act(`switch:${account.id}`, async () => {
    const next = await switchRuntimeAccount(props.runtime.id, account.id);
    notice.value = next.runningElsewhere ? t("runtimeAccount.restartHint") : t("runtimeAccount.switched");
    return next;
  });
}

function remove(account: RuntimeAccount) {
  if (!window.confirm(t("runtimeAccount.removeConfirm", { name: accountTitle(account) }))) return;
  return act(`remove:${account.id}`, () => removeRuntimeAccount(props.runtime.id, account.id));
}

function startRename(account: RuntimeAccount) {
  editingId.value = account.id;
  draftName.value = account.name ?? account.label ?? "";
}

async function saveRename() {
  const id = editingId.value;
  if (await act(`rename:${id}`, () => renameRuntimeAccount(props.runtime.id, id, draftName.value))) {
    editingId.value = "";
  }
}

function accountTitle(account: RuntimeAccount) {
  return account.name || account.label || t("runtimeAccount.unnamed");
}
</script>

<template>
  <div class="runtime-accounts" @click.stop>
    <p v-if="status.overrideEnv" class="ds-alert" data-tone="warning">
      {{ t("runtimeAccount.overrideEnv", { env: status.overrideEnv }) }}
    </p>
    <ul v-if="visibleAccounts.length" class="runtime-accounts-list">
      <li v-for="account in visibleAccounts" :key="account.id" :data-current="account.current">
        <template v-if="editingId === account.id">
          <UiInput
            v-model="draftName"
            :placeholder="account.label ?? t('runtimeAccount.unnamed')"
            :aria-label="t('runtimeAccount.rename')"
            @keydown.enter.prevent="saveRename"
            @keydown.esc.stop="editingId = ''"
          />
          <UiButton size="small" :disabled="Boolean(busy)" @click="saveRename">{{ t("common.save") }}</UiButton>
          <UiButton size="small" variant="ghost" @click="editingId = ''">{{ t("common.cancel") }}</UiButton>
        </template>
        <template v-else>
          <span class="runtime-accounts-name">
            <strong>{{ accountTitle(account) }}</strong>
            <small>
              <template v-if="account.name && account.label">{{ account.label }} · </template>
              {{ t("runtimeAccount.capturedAt") }} <UiRelativeTime :value="account.capturedAt" />
            </small>
          </span>
          <span v-if="account.current" class="ds-badge" data-tone="success">{{ t("runtimeAccount.inUse") }}</span>
          <UiButton
            v-else
            size="small"
            :class="{ 'is-loading': busy === `switch:${account.id}` }"
            :disabled="Boolean(busy)"
            @click="switchTo(account)"
          >
            {{ t("runtimeAccount.switch") }}
          </UiButton>
          <UiButton size="small" variant="ghost" :disabled="Boolean(busy)" @click="startRename(account)">
            {{ t("runtimeAccount.rename") }}
          </UiButton>
          <UiButton
            v-if="!account.current"
            size="small"
            variant="danger"
            :disabled="Boolean(busy)"
            @click="remove(account)"
          >
            {{ t("runtimeAccount.remove") }}
          </UiButton>
        </template>
      </li>
    </ul>
    <div class="runtime-accounts-foot">
      <span v-if="hint" class="runtime-accounts-hint">{{ hint }}</span>
      <UiButton v-if="hiddenCount > 0" size="small" variant="ghost" @click="visibleCount += ACCOUNT_PAGE_SIZE">
        <ChevronDown :size="14" /> {{ t("runtimeAccount.showMore", { n: hiddenCount }) }}
      </UiButton>
      <UiButton v-if="visibleCount > ACCOUNT_PAGE_SIZE" size="small" variant="ghost" @click="visibleCount = ACCOUNT_PAGE_SIZE">
        <ChevronUp :size="14" /> {{ t("runtimeAccount.collapse") }}
      </UiButton>
      <UiButton class="runtime-accounts-login" size="small" :disabled="!runtime.path" @click="emit('login')">
        <template v-if="signedIn"><Plus :size="14" /> {{ t("runtimeAccount.add") }}</template>
        <template v-else><LogIn :size="14" /> {{ t("runtimeAccount.loginWith", { name: runtime.name }) }}</template>
      </UiButton>
    </div>
    <p v-if="notice" class="ds-alert" data-tone="success">{{ notice }}</p>
    <p v-if="error" class="form-error">{{ error }}</p>
  </div>
</template>

<style scoped>
.runtime-accounts {
  display: grid;
  gap: var(--spacing-2);
  min-width: 0;
  cursor: default;
}
.runtime-accounts-list {
  display: grid;
  gap: var(--spacing-1);
  margin: 0;
  padding: 0;
  list-style: none;
}
.runtime-accounts-list li {
  display: flex;
  align-items: center;
  gap: var(--spacing-2);
  min-height: var(--control-height-md);
  padding: var(--spacing-1) var(--spacing-2-5);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-surface);
}
.runtime-accounts-list li[data-current="true"] {
  border-color: var(--color-success-border);
}
.runtime-accounts-list li > .ui-input {
  flex: 1;
}
.runtime-accounts-name {
  display: grid;
  flex: 1;
  min-width: 0;
}
.runtime-accounts-name strong,
.runtime-accounts-name small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.runtime-accounts-name strong {
  font-size: var(--font-size-sm);
  font-weight: var(--font-weight-medium);
}
.runtime-accounts-name small,
.runtime-accounts-hint {
  color: var(--color-text-secondary);
  font-size: var(--font-size-xs);
}
.runtime-accounts-foot {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--spacing-2);
}
.runtime-accounts-login {
  margin-left: auto;
}
</style>
