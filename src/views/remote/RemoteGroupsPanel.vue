<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Pencil, Plus, Trash2 } from "@lucide/vue";
import type { RemoteGroup } from "../../domain/remote";
import { roleLabel } from "../../components/remote/remote-labels";
import { confirmDialog } from "../../components/ui/confirm-dialog";
import UiButton from "../../components/ui/UiButton.vue";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";
import RemoteGroupDialog from "./RemoteGroupDialog.vue";

const DEFAULT_MAX_RUNNING = 2;

const groups = ref<RemoteGroup[]>([]);
const editing = ref<RemoteGroup | null>(null);
const error = ref("");

function blankGroup(): RemoteGroup {
  return {
    id: "",
    name: "",
    role: "viewer",
    allProjects: false,
    allAgents: true,
    shareInGroup: false,
    requireApproval: false,
    maxRunning: DEFAULT_MAX_RUNNING,
    isDefault: false,
    projectIds: [],
    agentIds: [],
    deviceCount: 0,
  };
}

async function reload() {
  groups.value = await remoteApi.listGroups().catch((cause) => {
    error.value = String(cause);
    return groups.value;
  });
}

async function save(group: RemoteGroup) {
  error.value = "";
  try {
    await remoteApi.saveGroup(group);
    editing.value = null;
    await reload();
  } catch (cause) {
    error.value = String(cause).replace(/^Error:\s*/, "");
  }
}

async function remove(group: RemoteGroup) {
  if (!(await confirmDialog(t("remote.group.deleteConfirm", { name: group.name }), { danger: true }))) return;
  error.value = "";
  await remoteApi.deleteGroup(group.id).catch((cause) => (error.value = String(cause).replace(/^Error:\s*/, "")));
  await reload();
}

function scopeLabel(group: RemoteGroup) {
  if (group.allProjects) return t("remote.group.allProjects");
  return t("remote.group.projectCount", { n: group.projectIds.length });
}

onMounted(reload);
</script>

<template>
  <div class="remote-toolbar">
    <p class="remote-hint">{{ t("remote.group.hint") }}</p>
    <UiButton variant="primary" @click="editing = blankGroup()"><Plus :size="16" /> {{ t("remote.group.add") }}</UiButton>
  </div>
  <p v-if="error" class="form-error">{{ error }}</p>
  <div class="card-list">
    <article v-for="group in groups" :key="group.id" class="list-card remote-group">
      <div class="grow">
        <strong>{{ group.name }}<span v-if="group.isDefault" class="remote-tag">{{ t("remote.group.default") }}</span></strong>
        <small>
          {{ roleLabel(group.role) }} · {{ scopeLabel(group) }} · {{ t("remote.group.deviceCount", { n: group.deviceCount }) }}
          <template v-if="group.requireApproval"> · {{ t("remote.group.requireApprovalShort") }}</template>
          <template v-if="group.shareInGroup"> · {{ t("remote.group.shareShort") }}</template>
        </small>
      </div>
      <UiButton variant="ghost" size="icon" :tip="t('common.edit')" @click="editing = group"><Pencil :size="14" /></UiButton>
      <UiButton v-if="!group.isDefault" variant="ghost" size="icon" :tip="t('common.delete')" @click="remove(group)"><Trash2 :size="14" /></UiButton>
    </article>
  </div>
  <RemoteGroupDialog v-if="editing" :group="editing" @close="editing = null" @save="save" />
</template>

<style scoped>
.remote-toolbar { display: flex; align-items: center; gap: 12px; justify-content: space-between; }
.remote-hint { margin: 0; color: var(--color-text-tertiary); font-size: var(--font-size-sm); line-height: 1.6; }
.remote-group { align-items: center; gap: 8px; }
.grow { display: grid; flex: 1; min-width: 0; gap: 2px; }
.grow small { color: var(--color-text-tertiary); }
.remote-tag { margin-left: 8px; padding: 0 6px; border: 1px solid var(--color-border); border-radius: var(--radius-sm); color: var(--color-text-secondary); font-size: var(--font-size-sm); font-weight: var(--font-weight-regular); }
.form-error { margin: 0; color: var(--color-error); font-size: var(--font-size-sm); }
</style>
