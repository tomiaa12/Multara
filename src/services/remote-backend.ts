import { invoke } from "./dev-backend";
import { remoteInvoke } from "./remote-invoke";
import type {
  RemoteApproval,
  RemoteAuditEntry,
  RemoteAuditQuery,
  RemoteDevice,
  RemoteGroup,
  RemoteHello,
  RemoteHost,
  RemoteHostStatus,
  RemotePairingCode,
  RemotePendingPairing,
  RemotePermissionRequest,
} from "../domain/remote";

// ---------- 主机端（本机） ----------

export const hostStatus = () => invoke<RemoteHostStatus>("remote_host_get_status");
export const setHostEnabled = (enabled: boolean) => invoke<RemoteHostStatus>("remote_host_set_enabled", { enabled });
export const setHostPort = (port: number) => invoke<RemoteHostStatus>("remote_host_set_port", { port });

export const createPairingCode = () => invoke<RemotePairingCode>("remote_pairing_create");
export const cancelPairingCode = () => invoke<void>("remote_pairing_cancel");
export const pendingPairings = () => invoke<RemotePendingPairing[]>("remote_pairing_pending");
export const confirmPairing = (input: { id: string; accept: boolean; name: string; ownerNote: string; groupId: string }) =>
  invoke<RemoteDevice | null>("remote_pairing_confirm", input);

export const listDevices = () => invoke<RemoteDevice[]>("remote_devices_list");
export const updateDevice = (input: { id: string; name: string; ownerNote: string; groupId: string }) =>
  invoke<void>("remote_device_update", input);
export const disconnectDevice = (id: string) => invoke<void>("remote_device_disconnect", { id });
export const removeDevice = (id: string) => invoke<void>("remote_device_remove", { id });

export const listGroups = () => invoke<RemoteGroup[]>("remote_groups_list");
export const saveGroup = (group: RemoteGroup) => invoke<RemoteGroup>("remote_group_save", { group });
export const deleteGroup = (id: string) => invoke<void>("remote_group_delete", { id });

export const listRequests = () => invoke<RemotePermissionRequest[]>("remote_requests_list");
export const decideRequest = (id: string, action: "move" | "addProject" | "reject", groupId?: string) =>
  invoke<void>("remote_request_decide", { id, action, groupId: groupId ?? null });

export const listApprovals = (pendingOnly: boolean) => invoke<RemoteApproval[]>("remote_approvals_list", { pendingOnly });
export const decideApproval = (id: string, approve: boolean) => invoke<void>("remote_approval_decide", { id, approve });

export const queryAudit = (query: RemoteAuditQuery) => invoke<RemoteAuditEntry[]>("remote_audit_query", { query });

// ---------- 控制端（本机） ----------

export const listHosts = () => invoke<RemoteHost[]>("remote_hosts_list");
export const localFingerprint = () => invoke<string>("remote_identity");
export const pairHost = (input: { address: string; port: number; code: string; fingerprint: string; deviceName: string }) =>
  invoke<RemoteHost>("remote_host_pair", input);
export const updateHost = (input: { id: string; name: string; address: string; port: number }) =>
  invoke<void>("remote_host_update", input);
export const removeHost = (id: string) => invoke<void>("remote_host_remove", { id });
export const connectHost = (id: string) => invoke<void>("remote_host_connect", { id });
export const disconnectHost = (id: string) => invoke<void>("remote_host_disconnect", { id });

// ---------- 控制端发往远程主机 ----------

export const hello = (hostId: string) => remoteInvoke<RemoteHello>(hostId, "remote.hello");
export const requestPermission = (hostId: string, input: { permission: string; projectId?: string | null; note: string }) =>
  remoteInvoke<RemotePermissionRequest>(hostId, "remote.request_permission", input);
export const cancelRequest = (hostId: string, id: string) => remoteInvoke<void>(hostId, "remote.cancel_request", { id });
export const listMyRequests = (hostId: string) => remoteInvoke<RemotePermissionRequest[]>(hostId, "remote.list_my_requests");
