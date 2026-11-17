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
