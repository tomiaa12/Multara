export type GitHookOperation = "push" | "commit" | "pull" | "sync";

export type GitHookFailure = {
  kind: "local_hook" | "deps_mismatch";
  repairable: boolean;
  skippable: boolean;
  operation: GitHookOperation;
  message: string;
};

const PREFIX = "@@multara-hook@@";

export function parseGitHookFailure(cause: unknown): GitHookFailure | null {
  const raw = cause instanceof Error ? cause.message : String(cause);
  const start = raw.indexOf(PREFIX);
  if (start < 0) return null;
  const rest = raw.slice(start + PREFIX.length);
  const newline = rest.indexOf("\n");
  if (newline < 0) return null;
  try {
    const payload = JSON.parse(rest.slice(0, newline)) as {
      kind?: string;
      repairable?: boolean;
      skippable?: boolean;
      operation?: string;
    };
    if (payload.kind !== "local_hook" && payload.kind !== "deps_mismatch") return null;
    if (
      payload.operation !== "push" &&
      payload.operation !== "commit" &&
      payload.operation !== "pull" &&
      payload.operation !== "sync"
    ) {
      return null;
    }
    const message = rest.slice(newline + 1).trim();
    if (!message) return null;
    return {
      kind: payload.kind,
      repairable: payload.repairable === true,
      skippable: payload.skippable === true,
      operation: payload.operation,
      message,
    };
  } catch {
    return null;
  }
}
