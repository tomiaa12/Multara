import type { NodeDef, WorkflowDef } from "./workflow";

function isBoundary(char: string | undefined): boolean {
  if (char === undefined) return true;
  return !/[\p{L}\p{N}_]/u.test(char);
}

/** 把 `prefix + from` 换成 `prefix + to`，后面紧挨着的字母数字不算同一个名字。 */
export function replacePath(source: string, prefix: string, from: string, to: string): string {
  if (!from || !to || from === to) return source;
  let out = "";
  let index = 0;
  while (index < source.length) {
    const at = source.indexOf(prefix, index);
    if (at < 0) {
      out += source.slice(index);
      break;
    }
    const before = at > 0 ? source[at - 1] : undefined;
    const rest = source.slice(at + prefix.length);
    if (isBoundary(before) && rest.startsWith(from) && isBoundary(rest[from.length])) {
      out += source.slice(index, at) + prefix + to;
      index = at + prefix.length + from.length;
    } else {
      out += source.slice(index, at + prefix.length);
      index = at + prefix.length;
    }
  }
  const head = prefix.endsWith(".") ? prefix.slice(0, -1) : prefix;
  return out
    .replaceAll(`${head}["${from}"]`, `${head}["${to}"]`)
    .replaceAll(`${head}['${from}']`, `${head}['${to}']`);
}

function replaceInTree(value: unknown, fn: (source: string) => string) {
  if (!value || typeof value !== "object") return;
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      if (typeof value[index] === "string") value[index] = fn(value[index]);
      else replaceInTree(value[index], fn);
    }
    return;
  }
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    const current = record[key];
    if (typeof current === "string") record[key] = fn(current);
    else replaceInTree(current, fn);
  }
}

function renameKey(map: Record<string, string> | undefined, from: string, to: string) {
  if (!map || !(from in map) || from === to || to in map) return;
  map[to] = map[from];
  delete map[from];
}

/** 输入端口改名：连线、本节点模板里的 inputs.旧名、子流程/遍历的入参键。 */
export function renameNodeInput(def: WorkflowDef, nodeId: string, from: string, to: string) {
  if (!from || !to || from === to) return;
  const node = def.nodes.find((item) => item.id === nodeId);
  if (!node) return;
  replaceInTree(node, (source) => replacePath(source, "inputs.", from, to));
  const config = node.config as {
    inputs?: Record<string, string>;
    extraInputs?: Record<string, string>;
    itemInput?: string;
  };
  renameKey(config.inputs, from, to);
  renameKey(config.extraInputs, from, to);
  if (config.itemInput === from) config.itemInput = to;
}

/** 输出端口改名：连线，以及全流程里的 nodes.<id>.outputs.旧名。触发器入参同时改 run.inputs。 */
export function renameNodeOutput(def: WorkflowDef, nodeId: string, from: string, to: string) {
  if (!from || !to || from === to) return;
  const prefix = `nodes.${nodeId}.outputs.`;
  replaceInTree(def, (source) => replacePath(source, prefix, from, to));
  const node = def.nodes.find((item) => item.id === nodeId);
  if (node?.kind === "trigger") replaceInTree(def, (source) => replacePath(source, "run.inputs.", from, to));
  retargetOutputFields(node, from, to);
}

function retargetOutputFields(node: NodeDef | undefined, from: string, to: string) {
  if (!node) return;
  const config = node.config as {
    cases?: { port: string }[];
    defaultPort?: string;
    decisions?: { port: string }[];
    assignments?: { port: string }[];
  };
  for (const item of config.cases ?? []) if (item.port === from) item.port = to;
  if (config.defaultPort === from) config.defaultPort = to;
  for (const item of config.decisions ?? []) if (item.port === from) item.port = to;
  for (const item of config.assignments ?? []) if (item.port === from) item.port = to;
}
