/** 只保留当前界面上的工作区。临时合并区挂在别的会话，需求页看不见，不能算进 Git 角标。 */
export function gitStatusForWorkspaces<T extends { workspaceId: string }>(
  repositories: readonly T[],
  workspaceIds: Iterable<string>,
): T[] {
  const ids = new Set(workspaceIds);
  return repositories.filter((repository) => ids.has(repository.workspaceId));
}
