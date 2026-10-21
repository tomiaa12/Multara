export const REQUIREMENT_CATEGORY_KEY = "multara.requirement-categories";

export const TAG_COLORS = [
  "#ef4444",
  "#f97316",
  "#eab308",
  "#22c55e",
  "#14b8a6",
  "#3b82f6",
  "#6366f1",
  "#a855f7",
  "#ec4899",
  "#64748b",
] as const;

export interface RequirementCategory {
  id: string;
  name: string;
  color: string;
}

export interface RequirementCategoryState {
  categories: RequirementCategory[];
  assignments: Record<string, string[]>;
}

export function tagColorForName(name: string): string {
  let hash = 0;
  for (let index = 0; index < name.length; index += 1) {
    hash = (hash * 31 + name.charCodeAt(index)) >>> 0;
  }
  return TAG_COLORS[hash % TAG_COLORS.length] ?? TAG_COLORS[0];
}

export function emptyRequirementCategories(): RequirementCategoryState {
  return { categories: [], assignments: {} };
}

function asIds(value: unknown): string[] {
  if (typeof value === "string" && value) return [value];
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string" && Boolean(item));
}

export function parseRequirementCategories(raw: string | null): RequirementCategoryState {
  if (!raw) return emptyRequirementCategories();
  try {
    const parsed = JSON.parse(raw) as Partial<RequirementCategoryState>;
    const categories = Array.isArray(parsed.categories)
      ? parsed.categories.flatMap((item) => {
          if (!item?.id || !item?.name) return [];
          return [{
            id: item.id,
            name: item.name,
            color: item.color || tagColorForName(item.name),
          }];
        })
      : [];
    const known = new Set(categories.map((item) => item.id));
    const assignments = parsed.assignments && typeof parsed.assignments === "object"
      ? Object.fromEntries(
          Object.entries(parsed.assignments).flatMap(([projectId, value]) => {
            const ids = asIds(value).filter((id) => known.has(id));
            return ids.length ? [[projectId, ids]] : [];
          }),
        )
      : {};
    return { categories, assignments };
  } catch {
    return emptyRequirementCategories();
  }
}

export function loadRequirementCategories(storage: Pick<Storage, "getItem"> | null): RequirementCategoryState {
  return parseRequirementCategories(storage?.getItem(REQUIREMENT_CATEGORY_KEY) ?? null);
}

export function saveRequirementCategories(storage: Pick<Storage, "setItem">, state: RequirementCategoryState) {
  storage.setItem(REQUIREMENT_CATEGORY_KEY, JSON.stringify(state));
}

export function addRequirementCategory(
  state: RequirementCategoryState,
  name: string,
  id: string,
  color = tagColorForName(name.trim()),
): RequirementCategoryState {
  const trimmed = name.trim();
  if (!trimmed || state.categories.some((item) => item.name === trimmed)) return state;
  return {
    ...state,
    categories: [...state.categories, { id, name: trimmed, color }],
  };
}

export function renameRequirementCategory(
  state: RequirementCategoryState,
  id: string,
  name: string,
): RequirementCategoryState {
  const trimmed = name.trim();
  if (!trimmed || state.categories.some((item) => item.id !== id && item.name === trimmed)) return state;
  return {
    ...state,
    categories: state.categories.map((item) => (item.id === id ? { ...item, name: trimmed } : item)),
  };
}

export function recolorRequirementCategory(
  state: RequirementCategoryState,
  id: string,
  color: string,
): RequirementCategoryState {
  return {
    ...state,
    categories: state.categories.map((item) => (item.id === id ? { ...item, color } : item)),
  };
}

export function removeRequirementCategory(state: RequirementCategoryState, id: string): RequirementCategoryState {
  const assignments = Object.fromEntries(
    Object.entries(state.assignments).flatMap(([projectId, ids]) => {
      const next = ids.filter((item) => item !== id);
      return next.length ? [[projectId, next]] : [];
    }),
  );
  return {
    categories: state.categories.filter((item) => item.id !== id),
    assignments,
  };
}

export function assignRequirementTags(
  state: RequirementCategoryState,
  projectId: string,
  categoryIds: string[],
): RequirementCategoryState {
  const known = new Set(state.categories.map((item) => item.id));
  const ids = [...new Set(categoryIds.filter((id) => known.has(id)))];
  const assignments = { ...state.assignments };
  if (!ids.length) delete assignments[projectId];
  else assignments[projectId] = ids;
  return { ...state, assignments };
}

export function projectMatchesTags(assigned: string[] | undefined, selected: string[]): boolean {
  if (!selected.length) return true;
  const owned = new Set(assigned ?? []);
  return selected.some((id) => owned.has(id));
}
