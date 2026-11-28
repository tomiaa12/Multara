export type RemoteRole = "viewer" | "runner" | "developer" | "owner";

export const REMOTE_ROLES: RemoteRole[] = ["viewer", "runner", "developer", "owner"];

export const REMOTE_PERMS = [
  "projectRead",
  "projectWrite",
  "projectDelete",
  "fileRead",
  "fileWrite",
  "fileDelete",
  "taskRead",
  "taskCreate",
  "taskManage",
  "agentPlan",
  "agentRun",
  "agentRunDirect",
  "gitRead",
  "gitWrite",
  "gitPush",
  "terminal",
] as const;

export type RemotePerm = (typeof REMOTE_PERMS)[number];

/** 申请把某个需求加入分组范围时用的伪权限。 */
export const PROJECT_REQUEST = "project";

export type RemoteError = {
  code: string;
  message: string;
  perm?: string;
  approvalId?: string;
};

export type RemoteHostStatus = {
  enabled: boolean;
  running: boolean;
  port: number;
  addresses: string[];
  fingerprint: string;
  fingerprintShort: string;
  error?: string | null;
};

export type RemotePairingCode = {
  code: string;
  expiresInSecs: number;
  link: string;
  qrSvg: string;
  fingerprint: string;
};

export type RemotePendingPairing = {
  id: string;
  deviceName: string;
  fingerprint: string;
  fingerprintShort: string;
  address: string;
};

export type RemoteGroup = {
  id: string;
  name: string;
  role: RemoteRole;
  allProjects: boolean;
  allAgents: boolean;
  shareInGroup: boolean;
  requireApproval: boolean;
  maxRunning: number;
  isDefault: boolean;
  projectIds: string[];
  agentIds: string[];
  deviceCount: number;
};

export type RemoteDevice = {
  id: string;
  name: string;
  ownerNote: string;
  fingerprint: string;
  groupId: string;
  pairedAt: string;
  lastSeenAt?: string | null;
  lastAddress?: string | null;
  online: boolean;
  running: number;
  totalTokens: number;
  costUsd: number;
};

export type RemoteRequestStatus = "pending" | "approved" | "rejected" | "cancelled";

export type RemotePermissionRequest = {
  id: string;
  deviceId: string;
  deviceName: string;
  permission: string;
  projectId?: string | null;
  note: string;
  status: RemoteRequestStatus;
  createdAt: string;
  decidedAt?: string | null;
};

export type RemoteApprovalStatus = "pending" | "approved" | "rejected" | "expired" | "used";

export type RemoteApproval = {
  id: string;
  deviceId: string;
  deviceName: string;
  cmd: string;
  argsHash: string;
  summary: string;
  status: RemoteApprovalStatus;
  createdAt: string;
  expiresAt: string;
  decidedAt?: string | null;
};

export type RemoteAuditEntry = {
  id: number;
  at: string;
  deviceId?: string | null;
  deviceName: string;
  groupId?: string | null;
  action: string;
  cmd?: string | null;
  resourceKind?: string | null;
  resourceId?: string | null;
  outcome: string;
  detail: unknown;
};

export type RemoteAuditQuery = {
  deviceId?: string | null;
  groupId?: string | null;
  action?: string | null;
  outcome?: string | null;
  beforeId?: number | null;
  limit?: number;
};

export type RemoteConnState = "connecting" | "online" | "offline" | "error";

export type RemoteHostConnStatus = {
  hostId: string;
  state: RemoteConnState;
  error?: string | null;
};

export type RemoteHost = {
  id: string;
  name: string;
  address: string;
  port: number;
  fingerprint: string;
  createdAt: string;
  lastConnectedAt?: string | null;
  status?: RemoteHostConnStatus | null;
};

/** `remote.hello` 返回的本设备在主机上的身份与权限。 */
export type RemoteHello = {
  hostName: string;
  device: { id: string; name: string };
  group: {
    id: string;
    name: string;
    role: RemoteRole;
    allProjects: boolean;
    shareInGroup: boolean;
    requireApproval: boolean;
    maxRunning: number;
  };
  perms: string[];
};

export type RemoteEventEnvelope = {
  hostId: string;
  event: string;
  payload: unknown;
};

/** 解析 `multara://pair?addr=&port=&fp=&code=` 配对链接。 */
export function parsePairingLink(text: string): { address: string; port: number; fingerprint: string; code: string } | null {
  const trimmed = text.trim();
  if (!trimmed.startsWith("multara://pair")) return null;
  const params = new URLSearchParams(trimmed.split("?")[1] ?? "");
  const port = Number(params.get("port"));
  const address = params.get("addr")?.trim() ?? "";
  if (!address || !Number.isInteger(port) || port <= 0) return null;
  return {
    address,
    port,
    fingerprint: params.get("fp") ?? "",
    code: params.get("code") ?? "",
  };
}
