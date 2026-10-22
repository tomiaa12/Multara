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

const RANGES = computed(() => [
  { days: "1", label: t("stats.range.today") },
  { days: "7", label: t("agent.stats.days", { n: 7 }) },
  { days: "30", label: t("agent.stats.days", { n: 30 }) },
  { days: "90", label: t("agent.stats.days", { n: 90 }) },
  { days: "180", label: t("agent.stats.days", { n: 180 }) },
]);

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

const timezone = computed(() => Intl.DateTimeFormat().resolvedOptions().timeZone || t("stats.localTime"));
const days = computed(() => Number(range.value) || 30);
const projects = computed(() => store.data?.projects ?? []);
const projectOptions = computed(() => [
  { value: "", label: t("stats.projectAll") },
  ...projects.value.map((project) => ({ value: project.id, label: project.name })),
]);

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
    updatedAt.value = new Date().toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
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
      label: t("stats.weekFrom", { date: start.slice(5) }),
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
  const period = dim.value === "weekly"
    ? t("stats.chart.period.weekly")
    : t("stats.chart.period.daily");
  if (tab.value === "errors") return t("stats.chart.errors", { period });
  if (metric.value === "time") return t("stats.chart.time", { period });
  if (metric.value === "tokens") return t("stats.chart.tokens", { period });
  if (metric.value === "cost") return t("stats.chart.cost", { period });
  return t("stats.chart.tasks", { period });
});

function barTitle(item: ChartBucket): string {
  let value: string;
  if (tab.value === "errors" || metric.value === "tasks") {
    value = t("stats.bar.times", { n: chartValue(item) });
  } else if (metric.value === "tokens") {
    value = `${formatTokens(totalTokens(item))} tokens`;
  } else if (metric.value === "cost") {
    value = formatUsd(item.costUsd);
  } else {
    value = formatDuration(item.seconds);
  }
  const partial = item.partial ? t("stats.bar.partial", { covered: item.covered }) : "";
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
        <p>{{ timezone }} · {{ t("stats.updatedAt", { time: updatedAt || "—" }) }}</p>
      </div>
      <div class="header-actions">
        <UiButton :title="t('stats.refresh')" :class="{ 'is-loading': loading }" :disabled="loading" @click="refresh">
          <RefreshCw :size="14" />{{ t("stats.refresh") }}
        </UiButton>
      </div>
    </div>

    <div class="usage-toolbar">
      <div class="ds-segmented usage-tabs" role="tablist">
        <button type="button" role="tab" :aria-selected="tab === 'usage'" :class="{ active: tab === 'usage' }" @click="tab = 'usage'">{{ t("stats.tab.usage") }}</button>
        <button type="button" role="tab" :aria-selected="tab === 'errors'" :class="{ active: tab === 'errors' }" @click="tab = 'errors'">{{ t("stats.tab.errors") }}</button>
      </div>
      <label>
        {{ t("stats.range") }}
        <UiSelect v-model="range" :aria-label="t('stats.range')" :options="RANGES.map((item) => ({ value: item.days, label: item.label }))" />
      </label>
      <label>
        {{ t("stats.project") }}
        <UiSelect
          v-model="projectId"
          :aria-label="t('stats.project')"
          :options="projectOptions"
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
      <h3>{{ t("stats.emptyTitle") }}</h3>
      <p>{{ t("stats.emptyHint") }}</p>
    </div>

    <template v-else-if="tab === 'usage'">
      <div class="usage-kpis">
        <article>
          <span>{{ t("agent.stats.cost", { n: days }) }}</span>
          <strong>{{ formatUsd(stats.costUsd) }}</strong>
          <small>{{ t("agent.stats.costHint") }}</small>
        </article>
        <article>
          <span>{{ t("agent.stats.tokens", { n: days }) }}</span>
          <strong>{{ formatTokens(totalTokens(stats)) }}</strong>
          <small>{{ t("agent.stats.tokensHint", { input: formatTokens(stats.inputTokens), output: formatTokens(stats.outputTokens) }) }}</small>
        </article>
        <article>
          <span>{{ t("agent.stats.duration", { n: days }) }}</span>
          <strong>{{ formatDuration(stats.totalSeconds) }}</strong>
          <small>{{ t("agent.stats.successRate", { rate: stats.successRate.toFixed(1) }) }}</small>
        </article>
        <article>
          <span>{{ t("agent.stats.runs", { n: days }) }}</span>
          <strong>{{ stats.total }}</strong>
          <small>{{ t("agent.stats.runsHint", { failed: stats.failed, stopped: stats.stopped }) }}</small>
        </article>
      </div>

      <section class="usage-card">
        <header>
          <h2>{{ chartTitle }}</h2>
          <div class="usage-controls">
            <div class="ds-segmented" :aria-label="t('stats.dim')">
              <button
                v-for="item in dims"
                :key="item"
                type="button"
                :class="{ active: dim === item }"
                @click="dim = item"
              >{{ item === "daily" ? t("stats.dim.daily") : t("stats.dim.weekly") }}</button>
            </div>
            <div class="ds-segmented" :aria-label="t('stats.metric')">
              <button type="button" :class="{ active: metric === 'cost' }" @click="metric = 'cost'">{{ t("stats.metric.cost") }}</button>
              <button type="button" :class="{ active: metric === 'tokens' }" @click="metric = 'tokens'">{{ t("stats.metric.tokens") }}</button>
              <button type="button" :class="{ active: metric === 'tasks' }" @click="metric = 'tasks'">{{ t("stats.metric.tasks") }}</button>
              <button type="button" :class="{ active: metric === 'time' }" @click="metric = 'time'">{{ t("stats.metric.time") }}</button>
            </div>
          </div>
        </header>
        <p v-if="buckets.every((item) => chartValue(item) === 0)" class="muted">{{ t("stats.noRangeData") }}</p>
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
            <h2>{{ t("stats.leaderboard") }}</h2>
            <p>{{ t("stats.agentCount", { n: sortedAgents.length }) }}</p>
          </div>
          <div class="usage-controls">
            <span class="usage-sort-label">{{ t("stats.sort") }}</span>
            <div class="ds-segmented" :aria-label="t('stats.sortBy')">
            <button type="button" :class="{ active: agentSort === 'cost' }" @click="agentSort = 'cost'">{{ t("stats.metric.cost") }}</button>
            <button type="button" :class="{ active: agentSort === 'tokens' }" @click="agentSort = 'tokens'">{{ t("stats.metric.tokens") }}</button>
            <button type="button" :class="{ active: agentSort === 'tasks' }" @click="agentSort = 'tasks'">{{ t("stats.metric.tasks") }}</button>
            <button type="button" :class="{ active: agentSort === 'time' }" @click="agentSort = 'time'">{{ t("stats.metric.time") }}</button>
            <button type="button" :class="{ active: agentSort === 'failed' }" @click="agentSort = 'failed'">{{ t("stats.failedCount") }}</button>
            </div>
          </div>
        </header>
        <p v-if="!sortedAgents.length" class="muted">{{ t("stats.noAgentActivity") }}</p>
        <table v-else class="usage-table">
          <thead>
            <tr>
              <th>{{ t("stats.col.agent") }}</th>
              <th class="usage-rank-col" />
              <th :class="{ 'is-metric': agentSort === 'cost' }">{{ t("stats.metric.cost") }}</th>
              <th :class="{ 'is-metric': agentSort === 'tokens' }">{{ t("stats.metric.tokens") }}</th>
              <th :class="{ 'is-metric': agentSort === 'time' }">{{ t("stats.metric.time") }}</th>
              <th :class="{ 'is-metric': agentSort === 'tasks' }">{{ t("stats.metric.tasks") }}</th>
              <th :class="{ 'is-metric': agentSort === 'failed' }">{{ t("stats.col.failed") }}</th>
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
          {{ showAllAgents ? t("stats.showTop8") : t("stats.showAll") }}
        </button>
      </section>

      <section v-if="stats.byRuntime.length" class="usage-card">
        <header><h2>{{ t("stats.byRuntime") }}</h2></header>
        <table class="usage-table">
          <thead>
            <tr>
              <th>{{ t("stats.col.runtime") }}</th>
              <th>{{ t("stats.metric.cost") }}</th>
              <th>{{ t("stats.metric.tokens") }}</th>
              <th>{{ t("stats.metric.time") }}</th>
              <th>{{ t("stats.metric.tasks") }}</th>
              <th>{{ t("stats.col.failed") }}</th>
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
          <span>{{ t("stats.failedRuns", { n: days }) }}</span>
          <strong>{{ stats.failed }}</strong>
          <small>{{ t("stats.failedRunsHint", { n: stats.total }) }}</small>
        </article>
        <article>
          <span>{{ t("stats.failRate", { n: days }) }}</span>
          <strong>{{ formatRate(failureRate) }}</strong>
          <small v-if="stats.total < 5">{{ t("stats.failRateLowSample") }}</small>
          <small v-else>{{ stats.failed }} / {{ stats.total }}</small>
        </article>
        <article>
          <span>{{ t("stats.affectedAgents", { n: days }) }}</span>
          <strong>{{ failingAgents.length }}</strong>
          <small v-if="topFailure && topFailure.failed">{{ t("stats.topFailure", { name: topFailure.name, n: topFailure.failed }) }}</small>
          <small v-else>{{ t("stats.noFailures") }}</small>
        </article>
      </div>

      <section class="usage-card">
        <header>
          <h2>{{ chartTitle }}</h2>
          <div class="ds-segmented usage-controls" :aria-label="t('stats.dim')">
            <button
              v-for="item in dims"
              :key="item"
              type="button"
              :class="{ active: dim === item }"
              @click="dim = item"
            >{{ item === "daily" ? t("stats.dim.daily") : t("stats.dim.weekly") }}</button>
          </div>
        </header>
        <p v-if="stats.failed === 0" class="muted">{{ t("stats.noFailedRuns") }}</p>
        <div v-else class="usage-chart" role="img" :aria-label="t('stats.errorTrend')">
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
            <h2>{{ t("stats.problemAgents") }}</h2>
            <p>{{ t("stats.problemSummary", { total: stats.total, failed: stats.failed, rate: formatRate(failureRate) }) }}</p>
          </div>
          <div class="ds-segmented usage-controls" :aria-label="t('stats.errorSort')">
            <button type="button" :class="{ active: errorSort === 'failed' }" @click="errorSort = 'failed'">{{ t("stats.failedCount") }}</button>
            <button type="button" :class="{ active: errorSort === 'rate' }" @click="errorSort = 'rate'">{{ t("stats.failRateLabel") }}</button>
          </div>
        </header>
        <p v-if="!failingAgents.length" class="muted">{{ t("stats.noFailedRuns") }}</p>
        <table v-else class="usage-table">
          <thead>
            <tr>
              <th>{{ t("stats.col.agent") }}</th>
              <th>{{ t("stats.failedCount") }}</th>
              <th>{{ t("stats.metric.tasks") }}</th>
              <th>{{ t("stats.failRateLabel") }}</th>
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
          {{ showAllErrors ? t("stats.showTop8") : t("stats.showAllN", { n: failingAgents.length }) }}
        </button>
      </section>
    </template>
  </div>
</template>
