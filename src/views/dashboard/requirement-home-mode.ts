import { ref } from "vue";
import { readStorage, writeStorage } from "../../utils/safe-storage";

export type RequirementHomeMode = "code" | "run";

const STORAGE_KEY = "multara.requirements.homeMode";

function readMode(): RequirementHomeMode {
  return readStorage(STORAGE_KEY) === "run" ? "run" : "code";
}

export const requirementHomeMode = ref<RequirementHomeMode>(readMode());

export function isRequirementHomeMode(value: string): value is RequirementHomeMode {
  return value === "code" || value === "run";
}

export function setRequirementHomeMode(mode: RequirementHomeMode) {
  requirementHomeMode.value = mode;
  writeStorage(STORAGE_KEY, mode);
}

const RUN_RAIL_KEY = "multara.requirements.runRail";
export const RUN_RAIL_MIN = 240;
export const RUN_RAIL_MAX = 480;
export const RUN_RAIL_DEFAULT = 300;

export function readRunRail() {
  const stored = Number(readStorage(RUN_RAIL_KEY));
  if (stored >= RUN_RAIL_MIN && stored <= RUN_RAIL_MAX) return stored;
  return RUN_RAIL_DEFAULT;
}

export function writeRunRail(width: number) {
  const next = Math.min(RUN_RAIL_MAX, Math.max(RUN_RAIL_MIN, width));
  writeStorage(RUN_RAIL_KEY, String(next));
  return next;
}
