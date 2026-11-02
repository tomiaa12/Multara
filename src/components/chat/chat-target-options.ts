import type { Agent, Runtime, Squad } from "../../domain/types";
import { isMiku } from "../../domain/types";
import { t } from "../../i18n";
import type { SelectGroup, SelectOption } from "../ui/select-types";

export type ChatTargetOption = SelectOption<string> & {
  /** 智能体选项：运行时 provider，供下拉第二行画 Logo */
  runtimeProvider?: string;
  /** 智能体选项：运行时显示名 */
  runtimeName?: string;
  /** 智能体选项：模型名（空则 auto） */
  runtimeModel?: string;
  /** 智能体选项：与智能体菜单列表相同的描述文案 */
  agentDescription?: string;
};

/** 与 CollectionView 智能体列表一致 */
export function agentListDescription(agent: Agent) {
  if (isMiku(agent)) return t("miku.description");
  return agent.description || t("common.noDescription");
}

export function buildChatTargetOptions(
  agents: Agent[],
  squads: Squad[],
  squadLabel: (name: string) => string,
  runtimes: Runtime[] = [],
): ChatTargetOption[] {
  const runtimeById = new Map(runtimes.map((item) => [item.id, item]));
  return [
    ...agents.map((agent) => {
      const runtime = runtimeById.get(agent.runtimeId);
      const model = agent.model || "auto";
      const agentDescription = agentListDescription(agent);
      return {
        value: `agent:${agent.id}`,
        label: agent.name,
        // 描述参与搜索；展示由 ChatComposer 插槽画图标+名称+模型+智能体描述
        description: [runtime?.name, model, agentDescription].filter(Boolean).join(" "),
        avatar: agent.avatar,
        keywords: [runtime?.provider, runtime?.name, agent.model, agent.description].filter(Boolean) as string[],
        runtimeProvider: runtime?.provider,
        runtimeName: runtime?.name,
        runtimeModel: model,
        agentDescription,
      };
    }),
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
  options: ChatTargetOption[],
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
