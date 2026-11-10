// 界面操作回到会话的唯一入口（方案 §6）：
// - 受信的消息内组件（L1）：用户点击提交 = 直接发一条用户消息（sendGenUiAction）。
// - 不可信页面（Canvas / MCP App）：只能填输入框（fillComposerFromUi）或挂上下文（attachUiContext），
//   都不会自己触发运行。
import type { ChatMessage } from "../../domain/types";
import { t } from "../../i18n";
import { genUiLabel } from "./genui/registry";

export type UiSource = "canvas" | "mcp-app";

export type FillResult = { ok: true } | { ok: false; reason: "composer_not_empty" | "session_closed" };

/** 当前在界面上打开了某个会话的输入框（ChatPanel 注册）；同一会话可能多处打开，取最近注册的 */
type ComposerFiller = (text: string) => FillResult;
const fillers = new Map<string, ComposerFiller[]>();

export function registerComposerFiller(sessionId: string, filler: ComposerFiller) {
  const list = fillers.get(sessionId) ?? [];
  fillers.set(sessionId, [...list, filler]);
  return () => {
    const next = (fillers.get(sessionId) ?? []).filter((item) => item !== filler);
    if (next.length) fillers.set(sessionId, next);
    else fillers.delete(sessionId);
  };
}

/** 页面要往输入框里填一段话：输入框为空时填进去并聚焦，用户自己按发送 */
export function fillComposerFromUi(input: { sessionId: string; source: UiSource; sourceId: string; text: string }): FillResult {
  const list = fillers.get(input.sessionId);
  const filler = list?.[list.length - 1];
  if (!filler) return { ok: false, reason: "session_closed" };
  return filler(input.text);
}

/** 页面上下文附件的正文：带来源头，明确是数据不是指令 */
export function formatUiContexts(items: { source: UiSource; label: string; text: string | null; data: unknown }[]) {
  return items
    .map((item) => {
      const head = t(item.source === "canvas" ? "canvas.contextHeader" : "mcpApp.contextHeader", { title: item.label });
      const parts = [head];
      if (item.text?.trim()) parts.push(item.text.trim());
      if (item.data !== undefined && item.data !== null) {
        parts.push(`\`\`\`json\n${JSON.stringify(item.data, null, 2)}\n\`\`\``);
      }
      return parts.join("\n");
    })
    .join("\n\n");
}

export interface GenUiAction {
  sessionId: string;
  messageId: string;
  /** 产出该块的智能体；小队会话里提交回给它 */
  agentId?: string | null;
  blockId: string;
  type: string;
  title?: string;
  /** form 为 submit；card 为按钮 id */
  action: string;
  values?: unknown;
}

export interface GenUiSubmission {
  action: string;
  values: unknown;
}

type Send = (
  content: string,
  sessionId: string,
  attachments: [],
  options: { priority?: boolean; source: { sourceType: "genui"; meta: Record<string, unknown> } },
) => void;

/** 人和模型都能读：第一行是来源和动作，第二行是机读 JSON */
export function formatGenUiAction(action: Pick<GenUiAction, "blockId" | "type" | "title" | "action" | "values">) {
  const label = action.title?.trim() || genUiLabel(action.type);
  const payload: Record<string, unknown> = { blockId: action.blockId };
  if (action.values !== undefined) payload.values = action.values;
  return `[${t("genui.actionHeader")} · ${label}] ${action.action}\n${JSON.stringify(payload)}`;
}

export function sendGenUiAction(send: Send, action: GenUiAction) {
  send(formatGenUiAction(action), action.sessionId, [], {
    source: {
      sourceType: "genui",
      meta: {
        blockId: action.blockId,
        messageId: action.messageId,
        ...(action.agentId ? { agentId: action.agentId } : {}),
        type: action.type,
        action: action.action,
        ...(action.values !== undefined ? { values: action.values } : {}),
      },
    },
  });
}

/** 已点击、还在发送队列里的块（这条助手消息的） */
export function genUiSendingIds(
  queued: { source?: { sourceType: string; meta: Record<string, unknown> } }[],
  messageId: string,
): Set<string> {
  const ids = new Set<string>();
  for (const item of queued) {
    const meta = item.source?.sourceType === "genui" ? item.source.meta : null;
    if (meta?.messageId === messageId && typeof meta.blockId === "string") ids.add(meta.blockId);
  }
  return ids;
}

/**
 * 已提交的界面块：从这条助手消息之后、带 genui 来源的用户消息里推出来，不另存状态。
 * key 是 blockId。
 */
export function genUiSubmissions(messages: ChatMessage[], messageId: string): Map<string, GenUiSubmission> {
  const result = new Map<string, GenUiSubmission>();
  for (const message of messages) {
    if (message.role !== "user" || message.sourceType !== "genui") continue;
    const meta = message.sourceMeta ?? {};
    if (meta.messageId !== messageId || typeof meta.blockId !== "string") continue;
    if (result.has(meta.blockId)) continue;
    result.set(meta.blockId, {
      action: typeof meta.action === "string" ? meta.action : "",
      values: meta.values,
    });
  }
  return result;
}
