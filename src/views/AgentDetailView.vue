<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { BarChart3, Check, ListTodo, Save, Trash2, Wrench } from "@lucide/vue";
import { useRoute, useRouter } from "vue-router";
import type { Agent, AgentActivityRecord, RunStatistics, UsageDay } from "../domain/types";
import { isMiku } from "../domain/types";
import AgentAvatar from "../components/AgentAvatar.vue";
import AvatarPicker from "../components/AvatarPicker.vue";
import ProviderLogo from "../components/ProviderLogo.vue";
import RuntimeModelSelect from "../components/RuntimeModelSelect.vue";
import RuntimeSelect from "../components/RuntimeSelect.vue";
import { agentSystemContext, getRunStatistics, listAgentRuns, type AgentSystemContext } from "../services/backend";
import { useAppStore } from "../stores/app";
import { relativeTime, taskStatusLabel } from "./task-board";
import { emptyUsageDay, formatDuration, formatTokens, formatUsd, totalTokens } from "./usage-format";

type DetailSection = "overview" | "tasks" | "stats" | "capabilities" | "settings";
type CapabilityTab = "instructions" | "skills" | "mcp";

const SECTION_TABS: Array<{ id: DetailSection; label: string }> = [
  { id: "overview", label: "概览" },
  { id: "tasks", label: "任务" },
  { id: "stats", label: "统计" },
  { id: "capabilities", label: "能力" },
  { id: "settings", label: "设置" },
];
const STAT_RANGES = [
  { days: "7", label: "7 天" },
  { days: "30", label: "30 天" },
  { days: "90", label: "90 天" },
] as const;

const route = useRoute();
const router = useRouter();
const store = useAppStore();
const section = ref<DetailSection>("overview");
const capabilityTab = ref<CapabilityTab>("instructions");
const busy = ref(false);
const saved = ref(false);
const error = ref("");
const runs = ref<AgentActivityRecord[]>([]);
const stats = ref<RunStatistics | null>(null);
const statsRange = ref("30");
const statsError = ref("");
const agent = computed(() => store.data?.agents.find((item) => item.id === String(route.params.agentId)) ?? null);
const isChief = computed(() => agent.value ? isMiku(agent.value) : false);
const runtime = computed(() => store.data?.runtimes.find((item) => item.id === agent.value?.runtimeId) ?? null);
const tasks = computed(() =>
  (store.data?.tasks ?? [])
    .filter((task) => task.agentId === agent.value?.id)
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
);
const skills = computed(() => store.data?.resources.filter((item) => item.kind === "skill") ?? []);
const mcps = computed(() => store.data?.resources.filter((item) => item.kind === "mcp") ?? []);
const selectedSkills = computed(() => skills.value.filter((item) => draft.skillIds.includes(item.id)));
const selectedMcps = computed(() => mcps.value.filter((item) => draft.mcpIds.includes(item.id)));
type ResourceFilter = "all" | "on" | "off";
const resourceFilter = ref<ResourceFilter>("all");
const resourceQuery = ref("");
const systemContext = ref<AgentSystemContext | null>(null);
const enabledIds = computed(() => (capabilityTab.value === "skills" ? draft.skillIds : draft.mcpIds));
const tabResources = computed(() => (capabilityTab.value === "skills" ? skills.value : mcps.value));
/** 内置 skill 对所有智能体始终生效，计入「已开启」，不参与开关 */
const builtinSkills = computed(() => (capabilityTab.value === "skills" ? systemContext.value?.builtinSkills ?? [] : []));
const resourceCounts = computed(() => {
  const on = tabResources.value.filter((item) => enabledIds.value.includes(item.id)).length + builtinSkills.value.length;
  const total = tabResources.value.length + builtinSkills.value.length;
  return { all: total, on, off: total - on };
});

function matchesQuery(...texts: Array<string | undefined>) {
  const query = resourceQuery.value.trim().toLowerCase();
  return !query || texts.some((text) => text?.toLowerCase().includes(query));
}

/** needsAuth 表示配置要求 OAuth；是否已授权看 configStatus */
function mcpAuthLabel(resource: { needsAuth: boolean; configStatus: string }) {
  if (resource.configStatus === "needs_auth") return "需要授权";
  if (resource.needsAuth) return "已授权";
  return "不需要授权";
}

const visibleResources = computed(() =>
  tabResources.value.filter((item) => {
    const on = enabledIds.value.includes(item.id);
    if (resourceFilter.value === "on" && !on) return false;
    if (resourceFilter.value === "off" && on) return false;
    return matchesQuery(item.name, item.description, item.summary, item.sourceLabel);
  }),
);
const visibleBuiltinSkills = computed(() =>
  resourceFilter.value === "off" ? [] : builtinSkills.value.filter((item) => matchesQuery(item.name, item.description)),
);

let resourceSave: Promise<void> = Promise.resolve();

/** Skill / MCP 开关即时保存；只提交授权列表，其余字段沿用已保存的值，不带上其它分区未保存的草稿 */
function toggleResource(id: string, on: boolean) {
  const key = capabilityTab.value === "skills" ? "skillIds" : "mcpIds";
  draft[key] = on ? [...new Set([...draft[key], id])] : draft[key].filter((item) => item !== id);
  const skillIds = [...draft.skillIds];
  const mcpIds = [...draft.mcpIds];
  // 串行保存，避免连续点击时后一次被前一次的结果覆盖
  resourceSave = resourceSave.then(async () => {
    const current = agent.value;
    if (!current) return;
    error.value = "";
    try {
      await store.saveAgent({
        id: current.id,
        name: current.name,
        avatar: current.avatar,
        description: current.description,
        instructions: current.instructions,
        runtimeId: current.runtimeId,
        model: current.model,
        maxConcurrentTasks: current.maxConcurrentTasks,
        skillIds,
        mcpIds,
      });
      saved.value = true;
      window.setTimeout(() => { saved.value = false; }, 1800);
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : String(cause);
      draft.skillIds = [...(agent.value?.skillIds ?? [])];
      draft.mcpIds = [...(agent.value?.mcpIds ?? [])];
    }
  });
}

watch(() => agent.value?.id, (id) => {
  systemContext.value = null;
  if (!id) return;
  void agentSystemContext(id).then((context) => {
    if (agent.value?.id === id) systemContext.value = context;
  }).catch(() => undefined);
}, { immediate: true });

const activeRuns = computed(() => runs.value.filter((item) => item.status === "running" || item.status === "queued"));
const recentRuns = computed(() => runs.value.filter((item) => item.status !== "running" && item.status !== "queued"));

const draft = reactive<Omit<Agent, "id">>({
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

watch(agent, (value) => {
  if (!value) return;
  Object.assign(draft, {
    name: value.name,
    avatar: value.avatar,
    description: value.description,
    instructions: value.instructions,
    runtimeId: value.runtimeId,
    model: value.model,
    maxConcurrentTasks: value.maxConcurrentTasks,
    skillIds: [...value.skillIds],
    mcpIds: [...value.mcpIds],
  });
}, { immediate: true });

watch(() => agent.value?.id, (id) => {
  runs.value = [];
  if (!id) return;
  void listAgentRuns(id).then((items) => {
    if (agent.value?.id === id) runs.value = items;
  }).catch(() => {
    if (agent.value?.id === id) runs.value = [];
  });
}, { immediate: true });

watch([() => agent.value?.id, statsRange], ([id]) => {
  stats.value = null;
  statsError.value = "";
  if (!id) return;
  void getRunStatistics(Number(statsRange.value) || 30, "", id).then((value) => {
    if (agent.value?.id === id) stats.value = value;
  }).catch((cause) => {
    if (agent.value?.id === id) {
      statsError.value = cause instanceof Error ? cause.message : String(cause);
    }
  });
}, { immediate: true });

function isoDate(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

const statsDays = computed(() => {
  const map = new Map((stats.value?.daily ?? []).map((item) => [item.date.slice(0, 10), item]));
  const today = new Date();
  const span = Number(statsRange.value) || 30;
  const out: UsageDay[] = [];
  for (let offset = span - 1; offset >= 0; offset -= 1) {
    const date = new Date(today.getFullYear(), today.getMonth(), today.getDate() - offset);
    const key = isoDate(date);
    out.push(map.get(key) ?? emptyUsageDay(key));
  }
  return out;
});

const statsChartMax = computed(() => Math.max(1, ...statsDays.value.map((item) => totalTokens(item) || item.total)));

function statsBarHeight(item: UsageDay): string {
  const value = totalTokens(item) || item.total;
  if (value <= 0) return "0%";
  return `${Math.max(4, (value / statsChartMax.value) * 100)}%`;
}

async function save() {
  if (!agent.value || !draft.name.trim() || busy.value) return;
  busy.value = true;
  saved.value = false;
  error.value = "";
  try {
    await store.saveAgent({
      id: agent.value.id,
      ...draft,
      skillIds: [...draft.skillIds],
      mcpIds: [...draft.mcpIds],
    });
    saved.value = true;
    window.setTimeout(() => { saved.value = false; }, 1800);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

async function remove() {
  if (!agent.value || !window.confirm(`确认删除智能体“${agent.value.name}”？`)) return;
  await store.deleteAgent(agent.value.id);
  await router.push("/agents");
}

function runStatusLabel(status: string) {
  if (status === "running") return "运行中";
  if (status === "queued") return "排队中";
  if (status === "completed") return "已完成";
  if (status === "failed") return "失败";
  if (status === "stopped") return "已停止";
  return status;
}

function runTitle(run: AgentActivityRecord) {
  const line = run.assignment.split("\n").map((item) => item.trim()).find(Boolean);
  return line || run.sessionTitle || "一次运行";
}

function openRun(run: AgentActivityRecord) {
  if (run.taskId) void router.push(`/tasks/${run.taskId}`);
  else if (run.projectId) void router.push(`/projects/${run.projectId}`);
}
</script>

<template>
  <div v-if="agent" class="agent-detail-page">
    <header class="agent-detail-header">
      <AgentAvatar :value="agent.avatar" :name="agent.name" :size="40" />
      <div class="grow">
        <div class="agent-title-line">
          <h1>{{ agent.name }}</h1>
          <span v-if="isChief" class="ds-badge" data-tone="purple">总管</span>
          <span class="ds-badge" :data-tone="runtime?.status === 'online' ? 'success' : 'warning'"><i class="ds-dot" />{{ runtime?.status === "online" ? "可用" : "运行时离线" }}</span>
        </div>
        <p>{{ agent.description || "还没有智能体描述" }}</p>
      </div>
      <UiButton v-if="!isChief" variant="ghost" size="icon" class="agent-delete" title="删除智能体" @click="remove"><Trash2 :size="15" /></UiButton>
    </header>

    <nav class="ds-tabs agent-detail-tabs" role="tablist" aria-label="智能体页面">
      <button
        v-for="tab in SECTION_TABS"
        :key="tab.id"
        type="button"
        role="tab"
        :aria-selected="section === tab.id"
        :class="{ active: section === tab.id }"
        @click="section = tab.id"
      >
        {{ tab.label }}<em v-if="tab.id === 'tasks' && tasks.length">{{ tasks.length }}</em>
      </button>
    </nav>

    <div v-if="section === 'overview'" class="agent-detail-body vs-scroll">
      <div class="agent-overview">
        <div class="agent-run-column">
          <section>
            <header>
              <h2>正在运行</h2>
              <p v-if="!activeRuns.length">现在没有进行中的运行。</p>
            </header>
            <div v-if="activeRuns.length" class="agent-run-list">
              <button v-for="run in activeRuns" :key="run.id" type="button" class="agent-run-row" @click="openRun(run)">
                <span class="agent-run-mark" :data-status="run.status" />
                <span>
                  <strong>{{ runTitle(run) }}</strong>
                  <small>{{ runStatusLabel(run.status) }} · {{ relativeTime(run.createdAt) }}</small>
                </span>
              </button>
            </div>
          </section>
          <section>
            <header>
              <h2>最近运行</h2>
              <p v-if="!recentRuns.length">还没有结束的运行记录。</p>
            </header>
            <div v-if="recentRuns.length" class="agent-run-list">
              <button v-for="run in recentRuns" :key="run.id" type="button" class="agent-run-row" @click="openRun(run)">
                <span class="agent-run-mark" :data-status="run.status" />
                <span>
                  <strong>{{ runTitle(run) }}</strong>
                  <small>{{ runStatusLabel(run.status) }} · {{ relativeTime(run.completedAt || run.createdAt) }}</small>
                </span>
              </button>
            </div>
          </section>
        </div>

        <aside class="agent-side-card">
          <h2>智能体</h2>
          <dl>
            <div>
              <dt>运行时</dt>
              <dd>
                <span class="agent-status-dot" :data-online="runtime?.status === 'online'" />
                <ProviderLogo :provider="runtime?.provider ?? ''" :size="14" />
                {{ runtime?.name ?? "未绑定" }}
              </dd>
            </div>
            <div><dt>模型</dt><dd>{{ agent.model || "auto" }}</dd></div>
            <div><dt>并发</dt><dd>{{ agent.maxConcurrentTasks }}</dd></div>
            <div><dt>任务</dt><dd>{{ tasks.length }}</dd></div>
          </dl>
          <section>
            <header><h3>Skills</h3><span>{{ selectedSkills.length }}</span></header>
            <div v-if="selectedSkills.length" class="agent-chip-row">
              <span v-for="skill in selectedSkills" :key="skill.id">{{ skill.name }}</span>
            </div>
            <p v-else>还没有允许访问的 Skill。</p>
          </section>
          <section>
            <header><h3>MCP</h3><span>{{ selectedMcps.length }}</span></header>
            <div v-if="selectedMcps.length" class="agent-chip-row">
              <span v-for="item in selectedMcps" :key="item.id">{{ item.name }}</span>
            </div>
            <p v-else>还没有允许访问的 MCP。</p>
          </section>
        </aside>
      </div>
    </div>

    <div v-else-if="section === 'stats'" class="agent-detail-body vs-scroll">
      <div class="agent-stats">
        <div class="usage-toolbar">
          <label>
            时间范围
            <UiSelect v-model="statsRange" aria-label="时间范围" :options="STAT_RANGES.map((item) => ({ value: item.days, label: item.label }))" />
          </label>
        </div>
        <p v-if="statsError" class="form-error">{{ statsError }}</p>
        <div v-else-if="!stats" class="usage-loading">
          <div class="ds-stat-grid">
            <span v-for="index in 4" :key="index" class="ds-skeleton usage-skeleton-kpi" />
          </div>
        </div>
        <div v-else-if="stats.total === 0" class="ds-card ds-empty usage-empty">
          <span class="ds-empty-icon"><BarChart3 :size="18" /></span>
          <h3>暂无运行数据</h3>
          <p>这个智能体开始运行后，次数、Token 和金额会出现在这里。</p>
        </div>
        <template v-else>
          <div class="usage-kpis">
            <article>
              <span>金额 · {{ statsRange }}天</span>
              <strong>{{ formatUsd(stats.costUsd) }}</strong>
              <small>按模型单价估算，CLI 上报的费用优先</small>
            </article>
            <article>
              <span>Token · {{ statsRange }}天</span>
              <strong>{{ formatTokens(totalTokens(stats)) }}</strong>
              <small>输入 {{ formatTokens(stats.inputTokens) }} · 输出 {{ formatTokens(stats.outputTokens) }}</small>
            </article>
            <article>
              <span>耗时 · {{ statsRange }}天</span>
              <strong>{{ formatDuration(stats.totalSeconds) }}</strong>
              <small>成功率 {{ stats.successRate.toFixed(1) }}%</small>
            </article>
            <article>
              <span>运行数 · {{ statsRange }}天</span>
              <strong>{{ stats.total }}</strong>
              <small>失败 {{ stats.failed }} · 停止 {{ stats.stopped }}</small>
            </article>
          </div>
          <section class="usage-card">
            <header>
              <h2>每日 Token</h2>
            </header>
            <p v-if="statsDays.every((item) => totalTokens(item) === 0 && item.total === 0)" class="muted">所选时间范围内暂无运行。</p>
            <div v-else class="usage-chart" role="img" aria-label="每日 Token">
              <div v-for="item in statsDays" :key="item.date" class="usage-bar" :title="`${item.date} ${formatTokens(totalTokens(item))} · ${formatUsd(item.costUsd)}`">
                <span class="usage-bar-fill" :style="{ height: statsBarHeight(item) }" />
              </div>
            </div>
          </section>
        </template>
      </div>
    </div>

    <div v-else-if="section === 'tasks'" class="agent-detail-body vs-scroll">
      <div class="agent-task-list">
        <div v-if="!tasks.length" class="ds-empty">
          <span class="ds-empty-icon"><ListTodo :size="18" /></span>
          <h3>还没有任务</h3>
          <p>分配给这个智能体的任务会出现在这里。</p>
        </div>
        <button
          v-for="task in tasks"
          :key="task.id"
          type="button"
          class="agent-task-row"
          @click="router.push(`/tasks/${task.id}`)"
        >
          <strong>{{ task.title }}</strong>
          <span class="ds-badge">{{ taskStatusLabel(task.status) }}</span>
          <em>{{ relativeTime(task.updatedAt) }}</em>
        </button>
      </div>
    </div>

    <div v-else class="agent-split">
      <aside>
        <div v-if="section === 'capabilities'" class="ds-nav-list" role="tablist" aria-label="能力">
          <button type="button" :class="{ active: capabilityTab === 'instructions' }" @click="capabilityTab = 'instructions'">指令</button>
          <button type="button" :class="{ active: capabilityTab === 'skills' }" @click="capabilityTab = 'skills'">Skills</button>
          <button type="button" :class="{ active: capabilityTab === 'mcp' }" @click="capabilityTab = 'mcp'">MCP</button>
        </div>
        <div v-else class="ds-nav-list" role="tablist" aria-label="设置">
          <button type="button" class="active">通用</button>
        </div>
      </aside>
      <section class="vs-scroll">
        <header class="agent-section-heading">
          <div>
            <h2>{{ section === 'capabilities' ? (capabilityTab === 'instructions' ? '指令' : capabilityTab === 'skills' ? 'Skills' : 'MCP') : '通用' }}</h2>
            <p v-if="section === 'capabilities' && capabilityTab === 'instructions' && isChief">系统智能体的指令只能查看。</p>
            <p v-else-if="section === 'capabilities' && capabilityTab === 'instructions'">这些指令会附加到该智能体发起的每次任务中。</p>
            <p v-else-if="section === 'capabilities'">打开开关后，这个智能体才能使用外层菜单里对应的 {{ capabilityTab === 'skills' ? 'Skill' : 'MCP' }}，切换后立即生效。</p>
            <p v-else>名称、头像、运行时和模型会显示在聊天与任务记录中。</p>
          </div>
          <span v-if="saved" class="save-success"><Check :size="14" /> 已保存</span>
          <!-- Skill / MCP 开关即时保存；系统智能体的指令只读，都不需要保存按钮 -->
          <UiButton
            v-if="section !== 'capabilities' || (capabilityTab === 'instructions' && !isChief)"
            variant="primary"
            :class="{ 'is-loading': busy }"
            :disabled="busy || !draft.name.trim()"
            @click="save"
          >
            <Save :size="15" />{{ busy ? "保存中…" : "保存更改" }}
          </UiButton>
        </header>

        <div v-if="section === 'capabilities'" class="agent-settings-card">
          <template v-if="capabilityTab === 'instructions'">
            <label v-if="isChief">
              产品身份
              <UiTextarea :model-value="agent.systemInstructions ?? ''" rows="10" readonly />
              <small>这段说明嵌在应用里，不能在这里改。</small>
            </label>
            <label>
              {{ isChief ? "工作区备注" : "系统指令" }}
              <UiTextarea v-model="draft.instructions" :rows="isChief ? 8 : 14" :readonly="isChief" placeholder="说明角色职责、回答风格和执行约束" />
            </label>
            <!-- 平台层的运行说明对所有智能体都会附加，只读展示，让用户知道模型实际收到了什么 -->
            <details v-if="systemContext" class="agent-system-brief">
              <summary>平台运行说明 <small>每次运行自动附加在最前面，只读</small></summary>
              <UiTextarea :model-value="systemContext.runtimeBrief" rows="16" readonly />
            </details>
          </template>
          <template v-else>
            <div class="resource-toolbar">
              <nav class="ds-segmented" aria-label="按开启状态筛选">
                <button type="button" :class="{ active: resourceFilter === 'all' }" @click="resourceFilter = 'all'">全部 {{ resourceCounts.all }}</button>
                <button type="button" :class="{ active: resourceFilter === 'on' }" @click="resourceFilter = 'on'">已开启 {{ resourceCounts.on }}</button>
                <button type="button" :class="{ active: resourceFilter === 'off' }" @click="resourceFilter = 'off'">未开启 {{ resourceCounts.off }}</button>
              </nav>
              <UiInput v-model="resourceQuery" type="search" :placeholder="`搜索 ${capabilityTab === 'skills' ? 'Skill' : 'MCP'}`" />
            </div>
            <div class="resource-check-list">
              <label v-for="resource in visibleResources" :key="resource.id">
                <span>
                  <strong>{{ resource.name }}</strong>
                  <small>{{ capabilityTab === "mcp" ? (resource.summary || resource.description) : (resource.description || resource.sourcePath) }}</small>
                </span>
                <span v-if="capabilityTab === 'mcp'" class="ds-badge" :data-tone="resource.configStatus === 'needs_auth' ? 'warning' : undefined">{{ mcpAuthLabel(resource) }}</span>
                <span class="ds-badge">{{ resource.sourceLabel || resource.owner }}</span>
                <input
                  class="ds-switch"
                  type="checkbox"
                  :checked="enabledIds.includes(resource.id)"
                  :aria-label="`${resource.name} 开关`"
                  @change="toggleResource(resource.id, ($event.target as HTMLInputElement).checked)"
                />
              </label>
              <template v-if="visibleBuiltinSkills.length">
                <p class="resource-group-label">系统内置 · 所有智能体都会加载，不能关闭</p>
                <label v-for="item in visibleBuiltinSkills" :key="`builtin:${item.name}`" class="is-locked">
                  <span>
                    <strong>{{ item.name }}</strong>
                    <small>{{ item.description }}</small>
                  </span>
                  <span class="ds-badge" data-tone="purple">内置</span>
                  <input class="ds-switch" type="checkbox" checked disabled :aria-label="`${item.name} 始终开启`" />
                </label>
              </template>
              <p v-if="!tabResources.length && !builtinSkills.length" class="empty-copy">
                外层{{ capabilityTab === 'skills' ? ' Skills ' : ' MCP ' }}菜单里还没有可授权的项。
              </p>
              <p v-else-if="!visibleResources.length && !visibleBuiltinSkills.length" class="empty-copy">没有符合条件的项。</p>
            </div>
          </template>
        </div>

        <template v-else>
          <section class="agent-settings-group">
            <div class="agent-settings-card">
              <header>
                <h3>资料</h3>
                <p>管理这个智能体在列表和聊天里的展示。</p>
              </header>
              <AvatarPicker v-model="draft.avatar" :name="draft.name" :locked="isChief" @error="error = $event" />
              <label>名称<UiInput v-model="draft.name" required /></label>
              <label>描述<UiTextarea v-model="draft.description" rows="4" placeholder="这个智能体负责什么" /></label>
            </div>
          </section>
          <section class="agent-settings-group">
            <div class="agent-settings-card">
              <header>
                <h3>执行配置</h3>
                <p>选择本机运行时、模型和同时进行的任务数。</p>
              </header>
              <div class="agent-settings-fields">
                <label>运行时
                  <RuntimeSelect
                    v-model="draft.runtimeId"
                    required
                    placeholder="选择运行时"
                    @update:model-value="draft.model = 'auto'"
                  />
                </label>
                <label>模型<RuntimeModelSelect v-model="draft.model" :runtime-id="draft.runtimeId" /></label>
                <label>最大并发任务
                  <UiSelect v-model="draft.maxConcurrentTasks" :options="Array.from({ length: 6 }, (_, index) => ({ value: index + 1, label: String(index + 1) }))" />
                </label>
              </div>
            </div>
          </section>
        </template>
        <p v-if="error" class="form-error">{{ error }}</p>
      </section>
    </div>
  </div>

  <section v-else class="page panel placeholder">
    <span class="ds-empty-icon"><Wrench :size="18" /></span>
    <h2>未找到智能体</h2>
    <p>该智能体可能已被删除。</p>
    <UiButton @click="router.push('/agents')">返回智能体列表</UiButton>
  </section>
</template>
