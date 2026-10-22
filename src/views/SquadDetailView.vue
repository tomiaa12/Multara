<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { ArrowUpRight, Check, Crown, FileText, Plus, Trash2, Users } from "@lucide/vue";
import { useRoute, useRouter } from "vue-router";
import type { Agent, Squad } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import { useAppStore } from "../stores/app";
import { t } from "../i18n";

type Pane = "members" | "instructions";
const route = useRoute();
const router = useRouter();
const store = useAppStore();
const pane = ref<Pane>("members");
const busy = ref(false);
const saved = ref(false);
const error = ref("");
const addAgentId = ref("");
const squad = computed(() => store.data?.squads.find((item) => item.id === String(route.params.squadId)) ?? null);
const members = computed(() => {
  const list = store.data?.agents.filter((agent) => draft.memberAgentIds.includes(agent.id)) ?? [];
  const leadId = draft.leaderAgentId;
  if (!leadId) return list;
  return [...list].sort((a, b) => Number(b.id === leadId) - Number(a.id === leadId));
});
const leader = computed(() => store.data?.agents.find((agent) => agent.id === draft.leaderAgentId) ?? null);
const candidates = computed(() => store.data?.agents.filter((agent) => !draft.memberAgentIds.includes(agent.id)) ?? []);
const instructionsDirty = computed(() => draft.instructions !== (squad.value?.instructions ?? ""));
const draft = reactive<Omit<Squad, "id">>({
  name: "",
  description: "",
  instructions: "",
  leaderAgentId: "",
  memberAgentIds: [],
});

watch(squad, (value) => {
  if (!value) return;
  Object.assign(draft, {
    name: value.name,
    description: value.description,
    instructions: value.instructions ?? "",
    leaderAgentId: value.leaderAgentId,
    memberAgentIds: [...value.memberAgentIds],
  });
}, { immediate: true });

type MemberStatusKind = "working" | "idle" | "offline";

function memberStatusKind(agent: Agent): MemberStatusKind {
  const running = Object.values(store.runStates).some((event) =>
    event.agentId === agent.id
    && (event.status === "running" || event.status === "queued")
    && !event.runId.startsWith("pending:"),
  );
  if (running) return "working";
  const runtime = store.data?.runtimes.find((item) => item.id === agent.runtimeId);
  return runtime?.status === "online" ? "idle" : "offline";
}

function memberStatusLabel(agent: Agent) {
  return t(`squad.status.${memberStatusKind(agent)}`);
}

function memberStatusData(agent: Agent) {
  return memberStatusKind(agent) === "working"
    ? t("collection.status.running")
    : memberStatusLabel(agent);
}

async function persist() {
  if (!squad.value || !draft.name.trim() || !draft.leaderAgentId || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    if (!draft.memberAgentIds.includes(draft.leaderAgentId)) draft.memberAgentIds.push(draft.leaderAgentId);
    await store.saveSquad({
      id: squad.value.id,
      name: draft.name.trim(),
      description: draft.description,
      instructions: draft.instructions,
      leaderAgentId: draft.leaderAgentId,
      memberAgentIds: [...draft.memberAgentIds],
    });
    saved.value = true;
    window.setTimeout(() => { saved.value = false; }, 1600);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

function selectPane(next: Pane) {
  if (next === pane.value) return;
  if (instructionsDirty.value && !window.confirm(t("squad.discardInstructionsConfirm"))) return;
  if (squad.value) draft.instructions = squad.value.instructions ?? "";
  pane.value = next;
}

async function saveIdentity() {
  if (!squad.value) return;
  if (!draft.name.trim()) {
    draft.name = squad.value.name;
    return;
  }
  if (draft.name.trim() === squad.value.name && draft.description === squad.value.description) return;
  await persist();
}

async function addMember() {
  const id = addAgentId.value;
  if (!id || draft.memberAgentIds.includes(id)) return;
  draft.memberAgentIds.push(id);
  if (!draft.leaderAgentId) draft.leaderAgentId = id;
  addAgentId.value = "";
  await persist();
}

async function setLeader(agentId: string) {
  if (agentId === draft.leaderAgentId) return;
  draft.leaderAgentId = agentId;
  if (!draft.memberAgentIds.includes(agentId)) draft.memberAgentIds.push(agentId);
  await persist();
}

async function removeMember(agentId: string) {
  if (agentId === draft.leaderAgentId) {
    error.value = t("squad.removeLeaderFirst");
    return;
  }
  draft.memberAgentIds = draft.memberAgentIds.filter((id) => id !== agentId);
  await persist();
}

async function remove() {
  if (!squad.value || !window.confirm(t("squad.deleteConfirm", { name: squad.value.name }))) return;
  await store.deleteSquad(squad.value.id);
  await router.push("/squads");
}
</script>

<template>
  <div v-if="squad" class="agent-detail-page">
    <header class="agent-detail-header">
      <span class="squad-mark squad-mark-lg" aria-hidden="true"><Users :size="22" /></span>
      <div class="grow">
        <h1>{{ squad.name }}</h1>
        <p>{{ squad.description || t("squad.noDescription") }}</p>
      </div>
      <UiButton variant="ghost" size="icon" class="agent-delete" :title="t('squad.delete')" @click="remove"><Trash2 :size="15" /></UiButton>
    </header>

    <div class="squad-workbench">
      <aside class="squad-inspector">
        <h2 class="ds-overline">{{ t("squad.profile") }}</h2>
        <label>{{ t("squad.field.name") }}
          <UiInput v-model="draft.name" required @change="saveIdentity" />
        </label>
        <label>{{ t("squad.field.description") }}
          <UiTextarea v-model="draft.description" rows="4" :placeholder="t('squad.descPlaceholder')" @change="saveIdentity" />
        </label>
        <dl class="squad-detail-list">
          <div>
            <dt>{{ t("squad.leader") }}</dt>
            <dd class="squad-detail-agent">
              <AgentAvatar v-if="leader" :value="leader.avatar" :name="leader.name" :size="18" />
              {{ leader?.name ?? t("squad.unassignedLeader") }}
            </dd>
          </div>
          <div><dt>{{ t("squad.members") }}</dt><dd>{{ members.length }}</dd></div>
        </dl>
      </aside>

      <section class="squad-pane">
        <nav class="ds-tabs squad-pane-tabs" role="tablist" :aria-label="t('squad.tabsAria')">
          <button type="button" role="tab" :aria-selected="pane === 'members'" :class="{ active: pane === 'members' }" @click="selectPane('members')">
            <Users :size="14" /> {{ t("squad.members") }}<em>{{ members.length }}</em>
          </button>
          <button type="button" role="tab" :aria-selected="pane === 'instructions'" :class="{ active: pane === 'instructions' }" @click="selectPane('instructions')">
            <FileText :size="14" /> {{ t("squad.tab.instructions") }}
          </button>
        </nav>

        <div class="squad-pane-body">
          <template v-if="pane === 'members'">
            <div class="squad-pane-heading">
              <div>
                <h2>{{ t("squad.membersHeading") }}</h2>
                <p>{{ t("squad.membersHint", { n: members.length }) }}</p>
              </div>
              <UiButton size="small" @click="router.push('/agents')"><Plus :size="14" /> {{ t("squad.createAgent") }}</UiButton>
            </div>
            <div class="squad-add-row">
              <UiSelect
                v-model="addAgentId"
                :disabled="!candidates.length"
                :placeholder="candidates.length ? t('squad.pickAgentToAdd') : t('squad.allAgentsJoined')"
                :options="candidates.map((agent) => ({ value: agent.id, label: agent.name, avatar: agent.avatar }))"
              />
              <UiButton variant="primary" :disabled="busy || !addAgentId" @click="addMember"><Plus :size="14" /> {{ t("squad.addMember") }}</UiButton>
            </div>
            <article v-for="member in members" :key="member.id" class="squad-member-row">
              <AgentAvatar :value="member.avatar" :name="member.name" :size="32" />
              <div class="grow">
                <strong>
                  {{ member.name }}
                  <span v-if="member.id === draft.leaderAgentId" class="ds-badge" data-tone="warning">{{ t("squad.leader") }}</span>
                </strong>
                <small class="agent-directory-status" :data-status="memberStatusData(member)">{{ memberStatusLabel(member) }}</small>
              </div>
              <UiButton
                v-if="member.id !== draft.leaderAgentId"
                variant="ghost"
                size="icon"
                :title="t('squad.setLeader')"
                :disabled="busy"
                @click="setLeader(member.id)"
              ><Crown :size="14" /></UiButton>
              <span v-else class="squad-crown" :title="t('squad.leader')"><Crown :size="14" /></span>
              <UiButton variant="ghost" size="icon" :title="t('squad.openAgent')" @click="router.push(`/agents/${member.id}`)"><ArrowUpRight :size="14" /></UiButton>
              <UiButton
                variant="ghost"
                size="icon"
                class="squad-member-remove"
                :title="t('squad.removeMember')"
                :disabled="busy || member.id === draft.leaderAgentId"
                @click="removeMember(member.id)"
              ><Trash2 :size="14" /></UiButton>
            </article>
            <div v-if="!members.length" class="ds-empty">
              <span class="ds-empty-icon"><Users :size="18" /></span>
              <h3>{{ t("squad.emptyMembersTitle") }}</h3>
              <p>{{ t("squad.emptyMembersHint") }}</p>
            </div>
          </template>

          <template v-else>
            <div class="squad-pane-heading">
              <div>
                <h2>{{ t("squad.instructionsHeading") }}</h2>
                <p>{{ t("squad.instructionsHint") }}</p>
              </div>
              <span v-if="saved" class="save-success"><Check :size="14" /> {{ t("agent.saved") }}</span>
              <UiButton variant="primary" :class="{ 'is-loading': busy }" :disabled="busy || !instructionsDirty" @click="persist">{{ t("common.save") }}</UiButton>
            </div>
            <UiTextarea v-model="draft.instructions" rows="14" :placeholder="t('squad.instructionsPlaceholder')" />
          </template>
          <p v-if="error" class="form-error">{{ error }}</p>
        </div>
      </section>
    </div>
  </div>

  <section v-else class="page panel placeholder">
    <span class="ds-empty-icon"><Users :size="18" /></span>
    <h2>{{ t("squad.notFoundTitle") }}</h2>
    <p>{{ t("squad.notFoundHint") }}</p>
    <UiButton @click="router.push('/squads')">{{ t("squad.backToList") }}</UiButton>
  </section>
</template>
