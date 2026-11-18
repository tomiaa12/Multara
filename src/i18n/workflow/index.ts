import type { Locale, MessageCatalog } from "../types";
import core from "./core";
import inspector from "./inspector";
import run from "./run";
import existing from "./existing";

export type LocaleParts = Record<Locale, MessageCatalog>;

const parts: LocaleParts[] = [core, inspector, run, existing];

/** workflow.* 文案按功能拆成几个文件（core / inspector / run / existing），每个文件内含 5 种语言 */
export function workflowMessages(locale: Locale): MessageCatalog {
  return Object.assign({}, ...parts.map((part) => part[locale]));
}
