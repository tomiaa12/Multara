<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { t } from "../i18n";
import { BarChart3, RefreshCw } from "@lucide/vue";
import type { RunStatistics, StatisticGroup, UsageDay } from "../domain/types";
import { getRunStatistics } from "../services/backend";
import { useAppStore } from "../stores/app";
import AgentAvatar from "../components/AgentAvatar.vue";
import { useRouter } from "vue-router";
import {
  addUsageDays,
  emptyUsageDay,
  formatDuration,
  formatTokens,
  formatUsd,
  totalTokens,
} from "./usage-format";

const RANGES = [
  { days: "1", label: "今天" },
  { days: "7", label: "7 天" },
  { days: "30", label: "30 天" },
  { days: "90", label: "90 天" },
  { days: "180", label: "180 天" },
] as const;

const store = useAppStore();
const router = useRouter();
const stats = ref<RunStatistics | null>(null);
const loading = ref(false);
const error = ref("");
const tab = ref<"usage" | "errors">("usage");
const range = ref("30");
const projectId = ref("");
const dim = ref<"daily" | "weekly">("daily");
const metric = ref<"tasks" | "time" | "tokens" | "cost">("tasks");
const agentSort = ref<"tasks" | "time" | "tokens" | "cost" | "failed">("tasks");
const errorSort = ref<"failed" | "rate">("failed");
const showAllAgents = ref(false);
const showAllErrors = ref(false);
const updatedAt = ref("");

const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || "本地时间";
const days = computed(() => Number(range.value) || 30);
const projects = computed(() => store.data?.projects ?? []);

function allowedDims(value: number): Array<"daily" | "weekly"> {
  // 短窗口按天才看得清；180 天只按周，避免一根柱子挤成一条线。
  if (value <= 7) return ["daily"];
  if (value >= 180) return ["weekly"];
  return ["daily", "weekly"];
}

const dims = computed(() => allowedDims(days.value));

watch(days, () => {
  if (!dims.value.includes(dim.value)) dim.value = dims.value[0];
  showAllAgents.value = false;
  showAllErrors.value = false;
});

async function refresh() {
  loading.value = true;
  error.value = "";
  try {
    stats.value = await getRunStatistics(days.value, projectId.value);
    updatedAt.value = new Date().toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading.value = false;
  }
}

watch([range, projectId], refresh, { immediate: true });

function isoDate(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** 周一作为一周的起点，和日历里的自然周对齐。 */
function weekStart(iso: string): string {
  const [year, month, day] = iso.split("-").map(Number);
  const date = new Date(year, (month || 1) - 1, day || 1);
  const weekday = date.getDay();
  const offset = weekday === 0 ? -6 : 1 - weekday;
  date.setDate(date.getDate() + offset);
  return isoDate(date);
}

const filledDays = computed(() => {
  const map = new Map((stats.value?.daily ?? []).map((item) => [item.date.slice(0, 10), item]));
  const today = new Date();
  const out: UsageDay[] = [];
  for (let offset = days.value - 1; offset >= 0; offset -= 1) {
    const date = new Date(today.getFullYear(), today.getMonth(), today.getDate() - offset);
    const key = isoDate(date);
    out.push(map.get(key) ?? emptyUsageDay(key));
  }
  return out;
});

interface ChartBucket extends UsageDay {
  label: string;
  partial: boolean;
  covered: number;
}

const buckets = computed<ChartBucket[]>(() => {
  if (dim.value === "daily") {
    return filledDays.value.map((item) => ({
      ...item,
      label: item.date.slice(5),
      partial: false,
      covered: 1,
    }));
  }
  const groups = new Map<string, ChartBucket>();
  for (const item of filledDays.value) {
    const start = weekStart(item.date);
    const current = groups.get(start) ?? {
      ...emptyUsageDay(start),
      label: `${start.slice(5)} 起`,
      partial: false,
      covered: 0,
    };
    const next = addUsageDays(current, item);
    groups.set(start, { ...current, ...next, covered: current.covered + 1 });
  }
  return [...groups.values()].map((item) => ({ ...item, partial: item.covered < 7 }));
});

function chartValue(item: UsageDay): number {
  if (tab.value === "errors") return item.failed;
  if (metric.value === "time") return item.seconds;
  if (metric.value === "tokens") return totalTokens(item);
  if (metric.value === "cost") return item.costUsd;
  return item.total;
}

const chartMax = computed(() => Math.max(1, ...buckets.value.map((item) => chartValue(item))));

function barHeight(item: UsageDay): string {
  const value = chartValue(item);
  if (value <= 0) return "0%";
  return `${Math.max(4, (value / chartMax.value) * 100)}%`;
}

const labelEvery = computed(() => {
  if (buckets.value.length <= 8) return 1;
  if (buckets.value.length <= 32) return 5;
  return 4;
});

const chartTitle = computed(() => {
  const period = dim.value === "weekly" ? "每周" : "每日";
  if (tab.value === "errors") return `${period}错误`;
  if (metric.value === "time") return `${period}耗时`;
  if (metric.value === "tokens") return `${period} Token`;
  if (metric.value === "cost") return `${period}金额`;
  return `${period}运行数`;
});

function barTitle(item: ChartBucket): string {
  const value = tab.value === "errors" || metric.value === "tasks"
    ? `${chartValue(item)} 次`
    : metric.value === "tokens"
      ? `${formatTokens(totalTokens(item))} tokens`
      : metric.value === "cost"
        ? formatUsd(item.costUsd)
        : formatDuration(item.seconds);
  const partial = item.partial ? `（未满 · 已过 ${item.covered} / 7 天）` : "";
  return `${item.date} ${value}${partial}`;
}

function formatRate(value: number): string {
  return `${value.toFixed(1)}%`;
}

function failRate(item: StatisticGroup): number {
  return item.total === 0 ? 0 : (item.failed / item.total) * 100;
}

const AGENT_METRIC: Record<typeof agentSort.value, (item: StatisticGroup) => number> = {
  time: (item) => item.seconds,
  tokens: (item) => totalTokens(item),
  cost: (item) => item.costUsd,
  failed: (item) => item.failed,
  tasks: (item) => item.total,
};

const sortedAgents = computed(() => {
  const metric = AGENT_METRIC[agentSort.value];
  return [...(stats.value?.byAgent ?? [])].sort((left, right) => metric(right) - metric(left));
});

const agentMetricMax = computed(() => Math.max(0, ...sortedAgents.value.map((item) => AGENT_METRIC[agentSort.value](item))));

function rankWidth(item: StatisticGroup): string {
  if (agentMetricMax.value <= 0) return "0%";
  return `${(AGENT_METRIC[agentSort.value](item) / agentMetricMax.value) * 100}%`;
}

/** 按 id 取当前智能体头像；旧数据没有 id 时按名字兜底，已删除的智能体用首字 */
function agentAvatar(group: StatisticGroup) {
  const agents = store.data?.agents ?? [];
  const agent = agents.find((item) => item.id === group.id) ?? agents.find((item) => item.name === group.name);
  return agent?.avatar ?? "";
}
const visibleAgents = computed(() => (
  showAllAgents.value ? sortedAgents.value : sortedAgents.value.slice(0, 8)
));

const failingAgents = computed(() => {
  const rows = (stats.value?.byAgent ?? []).filter((item) => item.failed > 0);
  rows.sort((left, right) => (
    errorSort.value === "rate" ? failRate(right) - failRate(left) : right.failed - left.failed
  ));
  return rows;
});

const visibleErrors = computed(() => (
  showAllErrors.value ? failingAgents.value : failingAgents.value.slice(0, 8)
));

const topFailure = computed(() =>
  [...(stats.value?.byAgent ?? [])].sort((left, right) => right.failed - left.failed)[0],
);

const failureRate = computed(() => {
  const total = stats.value?.total ?? 0;
  const failed = stats.value?.failed ?? 0;
  return total === 0 ? 0 : (failed / total) * 100;
});

function openAgent(group: StatisticGroup) {
  if (group.id) void router.push(`/agents/${group.id}`);
}
</script>

<template>
  <div class="page usage-page">
    <div class="page-header">
      <div>
        <h1>{{ t("stats.title") }}</h1>
        <p>{{ timezone }} · 更新于 {{ updatedAt || "—" }}</p>
      </div>
      <div class="header-actions">
        <UiButton title="刷新" :class="{ 'is-loading': loading }" :disabled="loading" @click="refresh">
          <RefreshCw :size="14" />刷新
        </UiButton>
      </div>
    </div>

    <div class="usage-toolbar">
      <div class="ds-segmented usage-tabs" role="tablist">
        <button type="button" role="tab" :aria-selected="tab === 'usage'" :class="{ active: tab === 'usage' }" @click="tab = 'usage'">用量</button>
        <button type="button" role="tab" :aria-selected="tab === 'errors'" :class="{ active: tab === 'errors' }" @click="tab = 'errors'">错误</button>
      </div>
      <label>
        时间范围
        <UiSelect v-model="range" aria-label="时间范围" :options="RANGES.map((item) => ({ value: item.days, label: item.label }))" />
      </label>
      <label>
        需求
        <UiSelect
          v-model="projectId"
          aria-label="需求"
          :options="[{ value: '', label: '全部需求' }, ...projects.map((project) => ({ value: project.id, label: project.name }))]"
        />
      </label>
    </div>

    <p v-if="error" class="form-error">{{ error }}</p>
    <div v-else-if="!stats" class="usage-loading">
      <div class="ds-stat-grid">
        <span v-for="index in 4" :key="index" class="ds-skeleton usage-skeleton-kpi" />
      </div>
      <span class="ds-skeleton usage-skeleton-chart" />
    </div>
    <div v-else-if="stats.total === 0" class="ds-card ds-empty usage-empty">
      <span class="ds-empty-icon"><BarChart3 :size="18" /></span>
      <h3>暂无运行数据</h3>
      <p>当智能体在这里开始运行后，运行次数、Token 和金额将出现在此处。</p>
    </div>

    <template v-else-if="tab === 'usage'">
      <div class="usage-kpis">
        <article>
          <span>金额 · {{ days }}天</span>
          <strong>{{ formatUsd(stats.costUsd) }}</strong>
          <small>按模型单价估算，CLI 上报的费用优先</small>
        </article>
        <article>
          <span>Token · {{ days }}天</span>
          <strong>{{ formatTokens(totalTokens(stats)) }}</strong>
          <small>输入 {{ formatTokens(stats.inputTokens) }} · 输出 {{ formatTokens(stats.outputTokens) }}</small>
        </article>
        <article>
          <span>耗时 · {{ days }}天</span>
          <strong>{{ formatDuration(stats.totalSeconds) }}</strong>
          <small>成功率 {{ formatRate(stats.successRate) }}</small>
        </article>
        <article>
          <span>运行数 · {{ days }}天</span>
          <strong>{{ stats.total }}</strong>
          <small>失败 {{ stats.failed }} · 停止 {{ stats.stopped }}</small>
        </article>
      </div>

      <section class="usage-card">
        <header>
          <h2>{{ chartTitle }}</h2>
          <div class="usage-controls">
            <div class="ds-segmented" aria-label="统计粒度">
              <button
                v-for="item in dims"
                :key="item"
                type="button"
                :class="{ active: dim === item }"
                @click="dim = item"
              >{{ item === "daily" ? "按天" : "按周" }}</button>
            </div>
            <div class="ds-segmented" aria-label="统计指标">
              <button type="button" :class="{ active: metric === 'cost' }" @click="metric = 'cost'">金额</button>
              <button type="button" :class="{ active: metric === 'tokens' }" @click="metric = 'tokens'">Token</button>
              <button type="button" :class="{ active: metric === 'tasks' }" @click="metric = 'tasks'">运行数</button>
              <button type="button" :class="{ active: metric === 'time' }" @click="metric = 'time'">耗时</button>
            </div>
          </div>
        </header>
        <p v-if="buckets.every((item) => chartValue(item) === 0)" class="muted">所选时间范围内暂无运行。</p>
        <div v-else class="usage-chart" role="img" :aria-label="chartTitle">
          <div v-for="(item, index) in buckets" :key="item.date" class="usage-bar" :title="barTitle(item)">
            <span class="usage-bar-fill" :style="{ height: barHeight(item) }" />
            <small v-if="index % labelEvery === 0 || item.partial">{{ item.partial ? `${item.label}*` : item.label }}</small>
            <small v-else>&nbsp;</small>
          </div>
        </div>
      </section>

      <section class="usage-card">
        <header>
          <div>
            <h2>排行榜</h2>
            <p>{{ sortedAgents.length }} 个智能体</p>
          </div>
          <div class="usage-controls">
            <span class="usage-sort-label">排序</span>
            <div class="ds-segmented" aria-label="智能体排序依据">
            <button type="button" :class="{ active: agentSort === 'cost' }" @click="agentSort = 'cost'">金额</button>
            <button type="button" :class="{ active: agentSort === 'tokens' }" @click="agentSort = 'tokens'">Token</button>
            <button type="button" :class="{ active: agentSort === 'tasks' }" @click="agentSort = 'tasks'">运行数</button>
            <button type="button" :class="{ active: agentSort === 'time' }" @click="agentSort = 'time'">耗时</button>
            <button type="button" :class="{ active: agentSort === 'failed' }" @click="agentSort = 'failed'">失败数</button>
            </div>
          </div>
        </header>
        <p v-if="!sortedAgents.length" class="muted">所选时间范围内暂无智能体活动。</p>
        <table v-else class="usage-table">
          <thead>
            <tr>
              <th>智能体</th>
              <th class="usage-rank-col" />
              <th :class="{ 'is-metric': agentSort === 'cost' }">金额</th>
              <th :class="{ 'is-metric': agentSort === 'tokens' }">Token</th>
              <th :class="{ 'is-metric': agentSort === 'time' }">耗时</th>
              <th :class="{ 'is-metric': agentSort === 'tasks' }">运行数</th>
              <th :class="{ 'is-metric': agentSort === 'failed' }">失败</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="agent in visibleAgents" :key="agent.id ?? agent.name">
              <td>
                <button type="button" class="usage-agent usage-agent-link" :disabled="!agent.id" @click="openAgent(agent)">
                  <AgentAvatar :value="agentAvatar(agent)" :name="agent.name" :size="20" />
                  <span>{{ agent.name }}</span>
                </button>
              </td>
              <td class="usage-rank-col">
                <span class="usage-rank"><span :style="{ width: rankWidth(agent) }" /></span>
              </td>
              <td :class="{ 'is-metric': agentSort === 'cost' }">{{ formatUsd(agent.costUsd) }}</td>
              <td :class="{ 'is-metric': agentSort === 'tokens' }">{{ formatTokens(totalTokens(agent)) }}</td>
              <td :class="{ 'is-metric': agentSort === 'time' }">{{ formatDuration(agent.seconds) }}</td>
              <td :class="{ 'is-metric': agentSort === 'tasks' }">{{ agent.total }}</td>
              <td :class="{ 'is-metric': agentSort === 'failed' }">{{ agent.failed }}</td>
            </tr>
          </tbody>
        </table>
        <button v-if="sortedAgents.length > 8" type="button" class="usage-more" @click="showAllAgents = !showAllAgents">
          {{ showAllAgents ? "只看前 8 个" : "展开全部" }}
        </button>
      </section>

      <section v-if="stats.byRuntime.length" class="usage-card">
        <header><h2>按运行时</h2></header>
        <table class="usage-table">
          <thead>
            <tr>
              <th>运行时</th>
              <th>金额</th>
              <th>Token</th>
              <th>耗时</th>
              <th>运行数</th>
              <th>失败</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="runtime in stats.byRuntime" :key="runtime.name">
              <td>{{ runtime.name }}</td>
              <td>{{ formatUsd(runtime.costUsd) }}</td>
              <td>{{ formatTokens(totalTokens(runtime)) }}</td>
              <td>{{ formatDuration(runtime.seconds) }}</td>
              <td>{{ runtime.total }}</td>
              <td>{{ runtime.failed }}</td>
            </tr>
          </tbody>
        </table>
      </section>
    </template>

    <template v-else>
      <div class="usage-kpis usage-kpis-3">
        <article>
          <span>失败运行 · {{ days }}天</span>
          <strong>{{ stats.failed }}</strong>
          <small>共 {{ stats.total }} 次运行</small>
        </article>
        <article>
          <span>失败率 · {{ days }}天</span>
          <strong>{{ formatRate(failureRate) }}</strong>
          <small v-if="stats.total < 5">运行不足 5 次，失败率参考价值有限。</small>
          <small v-else>{{ stats.failed }} / {{ stats.total }}</small>
        </article>
        <article>
          <span>受影响智能体 · {{ days }}天</span>
          <strong>{{ failingAgents.length }}</strong>
          <small v-if="topFailure && topFailure.failed">最多 {{ topFailure.name }} · {{ topFailure.failed }} 次</small>
          <small v-else>没有失败记录</small>
        </article>
      </div>

      <section class="usage-card">
        <header>
          <h2>{{ chartTitle }}</h2>
          <div class="ds-segmented usage-controls" aria-label="统计粒度">
            <button
              v-for="item in dims"
              :key="item"
              type="button"
              :class="{ active: dim === item }"
              @click="dim = item"
            >{{ item === "daily" ? "按天" : "按周" }}</button>
          </div>
        </header>
        <p v-if="stats.failed === 0" class="muted">所选时间范围内没有失败的运行。</p>
        <div v-else class="usage-chart" role="img" aria-label="错误趋势">
          <div v-for="(item, index) in buckets" :key="item.date" class="usage-bar" :title="barTitle(item)">
            <span class="usage-bar-fill is-error" :style="{ height: barHeight(item) }" />
            <small v-if="index % labelEvery === 0">{{ item.label }}</small>
            <small v-else>&nbsp;</small>
          </div>
        </div>
      </section>

      <section class="usage-card">
        <header>
          <div>
            <h2>问题最多的智能体</h2>
            <p>{{ stats.total }} 次运行中有 {{ stats.failed }} 次失败 · {{ formatRate(failureRate) }}</p>
          </div>
          <div class="ds-segmented usage-controls" aria-label="错误榜排序依据">
            <button type="button" :class="{ active: errorSort === 'failed' }" @click="errorSort = 'failed'">失败数</button>
            <button type="button" :class="{ active: errorSort === 'rate' }" @click="errorSort = 'rate'">失败率</button>
          </div>
        </header>
        <p v-if="!failingAgents.length" class="muted">所选时间范围内没有失败的运行。</p>
        <table v-else class="usage-table">
          <thead>
            <tr>
              <th>智能体</th>
              <th>失败数</th>
              <th>运行数</th>
              <th>失败率</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="agent in visibleErrors" :key="agent.id ?? agent.name">
              <td>
                <span class="usage-agent">
                  <AgentAvatar :value="agentAvatar(agent)" :name="agent.name" :size="20" />
                  <span>{{ agent.name }}</span>
                </span>
              </td>
              <td>{{ agent.failed }}</td>
              <td>{{ agent.total }}</td>
              <td>{{ formatRate(failRate(agent)) }}</td>
            </tr>
          </tbody>
        </table>
        <button v-if="failingAgents.length > 8" type="button" class="usage-more" @click="showAllErrors = !showAllErrors">
          {{ showAllErrors ? "只看前 8 个" : `展开全部 ${failingAgents.length} 个` }}
        </button>
      </section>
    </template>
  </div>
</template>
