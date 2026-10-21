<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { ChevronDown, ChevronUp, History, Pencil, Play, Plus, Trash2, Workflow, X } from "@lucide/vue";
import type { Automation, AutomationRun, WebhookDelivery } from "../domain/types";
import { listAutomationRuns, listWebhookDeliveries } from "../services/backend";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import { runStatusLabel, runStatusTone } from "../components/run-log";
import {
  COMMON_TIMEZONES,
  RUNBOOK_TEMPLATE,
  browserTimezone,
  buildSchedulePreview,
  defaultScheduleDraft,
  formatNextLabel,
  fromCron,
  toCron,
  WEEKDAY_LABELS,
  type ScheduleDraft,
  type ScheduleRepeat,
} from "./automation-schedule";

const store = useAppStore();
const showForm = ref(false);
const busy = ref(false);
const error = ref("");
const expandedId = ref<string | null>(null);
const histories = reactive<Record<string, AutomationRun[]>>({});
const deliveries = reactive<Record<string, WebhookDelivery[]>>({});

const form = reactive({
  id: undefined as string | undefined,
  name: "",
  projectId: "",
  target: "",
  prompt: "",
  triggerType: "schedule" as Automation["triggerType"],
  eventName: "",
  signingSecret: "",
  enabled: true,
  schedule: defaultScheduleDraft() as ScheduleDraft,
});

const agentTargets = computed(() =>
  store.data?.agents.map((agent) => ({ value: `agent:${agent.id}`, label: agent.name, avatar: agent.avatar })) ?? [],
);
// 小队以队长头像作为标识
const squadTargets = computed(() =>
  store.data?.squads.map((squad) => ({
    value: `squad:${squad.id}`,
    label: squad.name,
    avatar: store.data?.agents.find((agent) => agent.id === squad.leaderAgentId)?.avatar ?? "◉",
  })) ?? [],
);
const targets = computed(() => [...agentTargets.value, ...squadTargets.value]);
const targetGroups = computed(() => [
  { label: t("automation.group.agent"), options: agentTargets.value },
  { label: t("automation.group.squad"), options: squadTargets.value },
].filter((group) => group.options.length));

const timezoneOptions = computed(() => {
  const set = new Set(COMMON_TIMEZONES);
  set.add(form.schedule.timezone || browserTimezone());
  return [...set];
});

const schedulePreview = computed(() => buildSchedulePreview(form.schedule));

const primaryTriggers = [
  { value: "schedule" as const, label: t("automation.trigger.schedule") },
  { value: "webhook" as const, label: "Webhook" },
];

const editOnlyTriggers = [
  { value: "manual" as const, label: t("automation.trigger.manual") },
  { value: "api" as const, label: "API" },
];


function edit(item?: Automation) {
  const timezone = item?.timezone ?? browserTimezone();
  Object.assign(form, {
    id: item?.id,
    name: item?.name ?? "",
    projectId: item?.projectId ?? "",
    target: item ? `${item.targetType}:${item.targetId}` : targets.value[0]?.value ?? "",
    prompt: item?.prompt ?? RUNBOOK_TEMPLATE,
    triggerType: item?.triggerType ?? "schedule",
    eventName: item?.eventName ?? "",
    signingSecret: "",
    enabled: item?.enabled ?? true,
    schedule: item?.triggerType === "schedule"
      ? fromCron(item.cronExpression, timezone)
      : defaultScheduleDraft(timezone),
  });
  error.value = "";
  showForm.value = true;
}

async function save() {
  const [targetType, targetId] = form.target.split(":", 2);
  if (!targetType || !targetId) {
    error.value = t("automation.targetRequired");
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    const cronExpression = form.triggerType === "schedule" ? toCron(form.schedule) : null;
    const intervalMinutes =
      form.triggerType === "schedule" && form.schedule.mode === "every" && !form.schedule.raw
        ? form.schedule.everyHours * 60
        : null;
    await store.saveAutomation({
      id: form.id,
      name: form.name,
      projectId: form.projectId || null,
      targetType: targetType as "agent" | "squad",
      targetId,
      prompt: form.prompt,
      triggerType: form.triggerType,
      intervalMinutes,
      cronExpression,
      timezone: form.triggerType === "schedule" ? form.schedule.timezone : null,
      eventName: ["webhook", "api"].includes(form.triggerType) ? form.eventName || null : null,
      signingSecret: form.triggerType === "webhook" ? form.signingSecret || null : null,
      enabled: form.enabled,
    });
    showForm.value = false;
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy.value = false;
  }
}

async function remove(item: Automation) {
  if (!window.confirm(t("automation.deleteConfirm", { name: item.name }))) return;
  await store.deleteAutomation(item.id);
}

async function run(item: Automation) {
  try {
    await store.runAutomation(item.id);
    await toggleHistory(item, true);
  } catch (cause) {
    window.alert(cause instanceof Error ? cause.message : String(cause));
  }
}

function triggerLabel(item: Automation) {
  if (item.triggerType === "schedule") return `Cron ${item.cronExpression} · ${item.timezone}`;
  if (item.triggerType === "webhook") return `Webhook${item.eventName ? ` · ${item.eventName}` : ""}`;
  if (item.triggerType === "api") return `API${item.eventName ? ` · ${item.eventName}` : ""}`;
  return t("automation.trigger.manual");
}

function formatTime(value: string | null) {
  return value ? new Date(value).toLocaleString() : "—";
}

async function toggleHistory(item: Automation, forceOpen = false) {
  if (!forceOpen && expandedId.value === item.id) {
    expandedId.value = null;
    return;
  }
  histories[item.id] = await listAutomationRuns(item.id);
  deliveries[item.id] = item.triggerType === "webhook" ? await listWebhookDeliveries(item.id) : [];
  expandedId.value = item.id;
}

function webhookUrl(item: Automation) {
  const port = store.data?.settings.webhookPort ?? "43127";
  return `http://127.0.0.1:${port}/webhooks/${item.webhookToken}`;
}

function setTrigger(type: Automation["triggerType"]) {
  form.triggerType = type;
}

function clearAdvancedCron() {
  form.schedule.raw = null;
}

function setRepeat(repeat: ScheduleRepeat) {
  form.schedule.repeat = repeat;
  if (repeat === "weekly" && form.schedule.weekdays.length === 0) {
    form.schedule.weekdays = [1, 2, 3, 4, 5];
  }
}

function toggleWeekday(day: number) {
  const current = new Set(form.schedule.weekdays);
  if (current.has(day)) {
    if (current.size === 1) return;
    current.delete(day);
  } else {
    current.add(day);
  }
  form.schedule.weekdays = [...current].sort((left, right) => left - right);
}
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div><h1>{{ t("automation.title") }}</h1><p>{{ t("automation.subtitle") }}</p></div>
      <div class="header-actions">
        <UiButton variant="primary" :disabled="!targets.length" @click="edit()"><Plus :size="16" /> {{ t("automation.create") }}</UiButton>
      </div>
    </div>
    <section v-if="!store.data?.automations.length" class="panel placeholder">
      <span class="ds-empty-icon"><Workflow :size="18" /></span>
      <h2>{{ t("automation.empty") }}</h2>
      <p>{{ t("automation.emptyHint") }}</p>
    </section>
    <div v-else class="card-list">
      <article v-for="item in store.data?.automations" :key="item.id" class="automation-entry">
        <div class="list-card automation-card">
          <span class="automation-status" :class="{ enabled: item.enabled }" />
          <div class="grow">
            <strong>{{ item.name }}</strong>
            <small>{{ triggerLabel(item) }}<template v-if="store.data?.projects.find((project) => project.id === item.projectId)"> · {{ store.data?.projects.find((project) => project.id === item.projectId)?.name }}</template></small>
            <p>{{ item.prompt }}</p>
            <div class="automation-times">
              <span>{{ t("automation.lastRun", { time: formatTime(item.lastRunAt) }) }}</span>
              <span v-if="item.nextRunAt">{{ t("automation.nextRun", { time: formatTime(item.nextRunAt) }) }}</span>
              <span v-if="item.lastRunStatus" class="ds-badge" :data-tone="runStatusTone(item.lastRunStatus)">{{ t("automation.lastStatus", { status: runStatusLabel(item.lastRunStatus) }) }}</span>
            </div>
            <code v-if="item.triggerType === 'webhook' && item.webhookToken" class="webhook-url" :title="webhookUrl(item)">{{ webhookUrl(item) }}</code>
          </div>
          <span class="ds-badge" :data-tone="item.enabled ? 'success' : undefined">{{ item.enabled ? t("common.enabledOn") : t("common.enabledOff") }}</span>
          <div class="automation-actions">
            <UiButton variant="ghost" size="icon" :title="t('common.runNow')" :disabled="!item.enabled" @click="run(item)"><Play :size="14" /></UiButton>
            <UiButton variant="ghost" size="small" :title="t('common.history')" :class="{ active: expandedId === item.id }" @click="toggleHistory(item)"><History :size="14" /><component :is="expandedId === item.id ? ChevronUp : ChevronDown" :size="12" /></UiButton>
            <UiButton variant="ghost" size="icon" :title="t('common.edit')" @click="edit(item)"><Pencil :size="14" /></UiButton>
            <UiButton variant="ghost" size="icon" class="automation-delete" :title="t('common.delete')" @click="remove(item)"><Trash2 :size="14" /></UiButton>
          </div>
        </div>
        <div v-if="expandedId === item.id" class="automation-history">
          <div v-if="!histories[item.id]?.length" class="empty-inline">{{ t("automation.noHistory") }}</div>
          <div v-for="runItem in histories[item.id]" :key="runItem.id" class="history-row">
            <span class="ds-badge">{{ runItem.source }}</span><strong><span class="ds-badge" :data-tone="runStatusTone(runItem.status)">{{ runStatusLabel(runItem.status) }}</span></strong>
            <span>{{ formatTime(runItem.triggeredAt) }}</span>
            <small v-if="runItem.reasonCode">{{ runItem.reasonCode }}{{ runItem.failureReason ? ` · ${runItem.failureReason}` : "" }}</small>
          </div>
          <template v-if="item.triggerType === 'webhook' && deliveries[item.id]?.length">
            <h4>{{ t("automation.webhookAudit") }}</h4>
            <div v-for="delivery in deliveries[item.id]" :key="delivery.id" class="history-row">
              <span class="ds-badge">{{ delivery.status }}</span><strong>{{ delivery.event ?? t("automation.anyEvent") }}</strong>
              <span>{{ formatTime(delivery.createdAt) }}</span>
              <small>{{ delivery.dedupeKey }}<template v-if="delivery.reasonCode"> · {{ delivery.reasonCode }}</template></small>
            </div>
          </template>
        </div>
      </article>
    </div>

    <div v-if="showForm" class="modal-backdrop" @click.self="showForm = false">
      <form class="modal automation-dialog" @submit.prevent="save">
        <header class="automation-dialog-head">
          <h2>{{ form.id ? t("automation.edit") : t("automation.create") }}</h2>
          <UiButton variant="ghost" size="icon" :title="t('common.close')" @click="showForm = false"><X :size="16" /></UiButton>
        </header>

        <div class="automation-dialog-body">
          <section class="automation-dialog-left">
            <UiInput
              v-model="form.name"
              class="automation-name-input"
              required
              :placeholder="t('automation.namePlaceholder')"
            />
            <div class="automation-runbook-label">
              <span>RUNBOOK</span>
              <small>{{ t("automation.promptHint") }}</small>
            </div>
            <UiTextarea
              v-model="form.prompt"
              class="automation-runbook"
              required
              rows="18"
              placeholder="# 目标"
            />
          </section>

          <aside class="automation-dialog-right">
            <label>
              {{ t("automation.target") }}
              <UiSelect v-model="form.target" required :placeholder="t('automation.targetPlaceholder')" :groups="targetGroups" />
            </label>

            <label>
              {{ t("automation.project") }}
              <UiSelect
                v-model="form.projectId"
                :options="[{ value: '', label: t('automation.noProject') }, ...(store.data?.projects ?? []).map((project) => ({ value: project.id, label: project.name }))]"
              />
              <small class="automation-hint">{{ t("automation.projectHint") }}</small>
            </label>

            <div class="automation-field">
              <span class="automation-field-label">{{ t("automation.trigger") }}</span>
              <div class="automation-segment">
                <button
                  v-for="item in primaryTriggers"
                  :key="item.value"
                  type="button"
                  class="automation-segment-btn"
                  :class="{ active: form.triggerType === item.value }"
                  @click="setTrigger(item.value)"
                >{{ item.label }}</button>
                <template v-if="form.id">
                  <button
                    v-for="item in editOnlyTriggers"
                    :key="item.value"
                    type="button"
                    class="automation-segment-btn"
                    :class="{ active: form.triggerType === item.value }"
                    @click="setTrigger(item.value)"
                  >{{ item.label }}</button>
                </template>
              </div>
            </div>

            <template v-if="form.triggerType === 'schedule'">
              <div v-if="form.schedule.raw" class="automation-config-card">
                <div class="automation-field-label">高级 Cron</div>
                <p class="automation-hint">当前表达式无法用简化编辑器表示，可直接修改或恢复为定点/间隔。</p>
                <UiInput v-model="form.schedule.raw" required placeholder="0 9 * * *" />
                <UiButton type="button" data-variant="ghost" data-size="small" @click="clearAdvancedCron">改用简化编辑器</UiButton>
              </div>

              <template v-else>
                <div class="automation-field">
                  <span class="automation-field-label">时间</span>
                  <div class="automation-segment">
                    <button
                      type="button"
                      class="automation-segment-btn"
                      :class="{ active: form.schedule.mode === 'at' }"
                      @click="form.schedule.mode = 'at'"
                    >定点</button>
                    <button
                      type="button"
                      class="automation-segment-btn"
                      :class="{ active: form.schedule.mode === 'every' }"
                      @click="form.schedule.mode = 'every'"
                    >按间隔</button>
                  </div>
                </div>

                <label v-if="form.schedule.mode === 'at'">
                  时刻
                  <UiInput v-model="form.schedule.time" type="time" required />
                </label>
                <label v-else>
                  每隔（小时）
                  <UiInput v-model.number="form.schedule.everyHours" type="number" min="1" max="23" required />
                </label>

                <div v-if="form.schedule.mode === 'at'" class="automation-field">
                  <span class="automation-field-label">重复</span>
                  <div class="automation-segment">
                    <button
                      type="button"
                      class="automation-segment-btn"
                      :class="{ active: form.schedule.repeat === 'daily' }"
                      @click="setRepeat('daily')"
                    >每天</button>
                    <button
                      type="button"
                      class="automation-segment-btn"
                      :class="{ active: form.schedule.repeat === 'weekly' }"
                      @click="setRepeat('weekly')"
                    >每周</button>
                    <button
                      type="button"
                      class="automation-segment-btn"
                      :class="{ active: form.schedule.repeat === 'monthly' }"
                      @click="setRepeat('monthly')"
                    >每月</button>
                  </div>
                  <div v-if="form.schedule.repeat === 'weekly'" class="automation-weekdays">
                    <button
                      v-for="(label, day) in WEEKDAY_LABELS"
                      :key="label"
                      type="button"
                      class="automation-weekday"
                      :class="{ active: form.schedule.weekdays.includes(day) }"
                      @click="toggleWeekday(day)"
                    >{{ label }}</button>
                  </div>
                  <label v-else-if="form.schedule.repeat === 'monthly'" class="automation-month-day">
                    日期
                    <UiInput v-model.number="form.schedule.monthDay" type="number" min="1" max="31" required />
                    <small v-if="form.schedule.monthDay >= 29" class="automation-hint">没有这一天的月份会跳过。</small>
                  </label>
                </div>
              </template>

              <label>
                时区
                <UiSelect
                  v-model="form.schedule.timezone"
                  required
                  searchable
                  search-placeholder="搜索时区"
                  :options="timezoneOptions.map((zone) => ({ value: zone, label: zone }))"
                />
              </label>

              <div class="automation-preview-card">
                <strong>{{ schedulePreview.label }}</strong>
                <code>{{ schedulePreview.cron }}</code>
                <div class="automation-preview-next">
                  <span class="automation-field-label">接下来</span>
                  <ul v-if="schedulePreview.next.length">
                    <li v-for="(item, index) in schedulePreview.next" :key="index">
                      <span>{{ formatNextLabel(item.at, form.schedule.timezone) }}</span>
                      <small>{{ item.relative }}</small>
                    </li>
                  </ul>
                  <p v-else class="automation-hint">无法预览下次运行时间</p>
                </div>
              </div>
            </template>

            <template v-else-if="form.triggerType === 'webhook'">
              <div class="automation-config-card">
                <p class="automation-hint">保存后会生成本地 Webhook URL。调用方需提供唯一的 <code>X-Multara-Delivery</code>；可选 HMAC 签名头 <code>X-Multara-Signature: sha256=&lt;hex&gt;</code>。</p>
              </div>
              <label>
                事件过滤（可选）
                <UiInput v-model="form.eventName" placeholder="例如：build.completed" />
              </label>
              <label>
                HMAC 签名密钥（可选）
                <UiInput v-model="form.signingSecret" type="password" placeholder="留空表示不校验；编辑时留空保留原值" />
              </label>
            </template>

            <template v-else-if="form.triggerType === 'api'">
              <label>
                事件过滤（可选）
                <UiInput v-model="form.eventName" placeholder="例如：deploy.finished" />
              </label>
              <p class="automation-hint">通过应用 API 触发；仅编辑已有自动化时可用。</p>
            </template>

            <template v-else>
              <p class="automation-hint">仅可通过列表中的「立即运行」手动触发。</p>
            </template>

            <label class="check-row"><UiInput v-model="form.enabled" type="checkbox" />{{ t("automation.enable") }}</label>
          </aside>
        </div>

        <footer class="automation-dialog-foot">
          <p class="automation-foot-hint">
            <template v-if="form.triggerType === 'schedule'">保存后会按计划运行；Multara 需保持打开。</template>
            <template v-else-if="form.triggerType === 'webhook'">保存后即可接收 Webhook 请求。</template>
            <template v-else>保存后可在列表中手动运行。</template>
          </p>
          <div class="modal-actions">
            <p v-if="error" class="form-error">{{ error }}</p>
            <UiButton type="button" @click="showForm = false">{{ t("common.cancel") }}</UiButton>
            <UiButton variant="primary" type="submit" :class="{ 'is-loading': busy }" :disabled="busy">{{ busy ? t("common.saving") : (form.id ? t("common.save") : t("common.create")) }}</UiButton>
          </div>
        </footer>
      </form>
    </div>
  </div>
</template>
