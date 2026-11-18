<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronDown, ChevronRight, Search, Trash2 } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import { NODE_KIND_META, kindLabelKey } from "./node-meta";
import { useAppStore } from "../../stores/app";
import { useWorkflowsStore } from "../../stores/workflows";
import { t } from "../../i18n";
import { PALETTE_KINDS, type NodeTemplate } from "../../domain/workflow";

export type PalettePayload =
  | { type: "kind"; kind: string }
  | { type: "template"; id: string }
  | { type: "agent"; agentType: "agent" | "squad"; id: string };

const emit = defineEmits<{ add: [payload: PalettePayload]; "delete-template": [template: NodeTemplate] }>();

const app = useAppStore();
const flows = useWorkflowsStore();
const query = ref("");
const collapsed = ref<Record<string, boolean>>({});

function toggle(key: string) {
  collapsed.value[key] = !collapsed.value[key];
}

function matches(text: string) {
  const q = query.value.trim().toLowerCase();
  return !q || text.toLowerCase().includes(q);
}

const kinds = computed(() => PALETTE_KINDS.filter((kind) => matches(`${kind} ${t(kindLabelKey(kind))}`)));
const builtin = computed(() => flows.nodeTemplates.filter((item) => item.builtin && matches(`${item.name} ${item.description}`)));
const mine = computed(() => flows.nodeTemplates.filter((item) => !item.builtin && matches(`${item.name} ${item.description}`)));
const agents = computed(() => (app.data?.agents ?? []).filter((agent) => matches(agent.name)));
const squads = computed(() => (app.data?.squads ?? []).filter((squad) => matches(squad.name)));

function leaderAvatar(leaderId: string) {
  return app.data?.agents.find((agent) => agent.id === leaderId)?.avatar ?? "◉";
}

function dragStart(event: DragEvent, payload: PalettePayload) {
  event.dataTransfer?.setData("application/x-multara-workflow", JSON.stringify(payload));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "copy";
}
</script>

<template>
  <aside class="palette">
    <div class="palette-search">
      <Search :size="13" />
      <input v-model="query" class="palette-input" :placeholder="t('workflow.palette.search')" />
    </div>
    <div class="palette-scroll vs-scroll">
      <section class="pal-group">
        <button type="button" class="pal-title" @click="toggle('kinds')">
          <component :is="collapsed.kinds ? ChevronRight : ChevronDown" :size="12" />{{ t("workflow.palette.base") }}
        </button>
        <div v-if="!collapsed.kinds" class="pal-items">
          <div
            v-for="kind in kinds"
            :key="kind"
            class="pal-item"
            draggable="true"
            :title="t('workflow.palette.dragHint')"
            @dragstart="dragStart($event, { type: 'kind', kind })"
            @dblclick="emit('add', { type: 'kind', kind })"
          >
            <component :is="NODE_KIND_META[kind].icon" :size="14" :style="{ color: NODE_KIND_META[kind].color }" />
            <span>{{ t(kindLabelKey(kind)) }}</span>
          </div>
        </div>
      </section>

      <section v-if="builtin.length" class="pal-group">
        <button type="button" class="pal-title" @click="toggle('builtin')">
          <component :is="collapsed.builtin ? ChevronRight : ChevronDown" :size="12" />{{ t("workflow.palette.builtin") }}
        </button>
        <div v-if="!collapsed.builtin" class="pal-items">
          <div
            v-for="item in builtin"
            :key="item.id"
            class="pal-item tall"
            draggable="true"
            :title="item.description"
            @dragstart="dragStart($event, { type: 'template', id: item.id })"
            @dblclick="emit('add', { type: 'template', id: item.id })"
          >
            <div class="pal-name">{{ item.name }}<span v-if="item.nodes.length > 1" class="pal-count">×{{ item.nodes.length }}</span></div>
            <small>{{ item.description }}</small>
          </div>
        </div>
      </section>

      <section class="pal-group">
        <button type="button" class="pal-title" @click="toggle('mine')">
          <component :is="collapsed.mine ? ChevronRight : ChevronDown" :size="12" />{{ t("workflow.palette.mine") }}
        </button>
        <div v-if="!collapsed.mine" class="pal-items">
          <div v-if="!mine.length" class="pal-empty">{{ t("workflow.palette.mineEmpty") }}</div>
          <div
            v-for="item in mine"
            :key="item.id"
            class="pal-item tall"
            draggable="true"
            :title="item.description"
            @dragstart="dragStart($event, { type: 'template', id: item.id })"
            @dblclick="emit('add', { type: 'template', id: item.id })"
          >
            <div class="pal-name">{{ item.name }}<span v-if="item.nodes.length > 1" class="pal-count">×{{ item.nodes.length }}</span></div>
            <button type="button" class="pal-del" :title="t('common.delete')" @click.stop="emit('delete-template', item)"><Trash2 :size="12" /></button>
          </div>
        </div>
      </section>

      <section v-if="agents.length || squads.length" class="pal-group">
        <button type="button" class="pal-title" @click="toggle('agents')">
          <component :is="collapsed.agents ? ChevronRight : ChevronDown" :size="12" />{{ t("workflow.palette.agents") }}
        </button>
        <div v-if="!collapsed.agents" class="pal-items">
          <div
            v-for="agent in agents"
            :key="agent.id"
            class="pal-item"
            draggable="true"
            @dragstart="dragStart($event, { type: 'agent', agentType: 'agent', id: agent.id })"
            @dblclick="emit('add', { type: 'agent', agentType: 'agent', id: agent.id })"
          >
            <AgentAvatar :value="agent.avatar" :name="agent.name" :size="18" /><span>{{ agent.name }}</span>
          </div>
          <div
            v-for="squad in squads"
            :key="squad.id"
            class="pal-item"
            draggable="true"
            @dragstart="dragStart($event, { type: 'agent', agentType: 'squad', id: squad.id })"
            @dblclick="emit('add', { type: 'agent', agentType: 'squad', id: squad.id })"
          >
            <AgentAvatar :value="leaderAvatar(squad.leaderAgentId)" :name="squad.name" :size="18" /><span>{{ squad.name }}</span>
          </div>
        </div>
      </section>
    </div>
  </aside>
</template>

<style scoped>
.palette { display: flex; flex-direction: column; width: 216px; flex: none; min-height: 0; background: var(--color-surface); border-right: 1px solid var(--color-border); }
.palette-search { display: flex; align-items: center; gap: var(--spacing-1-5); margin: var(--spacing-2); padding: 0 var(--spacing-2); height: var(--control-height-sm); border: 1px solid var(--color-field-border); border-radius: var(--radius-md); background: var(--color-field); color: var(--color-text-tertiary); }
.palette-input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--color-text); font-size: var(--font-size-sm); }
.palette-scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 0 var(--spacing-2) var(--spacing-3); }
.pal-group { display: grid; gap: var(--spacing-1); margin-bottom: var(--spacing-2); }
.pal-title { display: flex; align-items: center; gap: var(--spacing-1); padding: var(--spacing-1) 0; border: 0; background: transparent; color: var(--color-text-tertiary); font-size: var(--font-size-xs); letter-spacing: var(--letter-spacing-caps); text-transform: uppercase; cursor: pointer; }
.pal-items { display: grid; gap: var(--spacing-1); }
.pal-item { position: relative; display: flex; align-items: center; gap: var(--spacing-2); min-height: 28px; padding: 0 var(--spacing-2); border: 1px solid var(--color-border); border-radius: var(--radius-md); background: var(--color-surface-raised); font-size: var(--font-size-sm); color: var(--color-text); cursor: grab; user-select: none; }
.pal-item:hover { background: var(--color-surface-hover); border-color: var(--color-border-strong); }
.pal-item.tall { flex-direction: column; align-items: flex-start; gap: 2px; padding: var(--spacing-1-5) var(--spacing-2); }
.pal-item small { color: var(--color-text-tertiary); font-size: var(--font-size-xs); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.pal-name { display: flex; gap: var(--spacing-1-5); align-items: center; }
.pal-count { color: var(--color-text-tertiary); font-size: var(--font-size-xs); }
.pal-del { position: absolute; top: 4px; right: 4px; border: 0; background: transparent; color: var(--color-text-tertiary); cursor: pointer; display: none; }
.pal-item:hover .pal-del { display: inline-flex; }
.pal-del:hover { color: var(--color-error); }
.pal-empty { font-size: var(--font-size-xs); color: var(--color-text-tertiary); padding: var(--spacing-1) 0; }
</style>
