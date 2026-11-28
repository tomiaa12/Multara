<script setup lang="ts">
import { computed, reactive } from "vue";
import { AlertTriangle, X } from "@lucide/vue";
import { REMOTE_ROLES, type RemoteGroup } from "../../domain/remote";
import { roleLabel } from "../../components/remote/remote-labels";
import UiButton from "../../components/ui/UiButton.vue";
import UiInput from "../../components/ui/UiInput.vue";
import UiModal from "../../components/ui/UiModal.vue";
import UiSelect from "../../components/ui/UiSelect.vue";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";

/** 与主机端 `save_group` 的取值范围一致。 */
const MIN_RUNNING = 1;
const MAX_RUNNING = 6;
/** 能用 Agent 模式的角色，界面上要提示风险。 */
const AGENT_ROLES = new Set(["runner", "developer", "owner"]);

const props = defineProps<{ group: RemoteGroup }>();
const emit = defineEmits<{ close: []; save: [group: RemoteGroup] }>();

const store = useAppStore();
const form = reactive<RemoteGroup>({ ...props.group, projectIds: [...props.group.projectIds], agentIds: [...props.group.agentIds] });

const roleOptions = REMOTE_ROLES.map((role) => ({ value: role, label: roleLabel(role) }));
const projectOptions = computed(() =>
  (store.data?.projects ?? []).filter((project) => !project.archivedAt).map((project) => ({ value: project.id, label: project.name })),
);
const agentOptions = computed(() => [
  ...(store.data?.agents ?? []).map((agent) => ({ value: agent.id, label: agent.name })),
  ...(store.data?.squads ?? []).map((squad) => ({ value: squad.id, label: t("remote.group.squadOption", { name: squad.name }) })),
]);

function ids(value: unknown) {
  return Array.isArray(value) ? value.map(String) : [];
}

function submit() {
  const running = Number(form.maxRunning);
  emit("save", { ...form, name: form.name.trim(), maxRunning: Math.min(MAX_RUNNING, Math.max(MIN_RUNNING, Number.isFinite(running) ? running : MIN_RUNNING)) });
}
</script>

<template>
  <UiModal @close="emit('close')">
    <form class="modal domain-modal" @submit.prevent="submit">
      <header class="modal-heading">
        <h2>{{ group.id ? t("remote.group.edit") : t("remote.group.add") }}</h2>
        <UiButton type="button" variant="ghost" size="icon" :tip="t('common.close')" @click="emit('close')"><X :size="16" /></UiButton>
      </header>
      <div class="modal-body">
        <label>{{ t("remote.group.name") }}<UiInput v-model="form.name" required maxlength="64" /></label>
        <label>
          {{ t("remote.group.role") }}
          <UiSelect v-model="form.role" :options="roleOptions" />
          <small class="remote-hint">{{ t(`remote.roleDesc.${form.role}`) }}</small>
        </label>
        <p v-if="AGENT_ROLES.has(form.role)" class="remote-risk"><AlertTriangle :size="14" /> {{ t("remote.group.agentRisk") }}</p>
        <section>
          <h3>{{ t("remote.group.projects") }}</h3>
          <label class="check-row"><input v-model="form.allProjects" class="ui-checkbox" type="checkbox" /> <span>{{ t("remote.group.allProjects") }}</span></label>
          <UiSelect
            v-if="!form.allProjects"
            :model-value="form.projectIds"
            multiple
            searchable
            clearable
            :placeholder="t('remote.group.pickProjects')"
            :aria-label="t('remote.group.projects')"
            :options="projectOptions"
            @update:model-value="form.projectIds = ids($event)"
          />
        </section>
        <section>
          <h3>{{ t("remote.group.agents") }}</h3>
          <label class="check-row"><input v-model="form.allAgents" class="ui-checkbox" type="checkbox" /> <span>{{ t("remote.group.allAgents") }}</span></label>
          <UiSelect
            v-if="!form.allAgents"
            :model-value="form.agentIds"
            multiple
            searchable
            clearable
            :placeholder="t('remote.group.pickAgents')"
            :aria-label="t('remote.group.agents')"
            :options="agentOptions"
            @update:model-value="form.agentIds = ids($event)"
          />
        </section>
        <section>
          <h3>{{ t("remote.group.rules") }}</h3>
          <label class="check-row"><input v-model="form.shareInGroup" class="ui-checkbox" type="checkbox" /> <span>{{ t("remote.group.shareInGroup") }}</span></label>
          <label class="check-row"><input v-model="form.requireApproval" class="ui-checkbox" type="checkbox" /> <span>{{ t("remote.group.requireApproval") }}</span></label>
          <label>
            {{ t("remote.group.maxRunning") }}
            <UiInput v-model.number="form.maxRunning" type="number" :min="MIN_RUNNING" :max="MAX_RUNNING" class="remote-number" />
          </label>
        </section>
      </div>
      <div class="modal-actions">
        <UiButton type="button" @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton type="submit" variant="primary" :disabled="!form.name.trim()">{{ t("common.save") }}</UiButton>
      </div>
    </form>
  </UiModal>
</template>

<style scoped>
.modal-body section { display: grid; gap: 6px; }
.modal-body h3 { margin: 4px 0 0; font-size: var(--font-size-sm); }
.remote-hint { color: var(--color-text-tertiary); font-weight: var(--font-weight-regular); }
.remote-risk { display: flex; gap: 6px; margin: 0; color: var(--color-warning); font-size: var(--font-size-sm); line-height: 1.6; }
.remote-risk svg { flex: none; margin-top: 3px; }
.remote-number { width: 120px; }
</style>
