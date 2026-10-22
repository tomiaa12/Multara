import { confirmRollbackIfDirty } from "../components/workbench/unsaved-editor";
import { t } from "../i18n";

export function countMessagesFrom(messages: Array<{ id: string }>, targetId: string) {
  const index = messages.findIndex((item) => item.id === targetId);
  return index < 0 ? 0 : Math.max(0, messages.length - index - 1);
}

export function reaskBannerText(count: number) {
  return `从这里重新提问，之后的 ${count} 条消息将被清除`;
}

export function reshuffleHintText(count: number) {
  return t("diff.reshuffleHint", { count });
}

/** 这条以及之后会被删掉的消息里，只要有一轮拍过文件快照，就需要问回滚还是保持。 */
export function reaskTouchesFiles(
  messages: Array<{ id: string }>,
  targetId: string,
  filesByMessage: ReadonlyMap<string, { size: number }>,
) {
  const index = messages.findIndex((item) => item.id === targetId);
  if (index < 0) return false;
  return messages.slice(index).some((item) => (filesByMessage.get(item.id)?.size ?? 0) > 0);
}

export async function executeReask(input: {
  rollback: boolean;
  dirtyPaths: string[];
  confirmDirty: (message: string) => Promise<boolean>;
  confirmRollbackFailure?: (message: string) => Promise<boolean>;
  rollbackToTurn: () => Promise<void>;
  truncateSession: () => Promise<void>;
  reloadDiff: () => Promise<void>;
  send: () => void;
}): Promise<"aborted" | "sent"> {
  if (input.rollback) {
    const allowed = await confirmRollbackIfDirty(input.dirtyPaths, input.confirmDirty);
    if (!allowed) return "aborted";
    try {
      await input.rollbackToTurn();
    } catch (error) {
      const message = error instanceof Error ? error.message : "文件回滚失败";
      const proceed = input.confirmRollbackFailure
        ? await input.confirmRollbackFailure(message)
        : false;
      if (!proceed) return "aborted";
    }
  }
  await input.truncateSession();
  await input.reloadDiff();
  input.send();
  return "sent";
}
