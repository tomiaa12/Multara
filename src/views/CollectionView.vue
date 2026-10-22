<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { Boxes, LayoutGrid, Pencil, Plus, RefreshCw, Rows3, Search, Trash2, Users, X } from "@lucide/vue";
import { useRoute } from "vue-router";
import type { Agent, Runtime, Squad } from "../domain/types";
import { isMiku } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import AvatarPicker from "../components/AvatarPicker.vue";
import ProviderLogo from "../components/ProviderLogo.vue";
import RuntimeModelSelect from "../components/RuntimeModelSelect.vue";
import RuntimeSelect from "../components/RuntimeSelect.vue";
import { t } from "../i18n";
import { gatewayHost } from "./runtime-catalog";
import { useAppStore } from "../stores/app";
import CreateRuntimeDialog from "./CreateRuntimeDialog.vue";

const RUNTIME_VIEW_KEY = "multara.runtimes.view";

function agentListDescription(agent: Agent) {
  if (isMiku(agent)) return t("miku.description");
  return agent.description || t("common.noDescription");
}

const route = useRoute();
const store = useAppStore();
const kind = computed(() => String(route.meta.kind ?? ""));
const runtimeView = ref<"list" | "card">(
  localStorage.getItem(RUNTIME_VIEW_KEY) === "card" ? "card" : "list",
);

function setRuntimeView(mode: "list" | "card") {
  runtimeView.value = mode;
  localStorage.setItem(RUNTIME_VIEW_KEY, mode);
}

function syncAgentsAndSquads() {
  if (kind.value !== "agents" && kind.value !== "squads") return;
  void store.refreshAgentsAndSquads();
}

onMounted(syncAgentsAndSquads);
watch(kind, syncAgentsAndSquads);
const title = computed(() => {
  if (kind.value === "runtimes") return t("runtime.title");
  if (kind.value === "squads") return t("nav.squads");
  return t("nav.agents");
});
const subtitle = computed(() => {
  if (kind.value === "runtimes") return t("runtime.subtitle");
  if (kind.value === "squads") return t("collection.squadsHint");
  return t("collection.agentsHint");
});
const busy = ref(false);
const error = ref("");
const editor = ref<"" | "agent" | "squad">("");
const runtimeModal = ref<null | Runtime | "new">(null);
const visibleRuntimes = computed(
  () => store.data?.runtimes.filter((runtime) => runtime.status === "online" || runtime.id.startsWith("custom:")) ?? [],
);
const agentQuery = ref("");
const visibleAgents = computed(() => {
  const query = agentQuery.value.trim().toLowerCase();
  return (store.data?.agents ?? [])
    .filter((agent) => {
      if (!query) return true;
      return agent.name.toLowerCase().includes(query) || agent.description.toLowerCase().includes(query);
    })
    .slice()
    .sort((left, right) => Number(isMiku(right)) - Number(isMiku(left)));
});

const runningAgentCount = computed(
  () => (store.data?.agents ?? []).filter((agent) => agentStatusKey(agent) === "running").length,
);

function agentRuntime(agent: Agent) {
  return store.data?.runtimes.find((runtime) => runtime.id === agent.runtimeId) ?? null;
}

function agentTaskCount(agentId: string) {
  return store.data?.tasks.filter((task) => task.agentId === agentId).length ?? 0;
}

function agentStatusKey(agent: Agent): "running" | "online" | "offline" {
  const running = Object.values(store.runStates).some((event) =>
    event.agentId === agent.id
    && (event.status === "running" || event.status === "queued")
    && !event.runId.startsWith("pending:"),
  );
  if (running) return "running";
  return agentRuntime(agent)?.status === "online" ? "online" : "offline";
}

function agentStatus(agent: Agent) {
  const key = agentStatusKey(agent);
  if (key === "running") return t("collection.status.running");
  if (key === "online") return t("collection.status.online");
  return t("collection.status.offline");
}

const agentForm = reactive<Omit<Agent, "id"> & { id?: string }>({
  name: "",
  avatar: "🤖",
  description: "",
  instructions: "",
  runtimeId: "",
  model: "auto",
  maxConcurrentTasks: 1,
  skillIds: [],
  mcpIds: [],
});
const squadQuery = ref("");
const visibleSquads = computed(() => {
  const query = squadQuery.value.trim().toLowerCase();
  return (store.data?.squads ?? []).filter((squad) => {
    if (!query) return true;
    const leader = store.data?.agents.find((agent) => agent.id === squad.leaderAgentId)?.name ?? "";
    return squad.name.toLowerCase().includes(query)
      || squad.description.toLowerCase().includes(query)
      || leader.toLowerCase().includes(query);
  });
});
const squadForm = reactive<Omit<Squad, "id"> & { id?: string }>({
  name: "",
  description: "",
  instructions: "",
  leaderAgentId: "",
  memberAgentIds: [],
});
const availableAgentResources = computed(() => store.data?.resources ?? []);

function editAgent(agent?: Agent) {
  Object.assign(agentForm, agent ?? {
    id: undefined,
    name: "",
    avatar: "🤖",
    description: "",
    instructions: "",
    runtimeId: store.data?.runtimes.find((runtime) => runtime.status === "online")?.id ?? "",
    model: "auto",
    maxConcurrentTasks: 1,
    skillIds: [],
    mcpIds: [],
  });
  agentForm.skillIds = [...(agent?.skillIds ?? [])];
  agentForm.mcpIds = [...(agent?.mcpIds ?? [])];
  error.value = "";
  editor.value = "agent";
}

function editSquad(squad?: Squad) {
  const firstAgent = store.data?.agents[0]?.id ?? "";
  Object.assign(squadForm, squad ?? {
    id: undefined,
    name: "",
    description: "",
    instructions: "",
    leaderAgentId: firstAgent,
    memberAgentIds: firstAgent ? [firstAgent] : [],
  });
  squadForm.memberAgentIds = [...(squad?.memberAgentIds ?? (firstAgent ? [firstAgent] : []))];
  error.value = "";
  editor.value = "squad";
}

function editRuntime(runtime?: Runtime) {
  runtimeModal.value = runtime ?? "new";
}

async function saveEditor() {
  busy.value = true;
  error.value = "";
  try {
    if (editor.value === "agent") await store.saveAgent({ ...agentForm });
    if (editor.value === "squad") {
      const members = squadForm.memberAgentIds.includes(squadForm.leaderAgentId)
        ? squadForm.memberAgentIds
        : [...squadForm.memberAgentIds, squadForm.leaderAgentId];
      await store.saveSquad({ ...squadForm, memberAgentIds: members });
    }
    editor.value = "";
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

async function removeRuntime(runtime: Runtime) {
  if (!window.confirm(t("runtime.deleteConfirm", { name: runtime.name }))) return;
  try {
    await store.deleteCustomRuntime(runtime.id);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

async function removeAgent(agent: Agent) {
  if (isMiku(agent)) return;
  if (!window.confirm(t("collection.deleteAgentConfirm", { name: agent.name }))) return;
  try {
    await store.deleteAgent(agent.id);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

async function removeSquad(squad: Squad) {
  if (!window.confirm(t("collection.deleteSquadConfirm", { name: squad.name }))) return;
  await store.deleteSquad(squad.id);
}

async function updateRuntime(runtimeId: string, value: unknown) {
  await store.configureRuntime(runtimeId, Number(value));
}

async function setAgentRuntime(agent: Agent, runtimeId: string) {
  if (!runtimeId || runtimeId === agent.runtimeId) return;
  try {
    await store.saveAgent({ ...agent, runtimeId, model: "auto" });
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

async function setAgentModel(agent: Agent, model: string) {
  const next = model || "auto";
  if (next === (agent.model || "auto")) return;
  try {
    await store.saveAgent({ ...agent, model: next });
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

function runtimePath(runtime: Runtime) {
  if (runtime.kind === "gateway" && runtime.apiUrl) return gatewayHost(runtime.apiUrl);
  return runtime.path ?? runtime.command ?? "";
}

function runtimeSubtitle(runtime: Runtime) {
  if (runtime.id.startsWith("custom:")) {
    return runtime.provider === "custom"
      ? t("runtime.legacy")
      : t("runtime.basedOn", { name: t(`runtime.family.${runtime.provider}`) });
  }
  return runtime.version || t("runtime.unknownVersion");
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <div>
        <h1>{{ title }}</h1>
        <p>{{ subtitle }}</p>
      </div>
      <div v-if="kind === 'runtimes'" class="header-actions">
        <div v-if="visibleRuntimes.length" class="ds-segmented" role="tablist" :aria-label="t('runtime.view')">
          <button type="button" :class="{ active: runtimeView === 'list' }" :aria-selected="runtimeView === 'list'" @click="setRuntimeView('list')">
            <Rows3 :size="14" /> {{ t("runtime.view.list") }}
          </button>
          <button type="button" :class="{ active: runtimeView === 'card' }" :aria-selected="runtimeView === 'card'" @click="setRuntimeView('card')">
            <LayoutGrid :size="14" /> {{ t("runtime.view.card") }}
          </button>
        </div>
        <UiButton @click="store.refreshRuntimes"><RefreshCw :size="15" /> {{ t("runtime.redetect") }}</UiButton>
        <UiButton variant="primary" @click="editRuntime()"><Plus :size="15" /> {{ t("runtime.add") }}</UiButton>
      </div>
      <div v-else-if="kind === 'agents'" class="header-actions">
        <UiButton variant="primary" @click="editAgent()"><Plus :size="15" /> {{ t("collection.newAgent") }}</UiButton>
      </div>
      <div v-else-if="kind === 'squads'" class="header-actions">
        <UiButton variant="primary" @click="editSquad()"><Plus :size="15" /> {{ t("collection.newSquad") }}</UiButton>
      </div>
    </header>

    <div v-if="kind === 'agents' || kind === 'squads'" class="page-toolbar">
      <label class="ds-search collection-search">
        <Search :size="14" />
        <UiInput
          v-if="kind === 'agents'"
          v-model="agentQuery"
          type="search"
          :placeholder="t('collection.searchAgent')"
        />
        <UiInput v-else v-model="squadQuery" type="search" :placeholder="t('collection.searchSquad')" />
      </label>
      <span class="collection-summary">
        <template v-if="kind === 'agents'">
          {{ t("collection.agentCount", { n: store.data?.agents.length ?? 0 }) }}
          <template v-if="runningAgentCount">{{ t("collection.runningCount", { n: runningAgentCount }) }}</template>
        </template>
        <template v-else>{{ t("collection.squadCount", { n: store.data?.squads.length ?? 0 }) }}</template>
      </span>
    </div>

    <section v-if="kind === 'runtimes'" class="page-section">
      <div v-if="!visibleRuntimes.length" class="ds-card ds-empty">
        <span class="ds-empty-icon"><Boxes :size="18" /></span>
        <h3>{{ t("runtime.empty") }}</h3>
        <p>{{ t("runtime.emptyHint") }}</p>
        <UiButton @click="store.refreshRuntimes"><RefreshCw :size="15" /> {{ t("runtime.redetect") }}</UiButton>
      </div>
      <div v-else-if="runtimeView === 'list'" class="agent-directory runtime-directory">
        <div class="agent-directory-head">
          <span>{{ t("runtime.col.runtime") }}</span>
          <span>{{ t("runtime.col.status") }}</span>
          <span>{{ t("runtime.col.path") }}</span>
          <span>{{ t("runtime.col.concurrent") }}</span>
          <span />
        </div>
        <div v-for="runtime in visibleRuntimes" :key="runtime.id" class="agent-directory-row">
          <span class="agent-directory-identity">
            <span class="runtime-logo"><ProviderLogo :provider="runtime.provider" :size="20" /></span>
            <span>
              <strong>{{ runtime.name }}</strong>
              <small>{{ runtimeSubtitle(runtime) }}</small>
            </span>
          </span>
          <span class="agent-directory-status" :data-status="runtime.status === 'online' ? 'idle' : 'offline'">
            {{ runtime.status === "online" ? t("runtime.online") : t("runtime.offline") }}
          </span>
          <code class="runtime-directory-path" :title="runtime.path ?? runtime.command">
            {{ runtimePath(runtime) || t("runtime.notFound", { command: runtime.command }) }}
          </code>
          <span class="agent-directory-field" @click.stop>
            <UiSelect
              :model-value="runtime.maxConcurrent"
              size="sm"
              :options="Array.from({ length: 6 }, (_, index) => ({ value: index + 1, label: String(index + 1) }))"
              :aria-label="t('collection.maxConcurrent')"
              @change="updateRuntime(runtime.id, $event)"
            />
          </span>
          <span class="runtime-directory-actions">
            <template v-if="runtime.id.startsWith('custom:')">
              <UiButton class="icon-button" size="icon" :title="t('common.edit')" @click="editRuntime(runtime)"><Pencil :size="14" /></UiButton>
              <UiButton class="icon-button" size="icon" variant="danger" :title="t('common.delete')" @click="removeRuntime(runtime)"><Trash2 :size="14" /></UiButton>
            </template>
          </span>
        </div>
      </div>
      <div v-else class="runtime-grid">
        <article v-for="runtime in visibleRuntimes" :key="runtime.id" class="ds-card runtime-card">
          <div class="runtime-card-head">
            <span class="runtime-logo"><ProviderLogo :provider="runtime.provider" :size="20" /></span>
            <div class="runtime-card-title">
              <strong>{{ runtime.name }}</strong>
              <small>{{ runtimeSubtitle(runtime) }}</small>
              <small v-if="runtime.description">{{ runtime.description }}</small>
            </div>
            <span class="ds-badge" :data-tone="runtime.status === 'online' ? 'success' : undefined">
              {{ runtime.status === "online" ? t("runtime.online") : t("runtime.offline") }}
            </span>
          </div>
          <code v-if="runtime.kind === 'gateway' && runtime.apiUrl" class="runtime-path">{{ gatewayHost(runtime.apiUrl) }}</code>
          <code class="runtime-path" :title="runtime.path ?? runtime.command">{{ runtime.path ?? t("runtime.notFound", { command: runtime.command }) }}</code>
          <footer class="runtime-card-foot">
            <label class="runtime-concurrency">
              {{ t("collection.maxConcurrent") }}
              <UiSelect
                :model-value="runtime.maxConcurrent"
                size="sm"
                :options="Array.from({ length: 6 }, (_, index) => ({ value: index + 1, label: String(index + 1) }))"
                :aria-label="t('collection.maxConcurrent')"
                @change="updateRuntime(runtime.id, $event)"
              />
            </label>
            <div v-if="runtime.id.startsWith('custom:')" class="runtime-actions">
              <UiButton class="icon-button" size="icon" :title="t('common.edit')" @click="editRuntime(runtime)"><Pencil :size="14" /></UiButton>
              <UiButton class="icon-button" size="icon" variant="danger" :title="t('common.delete')" @click="removeRuntime(runtime)"><Trash2 :size="14" /></UiButton>
            </div>
          </footer>
        </article>
      </div>
    </section>

    <div v-else-if="kind === 'agents'" class="agent-directory">
      <div class="agent-directory-head">
        <span>{{ t("collection.col.agent") }}</span>
        <span>{{ t("collection.col.runtime") }}</span>
        <span>{{ t("collection.col.model") }}</span>
        <span>{{ t("collection.col.status") }}</span>
        <span>{{ t("collection.col.tasks") }}</span>
        <span />
      </div>
      <p v-if="!(store.data?.agents.length)" class="empty-copy">{{ t("collection.emptyAgents") }}</p>
      <p v-else-if="!visibleAgents.length" class="empty-copy">{{ t("collection.noMatchAgent", { query: agentQuery }) }}</p>
      <div v-for="agent in visibleAgents" :key="agent.id" class="agent-directory-row">
        <RouterLink :to="`/agents/${agent.id}`" class="agent-directory-hit">
          <span class="agent-directory-identity">
            <AgentAvatar :value="agent.avatar" :name="agent.name" :size="32" />
            <span>
              <strong>{{ agent.name }} <span v-if="isMiku(agent)" class="tag">{{ t("collection.steward") }}</span></strong>
              <small>{{ agentListDescription(agent) }}</small>
            </span>
          </span>
        </RouterLink>
        <span class="agent-directory-field" @click.stop @mousedown.stop>
          <RuntimeSelect
            size="sm"
            :model-value="agent.runtimeId"
            :online-only="false"
            :placeholder="t('collection.pickRuntime')"
            @update:model-value="setAgentRuntime(agent, $event ?? '')"
          />
        </span>
        <span class="agent-directory-field" @click.stop @mousedown.stop>
          <RuntimeModelSelect
            compact
            size="sm"
            :model-value="agent.model || 'auto'"
            :runtime-id="agent.runtimeId"
            @update:model-value="setAgentModel(agent, $event)"
          />
        </span>
        <span class="agent-directory-status" :data-status="agentStatusKey(agent)">{{ agentStatus(agent) }}</span>
        <RouterLink :to="`/agents/${agent.id}`" class="agent-directory-hit">
          <span>{{ agentTaskCount(agent.id) }}</span>
        </RouterLink>
        <UiButton v-if="!isMiku(agent)" class="icon-button" :title="t('collection.deleteAgent')" @click.stop="removeAgent(agent)"><Trash2 :size="13" /></UiButton>
        <span v-else />
      </div>
    </div>

    <div v-else-if="kind === 'squads'" class="agent-directory squad-directory">
      <div class="agent-directory-head">
        <span>{{ t("collection.col.squad") }}</span>
        <span>{{ t("collection.col.leader") }}</span>
        <span>{{ t("collection.col.members") }}</span>
        <span />
      </div>
      <p v-if="!(store.data?.squads.length)" class="empty-copy">{{ t("collection.emptySquads") }}</p>
      <p v-else-if="!visibleSquads.length" class="empty-copy">{{ t("collection.noMatchSquad", { query: squadQuery }) }}</p>
      <div v-for="squad in visibleSquads" :key="squad.id" class="agent-directory-row">
        <RouterLink :to="`/squads/${squad.id}`" class="agent-directory-hit">
          <span class="agent-directory-identity">
            <span class="squad-mark" aria-hidden="true"><Users :size="16" /></span>
            <span>
              <strong>{{ squad.name }}</strong>
              <small>{{ squad.description || t("common.noDescription") }}</small>
            </span>
          </span>
          <span class="squad-leader-cell">
            <AgentAvatar
              :value="store.data?.agents.find((agent) => agent.id === squad.leaderAgentId)?.avatar"
              :name="store.data?.agents.find((agent) => agent.id === squad.leaderAgentId)?.name"
              :size="22"
            />
            <span>{{ store.data?.agents.find((agent) => agent.id === squad.leaderAgentId)?.name ?? t("collection.unspecified") }}</span>
          </span>
          <span class="squad-member-stack">
            <AgentAvatar
              v-for="id in squad.memberAgentIds.slice(0, 4)"
              :key="id"
              :value="store.data?.agents.find((agent) => agent.id === id)?.avatar"
              :name="store.data?.agents.find((agent) => agent.id === id)?.name"
              :size="22"
            />
            <small v-if="squad.memberAgentIds.length > 4">+{{ squad.memberAgentIds.length - 4 }}</small>
            <small v-else-if="!squad.memberAgentIds.length">{{ t("collection.noMembers") }}</small>
          </span>
        </RouterLink>
        <UiButton class="icon-button" :title="t('collection.deleteSquad')" @click.stop="removeSquad(squad)"><Trash2 :size="13" /></UiButton>
      </div>
    </div>

    <div v-else-if="kind === 'stats'" class="ds-stat-grid">
      <div class="ds-stat"><span>{{ t("collection.stat.completedToday") }}</span><strong>{{ store.data?.stats.completedToday ?? 0 }}</strong></div>
      <div class="ds-stat"><span>{{ t("collection.stat.running") }}</span><strong>{{ store.data?.stats.running ?? 0 }}</strong></div>
      <div class="ds-stat"><span>{{ t("collection.stat.queued") }}</span><strong>{{ store.data?.stats.queued ?? 0 }}</strong></div>
      <div class="ds-stat"><span>{{ t("collection.stat.failedToday") }}</span><strong>{{ store.data?.stats.failedToday ?? 0 }}</strong></div>
    </div>

    <section v-else class="ds-card ds-empty">
      <h3>{{ t("collection.skeletonTitle") }}</h3>
      <p>{{ t("collection.skeletonHint") }}</p>
    </section>

    <div v-if="editor" class="modal-backdrop" @click.self="editor = ''">
      <form class="modal domain-modal" @submit.prevent="saveEditor">
        <header class="modal-heading">
          <h2 v-if="editor === 'agent'">{{ agentForm.id ? t("collection.editAgent") : t("collection.newAgent") }}</h2>
          <h2 v-else>{{ squadForm.id ? t("collection.editSquad") : t("collection.newSquad") }}</h2>
          <UiButton class="icon-button" @click="editor = ''"><X :size="16" /></UiButton>
        </header>
        <template v-if="editor === 'agent'">
          <span class="ds-overline">{{ t("collection.basicInfo") }}</span>
          <AvatarPicker
            v-model="agentForm.avatar"
            :name="agentForm.name"
            :locked="Boolean(agentForm.id && (isMiku({ id: agentForm.id, systemKey: agentForm.systemKey }) || agentForm.id === 'miku'))"
            @error="error = $event"
          />
          <label>{{ t("collection.field.name") }}<UiInput v-model="agentForm.name" required :placeholder="t('collection.namePlaceholder')" /></label>
          <label>{{ t("collection.field.description") }}<UiTextarea v-model="agentForm.description" rows="2" :placeholder="t('collection.descPlaceholder')" /></label>
          <label>{{ t("collection.field.instructions") }}<UiTextarea v-model="agentForm.instructions" rows="4" :placeholder="t('collection.instructionsPlaceholder')" /></label>
          <span class="ds-overline">{{ t("collection.runConfig") }}</span>
          <div class="form-row">
            <label>{{ t("collection.col.runtime") }}
              <RuntimeSelect
                v-model="agentForm.runtimeId"
                required
                :placeholder="t('collection.pickRuntime')"
                @update:model-value="agentForm.model = 'auto'"
              />
            </label>
            <label>{{ t("collection.field.model") }}<RuntimeModelSelect v-model="agentForm.model" :runtime-id="agentForm.runtimeId" /></label>
            <label>{{ t("collection.field.concurrency") }}
              <UiSelect v-model="agentForm.maxConcurrentTasks" :options="Array.from({ length: 6 }, (_, index) => ({ value: index + 1, label: String(index + 1) }))" />
            </label>
          </div>
          <span class="ds-overline">{{ t("collection.permissions") }}</span>
          <fieldset>
            <legend>{{ t("collection.availableSkills") }}</legend>
            <label v-for="resource in availableAgentResources.filter((item) => item.kind === 'skill')" :key="resource.id" class="check-row">
              <UiInput v-model="agentForm.skillIds" type="checkbox" :value="resource.id" />{{ resource.name }} <small>{{ resource.sourceLabel || resource.owner }}</small>
            </label>
            <small v-if="!availableAgentResources.some((item) => item.kind === 'skill')">{{ t("collection.noSkills") }}</small>
          </fieldset>
          <fieldset>
            <legend>{{ t("collection.availableMcp") }}</legend>
            <label v-for="resource in availableAgentResources.filter((item) => item.kind === 'mcp')" :key="resource.id" class="check-row">
              <UiInput v-model="agentForm.mcpIds" type="checkbox" :value="resource.id" />{{ resource.name }} <small>{{ resource.sourceLabel || resource.owner }}</small>
            </label>
            <small v-if="!availableAgentResources.some((item) => item.kind === 'mcp')">{{ t("collection.noMcp") }}</small>
          </fieldset>
        </template>
        <template v-else-if="editor === 'squad'">
          <label>{{ t("collection.squadName") }}<UiInput v-model="squadForm.name" required :placeholder="t('collection.squadNamePlaceholder')" /></label>
          <label>{{ t("collection.field.description") }}<UiTextarea v-model="squadForm.description" rows="3" /></label>
          <label>{{ t("collection.leader") }}
            <UiSelect
              v-model="squadForm.leaderAgentId"
              required
              :placeholder="t('collection.pickLeader')"
              :options="(store.data?.agents ?? []).map((agent) => ({ value: agent.id, label: agent.name, avatar: agent.avatar }))"
            />
          </label>
          <fieldset>
            <legend>{{ t("collection.members") }}</legend>
            <label v-for="agent in store.data?.agents" :key="agent.id" class="check-row">
              <UiInput v-model="squadForm.memberAgentIds" type="checkbox" :value="agent.id" />
              <AgentAvatar :value="agent.avatar" :name="agent.name" :size="20" />
              <span>{{ agent.name }}</span>
            </label>
          </fieldset>
        </template>
        <p v-if="error" class="form-error">{{ error }}</p>
        <div class="modal-actions">
          <UiButton @click="editor = ''">{{ t("common.cancel") }}</UiButton>
          <UiButton variant="primary" type="submit" :class="{ 'is-loading': busy }" :disabled="busy">{{ busy ? t("common.saving") : t("common.save") }}</UiButton>
        </div>
      </form>
    </div>
    <CreateRuntimeDialog
      v-if="runtimeModal !== null"
      :runtime="runtimeModal === 'new' ? null : runtimeModal"
      @close="runtimeModal = null"
    />
  </div>
</template>
