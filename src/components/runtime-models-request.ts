import { listRuntimeModels, type RuntimeModels } from "../services/backend";

const inFlight = new Map<string, Promise<RuntimeModels>>();

/** 同一运行时的并发查询共用一次后端调用（cursor 的 --list-models 约 3 秒）。 */
export function requestRuntimeModels(runtimeId: string, refresh = false): Promise<RuntimeModels> {
  const key = `${runtimeId}\n${refresh}`;
  const pending = inFlight.get(key);
  if (pending) return pending;
  const request = listRuntimeModels(runtimeId, refresh).finally(() => {
    inFlight.delete(key);
  });
  inFlight.set(key, request);
  return request;
}
