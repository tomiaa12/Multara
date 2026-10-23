import type { Project } from "../domain/types";
import {
  isReferenceChip,
  type ComposerNode,
  type ComposerReferenceChipNode,
} from "../components/chat/composer-content";

export function requirementIds(nodes: ComposerNode[]) {
  return nodes.flatMap((node) =>
    isReferenceChip(node) && node.referenceKind === "requirement" ? [node.referenceId] : [],
  );
}

export function syncRequirementChips(
  nodes: ComposerNode[],
  projectIds: string[],
  projects: Pick<Project, "id" | "name">[],
) {
  const withoutRequirements = nodes.filter((node) =>
    !isReferenceChip(node) || node.referenceKind !== "requirement",
  );
  const chips = projectIds.flatMap<ComposerNode>((id) => {
    const requirement = projects.find((item) => item.id === id);
    if (!requirement) return [];
    const chip: ComposerReferenceChipNode = {
      type: "chip",
      id: `requirement:${id}`,
      referenceId: id,
      referenceKind: "requirement",
      label: requirement.name,
    };
    return [chip, { type: "text", value: " " }];
  });
  return [...withoutRequirements, ...chips];
}
