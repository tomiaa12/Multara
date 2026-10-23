import type { Runtime } from "../domain/types";

export type RuntimeKindFilter = "all" | "builtin" | "custom";

export function isCustomRuntime(runtime: Runtime): boolean {
  return runtime.id.startsWith("custom:");
}

export function visibleRuntimes(runtimes: Runtime[]): Runtime[] {
  return runtimes.filter((runtime) => runtime.status === "online" || isCustomRuntime(runtime));
}

export function filterRuntimes(
  runtimes: Runtime[],
  filter: RuntimeKindFilter,
): Runtime[] {
  const visible = visibleRuntimes(runtimes);
  if (filter === "builtin") {
    return visible.filter((runtime) => !isCustomRuntime(runtime));
  }
  if (filter === "custom") {
    return visible.filter(isCustomRuntime);
  }
  return visible;
}
