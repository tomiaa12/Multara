import type { Agent, ChannelType, FeishuContact, InboxItem, Squad } from "../../domain/types";
import { isWorkflowNodeSession, isWorkflowRunSession } from "../../domain/workflow";
import { t } from "../../i18n";

const CHANNEL_SOURCES = new Set(["feishu", "lark", "slack", "dingtalk", "wecom", "telegram", "weixin", "qq"]);

export type InboxSource = "project" | "task" | "workflow" | "chat" | "channel";

export function sortedInboxItems(items: InboxItem[]) {
  return items
    .filter((item) => !isWorkflowNodeSession(item.sessionId))
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
}

export function inboxSource(item: InboxItem): InboxSource {
  if (isWorkflowRunSession(item.sessionId)) return "workflow";
  if (CHANNEL_SOURCES.has(item.channelType ?? "")) return "channel";
  if (item.taskId) return "task";
  if (item.projectId) return "project";
  return "chat";
}

export function channelMarkType(item: InboxItem): ChannelType | null {
  const type = item.channelType ?? "";
  return CHANNEL_SOURCES.has(type) ? type as ChannelType : null;
}

export function inboxSourceLabel(item: InboxItem) {
  const source = inboxSource(item);
  if (source === "channel") return t("inbox.filter.channel");
  if (source === "project") return t("inbox.filter.project");
  if (source === "task") return t("nav.tasks");
  if (source === "workflow") return t("inbox.filter.workflow");
  return t("inbox.filter.chat");
}

export function inboxSourceTone(item: InboxItem): "primary" | "accent" | "purple" | "success" | "warning" {
  const source = inboxSource(item);
  if (source === "channel") return "success";
  if (source === "project") return "primary";
  if (source === "task") return "accent";
  if (source === "workflow") return "warning";
  return "purple";
}

export function inboxAgent(
  item: InboxItem,
  sessions: Array<{ id: string; targetType: string; targetId: string }>,
  agents: Agent[],
  squads: Squad[],
) {
  const session = sessions.find((entry) => entry.id === item.sessionId);
  if (!session) return { avatar: "M", name: t("inbox.agentFallback") };
  if (session.targetType === "squad") {
    const squad = squads.find((entry) => entry.id === session.targetId);
    return { avatar: "队", name: squad?.name ?? t("inbox.squadFallback") };
  }
  const agent = agents.find((entry) => entry.id === session.targetId);
  return { avatar: agent?.avatar ?? "M", name: agent?.name ?? t("inbox.agentFallback") };
}

export function inboxListTitle(item: InboxItem, contacts: FeishuContact[]) {
  if (inboxSource(item) !== "channel") return item.title;
  const contact = contacts.find((entry) => entry.sessionId === item.sessionId);
  const name = contact?.displayName?.trim() || contact?.note?.trim() || contact?.openId || "";
  if (!name) return item.title;
  return t("inbox.channel.private", { name });
}
