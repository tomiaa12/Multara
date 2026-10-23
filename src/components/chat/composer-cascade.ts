export type CascadePane = "categories" | "results";

export type CascadeNavigationState = {
  pane: CascadePane;
  categoryIndex: number;
  resultIndex: number;
};

export type CascadeNavigationIntent = "none" | "close" | "enter-results" | "activate-result";

export function navigateCascade(
  state: CascadeNavigationState,
  key: string,
  categoryCount: number,
  resultCount: number,
): { state: CascadeNavigationState; intent: CascadeNavigationIntent; handled: boolean } {
  if (key === "Escape") return { state, intent: "close", handled: true };
  if (key === "ArrowLeft") {
    return {
      state: { ...state, pane: "categories" },
      intent: "none",
      handled: true,
    };
  }
  if (key === "ArrowRight") {
    return {
      state: { ...state, pane: "results", resultIndex: 0 },
      intent: "enter-results",
      handled: true,
    };
  }
  if (key === "ArrowDown" || key === "ArrowUp") {
    const count = state.pane === "categories" ? categoryCount : resultCount;
    if (!count) return { state, intent: "none", handled: true };
    const direction = key === "ArrowDown" ? 1 : -1;
    const current = state.pane === "categories" ? state.categoryIndex : state.resultIndex;
    const next = (current + direction + count) % count;
    return {
      state: state.pane === "categories"
        ? { ...state, categoryIndex: next, resultIndex: 0 }
        : { ...state, resultIndex: next },
      intent: "none",
      handled: true,
    };
  }
  if (key === "Enter" || key === "Tab") {
    if (state.pane === "categories") {
      return {
        state: { ...state, pane: "results", resultIndex: 0 },
        intent: "enter-results",
        handled: true,
      };
    }
    return { state, intent: "activate-result", handled: true };
  }
  return { state, intent: "none", handled: false };
}

export function matchesCascadeQuery(label: string, description: string | undefined, query: string) {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return true;
  return `${label} ${description ?? ""}`.toLocaleLowerCase().includes(normalized);
}

export function nextCascadeVisibleCount(current: number | undefined, total: number, pageSize: number) {
  return Math.min((current ?? pageSize) + pageSize, total);
}
