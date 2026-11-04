import type {
  Agent,
  BootstrapData,
  ChatSession,
  Project,
  RunEvent,
  Squad,
  Workspace,
} from "../../domain/types";
import { t } from "../../i18n";

export type HomeFilter = "all" | "running" | "archived";
export type HomePill = "running" | "ready" | "setup";

export interface HomeWorker {
  key: string;
  name: string;
  kind: "agent" | "squad";
  initial: string;
  image: string;
  title: string;
}

export interface HomeProject {
  id: string;
  name: string;
  note: string;
  workspaces: Workspace[];
  pill: HomePill;
  pillText: string;
  archived: boolean;
  filters: Exclude<HomeFilter, "all" | "archived">[];
  meta: string;
  workingMeta: string;
  workerSentence: string;
  workerParts: SentencePart[];
  idleLine: string;
  workers: HomeWorker[];
  /** 这个需求下所有会话；界面据此读实时 run 状态 */
  sessionIds: string[];
  gitCount: number;
  folderCount: number;
  searchText: string;
}

export interface SentencePart {
  text: string;
  bold?: boolean;
}

function pushJoinedNames(parts: SentencePart[], names: string[]) {
  names.forEach((name, index) => {
    if (index > 0) {
      parts.push({
        text: index === names.length - 1 && names.length > 1
          ? t("projectHome.listJoinerLast")
          : t("projectHome.listJoiner"),
      });
    }
    parts.push({ text: name, bold: true });
  });
}

export function workerSentenceParts(working: string[], queued: string[]): SentencePart[] {
  const parts: SentencePart[] = [];
  if (working.length || queued.length) {
    if (working.length) {
      pushJoinedNames(parts, working);
      parts.push({ text: t("projectHome.working") });
    }
    if (queued.length) {
      if (parts.length) parts.push({ text: t("projectHome.clauseJoiner") });
      pushJoinedNames(parts, queued);
      parts.push({ text: t("projectHome.queued") });
    }
    return parts;
  }
  return parts;
}

export function relativeEndedAt(iso: string, now = Date.now()): string {
  const elapsed = Math.max(0, now - new Date(iso).getTime());
  const hours = Math.floor(elapsed / 3_600_000);
  if (hours < 1) return t("projectHome.justEnded");
  if (hours < 24) return t("projectHome.endedHoursAgo", { n: hours });
  const days = Math.floor(hours / 24);
  if (days === 1) return t("projectHome.endedYesterday");
  return t("projectHome.endedDaysAgo", { n: days });
}

function initialOf(name: string, fallback: string): string {
  return name.trim().slice(0, 1) || fallback;
}

/** 所有需求共用的查找表，一次建好，避免每个需求都扫全部工作区、会话和运行。 */
export interface HomeIndex {
  workspaces: Map<string, Workspace[]>;
  sessions: Map<string, ChatSession[]>;
  sessionById: Map<string, ChatSession>;
  activity: Map<string, "running" | "queued">;
  /** 会话上正在跑 / 排队的智能体（按 run.agentId） */
  activeAgents: Map<string, { status: "running" | "queued"; agentIds: string[] }>;
  agents: Map<string, Agent>;
  squads: Map<string, Squad>;
}

function groupByProject<T extends { projectId?: string | null; projectIds?: string[] }>(items: T[]) {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const ids = item.projectIds?.length
      ? item.projectIds
      : item.projectId
        ? [item.projectId]
        : [];
    for (const projectId of ids) {
      const list = groups.get(projectId);
      if (list) {
        if (!list.includes(item)) list.push(item);
      } else {
        groups.set(projectId, [item]);
      }
    }
  }
  return groups;
}

export function buildHomeIndex(data: BootstrapData, runStates: Record<string, RunEvent>): HomeIndex {
  const activity = new Map<string, "running" | "queued">();
  const activeAgents = new Map<string, { status: "running" | "queued"; agentIds: string[] }>();
  for (const event of Object.values(runStates)) {
    if (event.status !== "running" && event.status !== "queued") continue;
    const prev = activity.get(event.sessionId);
    if (event.status === "running" || !prev) {
      activity.set(event.sessionId, event.status);
    }
    if (!event.agentId) continue;
    const entry = activeAgents.get(event.sessionId) ?? {
      status: event.status,
      agentIds: [],
    };
    if (event.status === "running") entry.status = "running";
    else if (entry.status !== "running") entry.status = "queued";
    if (!entry.agentIds.includes(event.agentId)) entry.agentIds.push(event.agentId);
    activeAgents.set(event.sessionId, entry);
  }
  return {
    // 会话专属工作区（例如解决冲突用的临时合并 Worktree）不算需求关联的项目
    workspaces: groupByProject(data.workspaces.filter((item) => !item.sessionId)),
    sessions: groupByProject(data.sessions),
    sessionById: new Map(data.sessions.map((item) => [item.id, item])),
    activity,
    activeAgents,
    agents: new Map(data.agents.map((item) => [item.id, item])),
    squads: new Map(data.squads.map((item) => [item.id, item])),
  };
}

function agentWorker(agentId: string, index: HomeIndex, fallbackName = ""): HomeWorker {
  const agent = index.agents.get(agentId);
  const name = agent?.name || fallbackName || t("projectHome.agentFallback");
  return {
    key: `agent:${agentId}`,
    name,
    kind: "agent",
    initial: initialOf(name, "智"),
    image: agent?.avatar ?? "",
    title: t("projectHome.agentTitle", { name }),
  };
}

function targetWorker(session: ChatSession, index: HomeIndex): HomeWorker {
  if (session.targetType === "squad") {
    const squad = index.squads.get(session.targetId);
    if (squad?.leaderAgentId) {
      return agentWorker(squad.leaderAgentId, index, squad.name);
    }
    const name = squad?.name || session.title || t("projectHome.squadFallback");
    return {
      key: `squad:${session.targetId}`,
      name,
      kind: "squad",
      initial: initialOf(name, "队"),
      image: "",
      title: name,
    };
  }
  return agentWorker(session.targetId, index, session.title);
}

function pushUniqueWorker(list: HomeWorker[], seen: Set<string>, worker: HomeWorker) {
  if (seen.has(worker.key)) return;
  seen.add(worker.key);
  list.push(worker);
}

function workingMetaOf(working: string[]): string {
  return working.length
    ? t("projectHome.workingMeta", { names: working.join(t("projectHome.listJoiner")) })
    : "";
}

export function buildHomeProject(
  project: Project,
  data: BootstrapData,
  runStates: Record<string, RunEvent>,
  now = Date.now(),
  index = buildHomeIndex(data, runStates),
): HomeProject {
  const workspaces = index.workspaces.get(project.id) ?? [];
  const sessions = [...(index.sessions.get(project.id) ?? [])]
    .sort((a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt));
  const working: HomeWorker[] = [];
  const queued: HomeWorker[] = [];
  const activeSeen = new Set<string>();
  const history: HomeWorker[] = [];
  const historySeen = new Set<string>();

  const projectSessionIds = new Set(sessions.map((item) => item.id));

  for (const session of sessions) {
    const active = index.activeAgents.get(session.id);
    if (active) {
      for (const agentId of active.agentIds) {
        pushUniqueWorker(
          active.status === "running" ? working : queued,
          activeSeen,
          agentWorker(agentId, index),
        );
      }
    } else {
      const activity = index.activity.get(session.id);
      if (activity) {
        pushUniqueWorker(
          activity === "running" ? working : queued,
          activeSeen,
          targetWorker(session, index),
        );
      }
    }
  }

  // 历史只收真正跑过 / 回复过的智能体：任务执行者 + 本需求会话里已结束的 run。
  // 不把「新对话选了谁」的 session.target 算进去。
  for (const event of Object.values(runStates)) {
    if (!projectSessionIds.has(event.sessionId) || !event.agentId) continue;
    if (event.status === "running" || event.status === "queued") continue;
    pushUniqueWorker(history, historySeen, agentWorker(event.agentId, index));
  }

  for (const task of data.tasks) {
    const session = index.sessionById.get(task.sessionId);
    if (!session) continue;
    const projectIds = session.projectIds?.length
      ? session.projectIds
      : session.projectId
        ? [session.projectId]
        : [];
    if (!projectIds.includes(project.id)) continue;
    if (task.agentId) pushUniqueWorker(history, historySeen, agentWorker(task.agentId, index));
  }

  const filters: HomeProject["filters"] = [];
  if (working.length > 0 || queued.length > 0) filters.push("running");

  const pill: HomePill = workspaces.length === 0
    ? "setup"
    : working.length > 0 || queued.length > 0
      ? "running"
      : "ready";

  const workerNames = {
    working: working.map((item) => item.name),
    queued: queued.map((item) => item.name),
  };
  const workerParts = workerSentenceParts(workerNames.working, workerNames.queued);
  const workerSentence = workerParts.map((part) => part.text).join("");
  const latest = sessions[0];
  const idleLine = latest
    ? t("projectHome.idleLine", {
      name: targetWorker(latest, index).name,
      ended: relativeEndedAt(latest.updatedAt, now),
    })
    : t("projectHome.idlePrefix");

  const gitCount = workspaces.filter((item) => item.kind === "git").length;
  const folderCount = workspaces.length - gitCount;
  // 有进行中 / 排队则只展示这些人；否则展示在该需求上工作过的智能体
  const workers = working.length || queued.length ? [...working, ...queued] : history;
  const searchText = [
    project.name,
    project.note,
    ...workspaces.map((item) => `${item.name} ${item.workPath} ${item.branch ?? ""}`),
    ...history.map((item) => item.name),
    ...workers.map((item) => item.name),
  ].join(" ").toLowerCase();

  return {
    id: project.id,
    name: project.name,
    note: project.note,
    workspaces,
    pill,
    pillText: pill === "running" ? t("projectHome.pillRunning") : "",
    archived: Boolean(project.archivedAt),
    filters,
    meta: t("projectHome.metaProjects", {
      n: workspaces.length,
      working: workingMetaOf(working.map((item) => item.name)),
    }),
    workingMeta: workingMetaOf(working.map((item) => item.name)),
    workerSentence,
    workerParts,
    idleLine,
    workers,
    sessionIds: sessions.map((item) => item.id),
    gitCount,
    folderCount,
    searchText,
  };
}

export function matchesHomeFilter(project: HomeProject, filter: HomeFilter, query: string): boolean {
  // 已归档的需求只在「已归档」里出现，其他筛选（含「全部」）都排除。
  if (filter === "archived") {
    if (!project.archived) return false;
  } else {
    if (project.archived) return false;
    if (filter !== "all" && !project.filters.includes(filter)) return false;
  }
  const needle = query.trim().toLowerCase();
  return !needle || project.searchText.includes(needle);
}

export function workspaceKindLabel(gitCount: number, folderCount: number): string {
  const parts = [t("projectHome.linkedProjects")];
  if (gitCount) parts.push(t("projectHome.gitCount", { n: gitCount }));
  if (folderCount) parts.push(t("projectHome.folderCount", { n: folderCount }));
  return parts.join(t("projectHome.metaJoiner"));
}
