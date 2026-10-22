import type { GitRepositoryStatus } from "../../domain/types";
import { t } from "../../i18n";

export interface BranchAction {
  icon: string;
  label: string;
  tooltip: string;
}

export interface SyncAction {
  icon: string;
  text: string;
  tooltip: string;
  kind: "sync" | "publish";
}

export type CheckoutRow =
  | { type: "sep"; label: string }
  | { type: "note"; label: string }
  | { type: "command"; id: "create" | "create-from" | "detach"; label: string; icon: string }
  | { type: "branch"; name: string; remote: boolean };

function checkoutCommands(): Array<Extract<CheckoutRow, { type: "command" }>> {
  return [
    { type: "command", id: "create", label: t("scm.checkoutCreate"), icon: "plus" },
    { type: "command", id: "create-from", label: t("scm.checkoutCreateFrom"), icon: "plus" },
    { type: "command", id: "detach", label: t("scm.checkoutDetach"), icon: "debug-disconnect" },
  ];
}

export function branchAction(repository: GitRepositoryStatus): BranchAction {
  const staged = repository.changes.some((change) => change.staged);
  const dirty = repository.changes.some((change) => !change.staged);
  let icon = "git-branch";
  if (repository.merging) icon = "git-branch-conflicts";
  else if (staged) icon = "git-branch-staged-changes";
  else if (dirty) icon = "git-branch-changes";
  const label = `${repository.branch || "HEAD"}${dirty ? "*" : ""}${staged ? "+" : ""}${repository.merging ? "!" : ""}`;
  return { icon, label, tooltip: t("scm.branchPickerTooltip", { label }) };
}

export function syncAction(repository: GitRepositoryStatus): SyncAction | null {
  if (!repository.hasRemote) return null;
  if (!repository.upstream) {
    return { icon: "cloud-upload", text: "", tooltip: t("scm.publishBranch"), kind: "publish" };
  }
  const slash = repository.upstream.indexOf("/");
  const remote = slash > 0 ? repository.upstream.slice(0, slash) : repository.upstream;
  const name = slash > 0 ? repository.upstream.slice(slash + 1) : repository.upstream;
  if (!repository.ahead && !repository.behind) {
    return { icon: "sync", text: "", tooltip: t("scm.syncChanges"), kind: "sync" };
  }
  const text = `${repository.behind}↓ ${repository.ahead}↑`;
  const remoteRef = `${remote}/${name}`;
  let tooltip = t("scm.syncBoth", {
    remote: remoteRef,
    behind: repository.behind,
    ahead: repository.ahead,
  });
  if (!repository.ahead) {
    tooltip = t("scm.syncPull", { remote: remoteRef, count: repository.behind });
  } else if (!repository.behind) {
    tooltip = t("scm.syncPush", { remote: remoteRef, count: repository.ahead });
  }
  return { icon: "sync", text, tooltip, kind: "sync" };
}

export function checkoutRows(repository: GitRepositoryStatus, query: string): CheckoutRow[] {
  const needle = query.trim().toLowerCase();
  const match = (name: string) => !needle || name.toLowerCase().includes(needle);
  const refs: CheckoutRow[] = [];
  const locals = repository.branches.filter(match);
  const remotes = repository.remoteBranches.filter(match);
  if (locals.length) {
    refs.push({ type: "sep", label: t("scm.branches") }, ...locals.map((name) => ({ type: "branch" as const, name, remote: false })));
  }
  if (remotes.length) {
    refs.push({ type: "sep", label: t("scm.remoteBranches") }, ...remotes.map((name) => ({ type: "branch" as const, name, remote: true })));
  }
  const commands = checkoutCommands();
  if (!needle) return [...commands, ...refs];
  if (!refs.length) return [...commands];
  return [...refs, { type: "sep", label: "" }, ...commands];
}

export function renameBranchNotice(report: {
  localRenamed: boolean;
  localBranch: string;
  remoteAttempted: boolean;
  remoteOk: boolean;
  remoteError: string;
  hadUpstream: boolean;
  upstream: string;
}): string {
  const local = report.localRenamed
    ? t("scm.rename.localDone", { name: report.localBranch })
    : t("scm.rename.localSkipped");
  let remote = t("scm.rename.remoteSkipped");
  if (report.remoteAttempted && report.remoteOk) remote = t("scm.rename.remoteDone");
  if (report.remoteAttempted && !report.remoteOk) remote = t("scm.rename.remoteFailed", { error: report.remoteError });
  const upstream = report.upstream
    ? t("scm.upstreamSet", { name: report.upstream })
    : report.hadUpstream
      ? t("scm.rename.upstreamCleared")
      : t("scm.noUpstream");
  return `${local}。${remote}。${upstream}。`;
}

export function branchNameError(name: string): string {
  const value = name.trim();
  if (!value) return t("scm.branchNameRequired");
  if (/[\s~^:?*[\\]|@{|\.\.|\/\/|^\.|\/$|\.lock$/.test(value) || value.startsWith("-")) {
    return t("scm.branchNameInvalid");
  }
  return "";
}
