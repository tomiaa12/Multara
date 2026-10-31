import type { BootstrapData, ChatSession, RunMode, Runtime } from "../../domain/types";
import { translate } from "../../i18n";

export const RUN_MODES: RunMode[] = ["auto", "plan", "agent"];
export const DEFAULT_RUN_MODE: RunMode = "auto";
/** 后端没给能力声明时按「只支持 auto / agent」处理 */
const FALLBACK_MODES: RunMode[] = ["auto", "agent"];
const FALLBACK_UNSUPPORTED_REASON = "runtimeNoNativePlan";

type ModeData = Pick<BootstrapData, "agents" | "squads" | "runtimes"> | null | undefined;

/** 会话目标是小队时，模式能力看队长 */
export function sessionLeadAgentId(data: ModeData, session: Pick<ChatSession, "targetType" | "targetId"> | null | undefined) {
  if (session?.targetType !== "squad") return session?.targetId ?? "";
  return data?.squads.find((squad) => squad.id === session.targetId)?.leaderAgentId ?? "";
}

export function agentRuntime(data: ModeData, agentId: string): Runtime | undefined {
  const runtimeId = data?.agents.find((agent) => agent.id === agentId)?.runtimeId;
  return data?.runtimes.find((runtime) => runtime.id === runtimeId);
}

export function supportedModes(runtime: Runtime | undefined): RunMode[] {
  return runtime?.capabilities?.requestedModes?.length ? runtime.capabilities.requestedModes : FALLBACK_MODES;
}

export function modeSupported(runtime: Runtime | undefined, mode: RunMode) {
  return supportedModes(runtime).includes(mode);
}

/** 不支持某模式的原因；后端给 i18n key 时翻译，给的是现成短句时原样显示 */
export function modeUnsupportedReason(runtime: Runtime | undefined, language?: string): string {
  const reason = runtime?.capabilities?.planUnsupportedReason || FALLBACK_UNSUPPORTED_REASON;
  const key = `chat.mode.unsupported.${reason}`;
  const translated = translate(key, language);
  return translated === key ? reason : translated;
}

export function modeLabel(mode: RunMode, language?: string) {
  return translate(`chat.mode.${mode}`, language);
}

export function modeDescriptionKey(runtime: Runtime | undefined, mode: RunMode) {
  if (mode !== "auto") return `chat.mode.desc.${mode}`;
  return runtime?.capabilities?.autoPlan ? "chat.mode.desc.auto" : "chat.mode.desc.autoFallback";
}