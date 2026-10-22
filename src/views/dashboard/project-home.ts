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

function sessionActivity(
  sessionId: string,
  runStates: Record<string, RunEvent>,
): "running" | "queued" | "idle" {
  let queued = false;
  for (const event of Object.values(runStates)) {
    if (event.sessionId !== sessionId) continue;
    if (event.status === "running") return "running";
    if (event.status === "queued") queued = true;
  }
  return queued ? "queued" : "idle";
}

function targetWorker(session: ChatSession, data: BootstrapData, kind?: HomeWorker["kind"]): HomeWorker {
  if (session.targetType === "squad") {
    const squad = data.squads.find((item: Squad) => item.id === session.targetId);
    const name = squad?.name || session.title || t("projectHome.squadFallback");
    return {
      key: `squad:${session.targetId}`,
      name,
      kind: kind ?? "squad",
      initial: initialOf(name, "队"),
      image: "",
      title: name,
    };
  }
  const agent = data.agents.find((item: Agent) => item.id === session.targetId);
  const name = agent?.name || session.title || t("projectHome.agentFallback");
  const avatar = agent?.avatar ?? "";
  return {
    key: `agent:${session.targetId}`,
    name,
    kind: kind ?? "agent",
    initial: initialOf(name, "智"),
    image: avatar.startsWith("data:image/") ? avatar : "",
    title: t("projectHome.agentTitle", { name }),
  };
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
): HomeProject {
  // 会话专属工作区（例如解决冲突用的临时合并 Worktree）不算需求关联的项目
  const workspaces = data.workspaces.filter((item) => item.projectId === project.id && !item.sessionId);
  const sessions = data.sessions
    .filter((item) => item.projectId === project.id)
    .slice()
    .sort((a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt));
  const working: HomeWorker[] = [];
  const queued: HomeWorker[] = [];
  const seen = new Set<string>();

  for (const session of sessions) {
    const activity = sessionActivity(session.id, runStates);
    if (activity === "idle") continue;
    const worker = targetWorker(session, data);
    if (seen.has(worker.key)) continue;
    seen.add(worker.key);
    (activity === "running" ? working : queued).push(worker);
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
      name: targetWorker(latest, data).name,
      ended: relativeEndedAt(latest.updatedAt, now),
    })
    : t("projectHome.idlePrefix");

  const gitCount = workspaces.filter((item) => item.kind === "git").length;
  const folderCount = workspaces.length - gitCount;
  const workers = [...working, ...queued];
  const searchText = [
    project.name,
    project.note,
    ...workspaces.map((item) => `${item.name} ${item.workPath} ${item.branch ?? ""}`),
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
