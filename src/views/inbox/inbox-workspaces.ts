import type { Workspace } from "../../domain/types";

export function inboxProjectIds(input: {
  itemProjectId?: string | null;
  sessionProjectId?: string | null;
  sessionProjectIds?: readonly string[] | null;
}): string[] {
  if (input.sessionProjectIds?.length) return [...input.sessionProjectIds];
  if (input.sessionProjectId) return [input.sessionProjectId];
  if (input.itemProjectId) return [input.itemProjectId];
  return [];
}

/** 按 store 里的现有顺序，留下这些需求下的工作区。 */
export function workspacesForProjects(
  projectIds: readonly string[],
  workspaces: readonly Workspace[],
): Workspace[] {
  if (!projectIds.length) return [];
  const allowed = new Set(projectIds);
  return workspaces.filter((item) => allowed.has(item.projectId));
}
