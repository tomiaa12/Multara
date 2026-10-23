import type { Agent, Squad } from "../../domain/types";
import type { SelectGroup, SelectOption } from "../ui/select-types";

export function buildChatTargetOptions(
  agents: Agent[],
  squads: Squad[],
  squadLabel: (name: string) => string,
): SelectOption<string>[] {
  return [
    ...agents.map((agent) => ({
      value: `agent:${agent.id}`,
      label: agent.name,
      description: agent.model || agent.description,
      avatar: agent.avatar,
    })),
    ...squads.map((squad) => ({
      value: `squad:${squad.id}`,
      label: squadLabel(squad.name),
      description: squad.description,
      avatar: agents.find((agent) => agent.id === squad.leaderAgentId)?.avatar ?? "◉",
      keywords: squad.memberAgentIds,
    })),
  ];
}

export function groupChatTargetOptions(
  options: SelectOption<string>[],
  labels: { agents: string; squads: string },
): SelectGroup<string>[] {
  return [
    {
      label: labels.agents,
      options: options.filter((option) => !String(option.value).startsWith("squad:")),
    },
    {
      label: labels.squads,
      options: options
        .filter((option) => String(option.value).startsWith("squad:"))
        .map((option) => ({ ...option, label: option.label.replace(/^[^·]+ · /, "") })),
    },
  ].filter((group) => group.options.length);
}
