import type { ChannelType } from "../domain/types";

export interface ChannelGrant {
  createTask: boolean;
  addAgent: boolean;
  addSquad: boolean;
  createAutomation: boolean;
  useWorkflow: boolean;
  readFiles: boolean;
  writeFiles: boolean;
  projectsUnrestricted: boolean;
  projectIds: string[];
  channel: Record<string, boolean>;
}

export interface GrantToggle {
  key: keyof Omit<ChannelGrant, "projectsUnrestricted" | "projectIds" | "channel">;
  labelKey: string;
}

export const APP_GRANT_TOGGLES: GrantToggle[] = [
  { key: "createTask", labelKey: "channels.perm.createTask" },
  { key: "addAgent", labelKey: "channels.perm.addAgent" },
  { key: "addSquad", labelKey: "channels.perm.addSquad" },
  { key: "createAutomation", labelKey: "channels.perm.createAutomation" },
  { key: "useWorkflow", labelKey: "channels.perm.useWorkflow" },
  { key: "readFiles", labelKey: "channels.perm.readFiles" },
  { key: "writeFiles", labelKey: "channels.perm.writeFiles" },
];

export function channelPermissionKeys(type: ChannelType): Array<{ key: string; labelKey: string }> {
  if (type === "feishu" || type === "lark") {
    return [
      { key: "comment", labelKey: "channels.perm.feishu.comment" },
      { key: "readDocs", labelKey: "channels.perm.feishu.readDocs" },
      { key: "editDocs", labelKey: "channels.perm.feishu.editDocs" },
      { key: "readBase", labelKey: "channels.perm.feishu.readBase" },
    ];
  }
  if (type === "slack") {
    return [
      { key: "history", labelKey: "channels.perm.slack.history" },
      { key: "react", labelKey: "channels.perm.slack.react" },
      { key: "upload", labelKey: "channels.perm.slack.upload" },
    ];
  }
  if (type === "dingtalk" || type === "wecom") {
    return [
      { key: "comment", labelKey: "channels.perm.feishu.comment" },
      { key: "readDocs", labelKey: "channels.perm.feishu.readDocs" },
    ];
  }
  if (type === "telegram" || type === "qq") {
    return [
      { key: "history", labelKey: "channels.perm.slack.history" },
      { key: "comment", labelKey: "channels.perm.feishu.comment" },
    ];
  }
  return [{ key: "history", labelKey: "channels.perm.weixin.history" }];
}

export function defaultChannelGrant(type: ChannelType): ChannelGrant {
  const channel: Record<string, boolean> = {};
  for (const item of channelPermissionKeys(type)) channel[item.key] = true;
  return {
    createTask: true,
    addAgent: true,
    addSquad: true,
    createAutomation: true,
    useWorkflow: true,
    readFiles: true,
    writeFiles: true,
    projectsUnrestricted: true,
    projectIds: [],
    channel,
  };
}

export function normalizeGrant(type: ChannelType, grant?: Partial<ChannelGrant> | null): ChannelGrant {
  const base = defaultChannelGrant(type);
  if (!grant) return base;
  return {
    ...base,
    ...grant,
    projectIds: grant.projectIds ?? [],
    channel: { ...base.channel, ...(grant.channel ?? {}) },
  };
}
