<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { ArrowUpRight, Check, Crown, FileText, Plus, Trash2, Users } from "@lucide/vue";
import { useRoute, useRouter } from "vue-router";
import type { Agent, Squad } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import { useAppStore } from "../stores/app";

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

function memberStatus(agent: Agent) {
  const running = Object.values(store.runStates).some((event) =>
    event.agentId === agent.id
    && (event.status === "running" || event.status === "queued")
    && !event.runId.startsWith("pending:"),
  );
  if (running) return "工作中";
  const runtime = store.data?.runtimes.find((item) => item.id === agent.runtimeId);
  return runtime?.status === "online" ? "空闲" : "离线";
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
  if (instructionsDirty.value && !window.confirm("小队指引有未保存的修改，确定放弃？")) return;
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
    error.value = "请先指定其他队长，再移出当前队长。";
    return;
  }
  draft.memberAgentIds = draft.memberAgentIds.filter((id) => id !== agentId);
  await persist();
}

async function remove() {
  if (!squad.value || !window.confirm(`确认删除小队“${squad.value.name}”？`)) return;
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
        <p>{{ squad.description || "还没有小队描述" }}</p>
      </div>
      <UiButton variant="ghost" size="icon" class="agent-delete" title="删除小队" @click="remove"><Trash2 :size="15" /></UiButton>
    </header>

    <div class="squad-workbench">
      <aside class="squad-inspector">
        <h2 class="ds-overline">小队资料</h2>
        <label>名称
          <UiInput v-model="draft.name" required @change="saveIdentity" />
        </label>
        <label>描述
          <UiTextarea v-model="draft.description" rows="4" placeholder="说明小队负责的工作" @change="saveIdentity" />
        </label>
        <dl class="squad-detail-list">
          <div>
            <dt>队长</dt>
            <dd class="squad-detail-agent">
              <AgentAvatar v-if="leader" :value="leader.avatar" :name="leader.name" :size="18" />
              {{ leader?.name ?? "未指定" }}
            </dd>
          </div>
          <div><dt>成员</dt><dd>{{ members.length }}</dd></div>
        </dl>
      </aside>

      <section class="squad-pane">
        <nav class="ds-tabs squad-pane-tabs" role="tablist" aria-label="小队内容">
          <button type="button" role="tab" :aria-selected="pane === 'members'" :class="{ active: pane === 'members' }" @click="selectPane('members')">
            <Users :size="14" /> 成员<em>{{ members.length }}</em>
          </button>
          <button type="button" role="tab" :aria-selected="pane === 'instructions'" :class="{ active: pane === 'instructions' }" @click="selectPane('instructions')">
            <FileText :size="14" /> 指引
          </button>
        </nav>

        <div class="squad-pane-body">
          <template v-if="pane === 'members'">
            <div class="squad-pane-heading">
              <div>
                <h2>成员</h2>
                <p>{{ members.length }} 名成员。队长负责拆分并分配任务。</p>
              </div>
              <UiButton size="small" @click="router.push('/agents')"><Plus :size="14" /> 创建智能体</UiButton>
            </div>
            <div class="squad-add-row">
              <UiSelect
                v-model="addAgentId"
                :disabled="!candidates.length"
                :placeholder="candidates.length ? '选择要加入的智能体' : '所有智能体都已加入'"
                :options="candidates.map((agent) => ({ value: agent.id, label: agent.name, avatar: agent.avatar }))"
              />
              <UiButton variant="primary" :disabled="busy || !addAgentId" @click="addMember"><Plus :size="14" /> 添加成员</UiButton>
            </div>
            <article v-for="member in members" :key="member.id" class="squad-member-row">
              <AgentAvatar :value="member.avatar" :name="member.name" :size="32" />
              <div class="grow">
                <strong>
                  {{ member.name }}
                  <span v-if="member.id === draft.leaderAgentId" class="ds-badge" data-tone="warning">队长</span>
                </strong>
                <small class="agent-directory-status" :data-status="memberStatus(member) === '工作中' ? '运行中' : memberStatus(member)">{{ memberStatus(member) }}</small>
              </div>
              <UiButton
                v-if="member.id !== draft.leaderAgentId"
                variant="ghost"
                size="icon"
                title="设为小队负责人"
                :disabled="busy"
                @click="setLeader(member.id)"
              ><Crown :size="14" /></UiButton>
              <span v-else class="squad-crown" title="队长"><Crown :size="14" /></span>
              <UiButton variant="ghost" size="icon" title="打开智能体" @click="router.push(`/agents/${member.id}`)"><ArrowUpRight :size="14" /></UiButton>
              <UiButton
                variant="ghost"
                size="icon"
                class="squad-member-remove"
                title="从小队移除"
                :disabled="busy || member.id === draft.leaderAgentId"
                @click="removeMember(member.id)"
              ><Trash2 :size="14" /></UiButton>
            </article>
            <div v-if="!members.length" class="ds-empty">
              <span class="ds-empty-icon"><Users :size="18" /></span>
              <h3>还没有成员</h3>
              <p>先添加一名智能体，并指定队长。</p>
            </div>
          </template>

          <template v-else>
            <div class="squad-pane-heading">
              <div>
                <h2>指引</h2>
                <p>小队指引会在队长智能体处理分配给该小队的任务时注入到它的 prompt 中。</p>
              </div>
              <span v-if="saved" class="save-success"><Check :size="14" /> 已保存</span>
              <UiButton variant="primary" :class="{ 'is-loading': busy }" :disabled="busy || !instructionsDirty" @click="persist">保存</UiButton>
            </div>
            <UiTextarea v-model="draft.instructions" rows="14" placeholder="例如：优先把界面工作分给前端智能体，把接口改动分给后端智能体。" />
          </template>
          <p v-if="error" class="form-error">{{ error }}</p>
        </div>
      </section>
    </div>
  </div>

  <section v-else class="page panel placeholder">
    <span class="ds-empty-icon"><Users :size="18" /></span>
    <h2>未找到小队</h2>
    <p>该小队可能已被删除。</p>
    <UiButton @click="router.push('/squads')">返回小队列表</UiButton>
  </section>
</template>
