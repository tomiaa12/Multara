<script setup lang="ts">
import { computed, createVNode, nextTick, onBeforeUnmount, onMounted, ref, render, watch } from "vue";
import { Bot, FolderKanban, Hash, Sparkles } from "@lucide/vue";
import FileTypeIcon from "../FileTypeIcon.vue";
import Codicon from "../ui/Codicon.vue";
import type { EditorSelectionContext } from "../../domain/types";
import { composerClipboardHtml, decodeComposerClipboard } from "./composer-clipboard";
import {
  chipLabel,
  fileName,
  isWholeFileChip,
  lineRange,
  type ComposerFileChipNode,
  type ComposerNode,
  type ComposerReferenceChipNode,
} from "./composer-content";
import { createComposerIme } from "./composer-ime";
import { insertComposerLineBreak, LINE_BREAK_PAD, stripLineBreakPad } from "./composer-linebreak";

export type { ComposerNode };

const props = withDefaults(defineProps<{
  placeholder?: string;
  initialNodes?: ComposerNode[];
  contexts: EditorSelectionContext[];
  disabled?: boolean;
}>(), {
  disabled: false,
});

const emit = defineEmits<{
  change: [nodes: ComposerNode[]];
  submit: [];
  "paste-image": [files: File[]];
}>();

const editor = ref<HTMLDivElement | null>(null);
const chipText = new Map<string, string>();
const contextById = computed(() => new Map(props.contexts.map((item) => [item.id, item])));
const ime = createComposerIme();
let trackedContextIds = new Set<string>();

function unmountChip(chip: HTMLElement) {
  const id = chip.dataset.contextId;
  if (id) chipText.delete(id);
  const icon = chip.querySelector<HTMLElement>(".composer-chip-icon");
  if (icon) render(null, icon);
}

function createChip(item: ComposerNode & { type: "chip" } | EditorSelectionContext) {
  const chip = document.createElement("span");
  chip.className = "composer-chip";
  chip.contentEditable = "false";
  chip.dataset.contextId = item.id;
  const reference = "referenceKind" in item ? item as ComposerReferenceChipNode : null;
  const file = reference ? null : item as ComposerFileChipNode | EditorSelectionContext;
  if (reference) {
    chip.dataset.referenceId = reference.referenceId;
    chip.dataset.referenceKind = reference.referenceKind;
    chip.dataset.referenceLabel = reference.label;
    chip.dataset.chipKind = reference.referenceKind;
    chip.title = reference.label;
  } else {
    if (!file) return chip;
    chipText.set(file.id, file.text);
    chip.dataset.path = file.path;
    chip.dataset.startLine = String(file.startLine);
    chip.dataset.endLine = String(file.endLine);
    if (file.isDirectory) chip.dataset.directory = "1";
    if (file.source === "terminal") chip.dataset.source = "terminal";
    const kind = file.source === "terminal" ? "terminal" : file.isDirectory ? "folder" : isWholeFileChip(file) ? "file" : "line";
    chip.dataset.chipKind = kind;
    chip.title = kind === "line" ? `${file.path} ${lineRange(file)}` : file.path;
  }

  const icon = document.createElement("span");
  icon.className = "composer-chip-icon";
  if (reference) {
    let component = Bot;
    if (reference.referenceKind === "skill") component = Sparkles;
    if (reference.referenceKind === "task") component = Hash;
    if (reference.referenceKind === "requirement") component = FolderKanban;
    render(createVNode(component, { size: 12 }), icon);
  } else {
    if (!file) return chip;
    render(
      file.source === "terminal"
        ? createVNode(Codicon, { name: "terminal", size: 12 })
        : createVNode(FileTypeIcon, { name: fileName(file.path), directory: Boolean(file.isDirectory), size: 12 }),
      icon,
    );
  }

  const label = document.createElement("span");
  label.className = "composer-chip-label";
  label.textContent = chipLabel(reference ?? {
    type: "chip",
    id: file?.id ?? "",
    path: file?.path ?? "",
    startLine: file?.startLine ?? 0,
    endLine: file?.endLine ?? 0,
    text: file?.text ?? "",
    isDirectory: file?.isDirectory,
    source: file?.source,
  });

  chip.append(icon, label);
  return chip;
}

function contextFromChip(chip: HTMLElement): ComposerNode & { type: "chip" } {
  const id = chip.dataset.contextId ?? "";
  const referenceKind = chip.dataset.referenceKind;
  if (referenceKind && ["agent", "squad", "skill", "task", "requirement"].includes(referenceKind)) {
    return {
      type: "chip",
      id,
      referenceId: chip.dataset.referenceId ?? "",
      referenceKind: referenceKind as "agent" | "squad" | "skill" | "task" | "requirement",
      label: chip.dataset.referenceLabel ?? "",
    };
  }
  const stored = contextById.value.get(id);
  return {
    type: "chip",
    id,
    path: stored?.path ?? chip.dataset.path ?? "",
    startLine: stored?.startLine ?? Number(chip.dataset.startLine || "1"),
    endLine: stored?.endLine ?? Number(chip.dataset.endLine || "1"),
    text: stored ? stored.text : (chipText.get(id) ?? ""),
    isDirectory: stored?.isDirectory ?? chip.dataset.directory === "1",
    source: stored?.source ?? (chip.dataset.source === "terminal" ? "terminal" : undefined),
  };
}

function walk(node: Node, nodes: ComposerNode[]) {
  if (node.nodeType === Node.TEXT_NODE) {
    const raw = (node.textContent ?? "").replace(/\u00a0/g, " ");
    const value = stripLineBreakPad(raw);
    if (!value && raw.includes(LINE_BREAK_PAD)) return;
    nodes.push({ type: "text", value });
    return;
  }
  if (!(node instanceof HTMLElement)) return;
  if (node.classList.contains("composer-chip")) {
    nodes.push(contextFromChip(node));
    return;
  }
  if (node.nodeName === "BR") {
    nodes.push({ type: "text", value: "\n" });
    return;
  }
  if (node.nodeName === "DIV" && nodes.length) {
    const last = nodes.at(-1);
    if (last?.type === "text") {
      if (!last.value.endsWith("\n")) last.value += "\n";
    } else {
      nodes.push({ type: "text", value: "\n" });
    }
  }
  for (const child of Array.from(node.childNodes)) walk(child, nodes);
}

function snapshot(): ComposerNode[] {
  const nodes: ComposerNode[] = [];
  if (!editor.value) return nodes;
  for (const child of Array.from(editor.value.childNodes)) walk(child, nodes);
  return nodes;
}

function visibleText(nodes = snapshot()) {
  return nodes
    .map((node) => (node.type === "chip" ? chipLabel(node) : node.value))
    .join("")
    .replace(/\u00a0/g, " ")
    .trim();
}

function isEmpty(nodes = snapshot()) {
  if (nodes.some((node) => node.type === "chip")) return false;
  return !nodes.some((node) => node.type === "text" && node.value.replace(/\u00a0/g, " ").trim());
}

function hasFieldContent(nodes = snapshot()) {
  if (nodes.some((node) => node.type === "chip")) return true;
  return nodes.some((node) => node.type === "text" && stripLineBreakPad(node.value.replace(/\u00a0/g, " ")).length > 0);
}

function refreshEmpty(nodes = snapshot()) {
  editor.value?.classList.toggle("is-empty", !hasFieldContent(nodes));
}

function emitChange() {
  const nodes = snapshot();
  refreshEmpty(nodes);
  emit("change", nodes);
}

function placeCaretAfter(node: Node) {
  const selection = window.getSelection();
  if (!selection) return;
  const range = document.createRange();
  range.setStartAfter(node);
  range.collapse(true);
  selection.removeAllRanges();
  selection.addRange(range);
}

function insertText(value: string) {
  if (!value) return;
  focus();
  document.execCommand("insertText", false, value);
  emitChange();
}

function isPlaceholderOnly(root: HTMLElement) {
  if (root.querySelector(".composer-chip, br")) return false;
  return !stripLineBreakPad((root.textContent ?? "").replace(/\u00a0/g, " ")).length;
}

function insertChip(item: EditorSelectionContext) {
  const root = editor.value;
  if (!root) return;
  if (root.querySelector(`[data-context-id="${item.id}"]`)) return;

  const chip = createChip(item);
  const space = document.createTextNode("\u00a0");
  const selection = window.getSelection();
  const range = selection && selection.rangeCount ? selection.getRangeAt(0) : null;
  if (isPlaceholderOnly(root)) {
    root.replaceChildren(chip, space);
  } else if (range && root.contains(range.commonAncestorContainer)) {
    range.collapse(true);
    range.insertNode(space);
    range.insertNode(chip);
  } else {
    root.append(chip, space);
  }
  placeCaretAfter(space);
  emitChange();
}

function insertMissing(contexts: EditorSelectionContext[]) {
  const root = editor.value;
  if (!root) return;
  for (const item of contexts) {
    if (!root.querySelector(`[data-context-id="${item.id}"]`)) insertChip(item);
  }
}

function restore(nodes: ComposerNode[]) {
  const root = editor.value;
  if (!root) return;
  for (const chip of root.querySelectorAll<HTMLElement>(".composer-chip")) unmountChip(chip);
  root.replaceChildren();
  for (const node of nodes) {
    if (node.type === "text") {
      if (node.value.includes("\n")) {
        const parts = node.value.split("\n");
        parts.forEach((part, index) => {
          if (part) root.appendChild(document.createTextNode(part));
          if (index < parts.length - 1) root.appendChild(document.createElement("br"));
        });
      } else if (node.value) {
        root.appendChild(document.createTextNode(node.value));
      }
    } else {
      root.appendChild(createChip(node));
    }
  }
  refreshEmpty();
}

function clear() {
  const root = editor.value;
  if (!root) return;
  for (const chip of root.querySelectorAll<HTMLElement>(".composer-chip")) unmountChip(chip);
  root.replaceChildren();
  emitChange();
}

function focus() {
  editor.value?.focus();
  const selection = window.getSelection();
  const root = editor.value;
  if (!root || !selection) return;
  if (root.contains(selection.anchorNode)) return;
  const range = document.createRange();
  range.selectNodeContents(root);
  range.collapse(false);
  selection.removeAllRanges();
  selection.addRange(range);
}

function focusEnd() {
  const root = editor.value;
  if (!root) return;
  root.focus();
  const selection = window.getSelection();
  if (!selection) return;
  const range = document.createRange();
  range.selectNodeContents(root);
  range.collapse(false);
  selection.removeAllRanges();
  selection.addRange(range);
  root.scrollTop = root.scrollHeight;
}

function serialize() {
  const nodes = snapshot();
  return {
    text: visibleText(nodes),
    nodes,
    empty: isEmpty(nodes),
  };
}

function syncChipContexts() {
  const root = editor.value;
  const ids = new Set(props.contexts.map((item) => item.id));
  if (!root) {
    trackedContextIds = ids;
    return;
  }
  let changed = false;
  for (const chip of [...root.querySelectorAll<HTMLElement>(".composer-chip")]) {
    const id = chip.dataset.contextId ?? "";
    const stored = contextById.value.get(id);
    if (trackedContextIds.has(id) && !ids.has(id)) {
      const next = chip.nextSibling;
      if (next?.nodeType === Node.TEXT_NODE && next.textContent === "\u00a0") next.remove();
      unmountChip(chip);
      chip.remove();
      changed = true;
      continue;
    }
    if (stored) chipText.set(id, stored.text);
  }
  trackedContextIds = ids;
  if (changed) emitChange();
}

function isChipElement(node: Node | null): node is HTMLElement {
  return node instanceof HTMLElement && node.classList.contains("composer-chip");
}

function isSpacerText(node: Node | null): node is Text {
  return node?.nodeType === Node.TEXT_NODE && (node.textContent === "\u00a0" || node.textContent === " ");
}

function skipEmptyText(node: Node | null, direction: "backward" | "forward"): Node | null {
  let current = node;
  while (current?.nodeType === Node.TEXT_NODE && !(current.textContent ?? "").length) {
    current = direction === "backward" ? current.previousSibling : current.nextSibling;
  }
  return current;
}

function insideField(root: HTMLElement, node: Node) {
  return node === root || root.contains(node);
}

function removeChipNode(chip: HTMLElement) {
  const spacer = chip.nextSibling;
  const anchor = chip.previousSibling;
  const parent = chip.parentNode;
  if (isSpacerText(spacer)) spacer.remove();
  unmountChip(chip);
  chip.remove();
  const selection = window.getSelection();
  if (!selection || !parent) return;
  const range = document.createRange();
  if (anchor?.isConnected && anchor.parentNode === parent) range.setStartAfter(anchor);
  else range.setStart(parent, 0);
  range.collapse(true);
  selection.removeAllRanges();
  selection.addRange(range);
}

function deleteChipForKey(key: "Backspace" | "Delete") {
  const root = editor.value;
  const selection = window.getSelection();
  if (!root || !selection?.rangeCount) return false;
  const range = selection.getRangeAt(0);
  if (!insideField(root, range.commonAncestorContainer)) return false;

  if (!selection.isCollapsed) {
    const chips = [...root.querySelectorAll<HTMLElement>(".composer-chip")].filter((chip) =>
      range.intersectsNode(chip),
    );
    if (!chips.length) return false;
    for (const chip of chips) unmountChip(chip);
    range.deleteContents();
    return true;
  }

  const { startContainer, startOffset } = range;
  if (key === "Backspace") {
    let node: Node | null = null;
    if (startContainer.nodeType === Node.TEXT_NODE) {
      const text = startContainer.textContent ?? "";
      if (startOffset === 0) node = startContainer.previousSibling;
      else if ((text === "\u00a0" || text === " ") && startOffset === text.length) node = startContainer.previousSibling;
      else return false;
    } else if (startContainer instanceof HTMLElement) {
      node = startContainer.childNodes[startOffset - 1] ?? null;
    }
    node = skipEmptyText(node, "backward");
    if (isSpacerText(node)) node = skipEmptyText(node.previousSibling, "backward");
    if (!isChipElement(node)) return false;
    removeChipNode(node);
    return true;
  }

  let node: Node | null = null;
  if (startContainer.nodeType === Node.TEXT_NODE) {
    const text = startContainer.textContent ?? "";
    if (startOffset !== text.length) return false;
    node = startContainer.nextSibling;
  } else if (startContainer instanceof HTMLElement) {
    node = startContainer.childNodes[startOffset] ?? null;
  }
  node = skipEmptyText(node, "forward");
  if (isSpacerText(node)) node = skipEmptyText(node.nextSibling, "forward");
  if (!isChipElement(node)) return false;
  removeChipNode(node);
  return true;
}

function onKeydown(event: KeyboardEvent) {
  if (props.disabled) return;
  if ((event.key === "Backspace" || event.key === "Delete") && !event.isComposing) {
    if (deleteChipForKey(event.key)) {
      event.preventDefault();
      emitChange();
      return;
    }
  }
  if (event.key === "Enter" && !event.shiftKey) {
    const action = ime.classifyEnter(event);
    if (action === "ime") return;
    event.preventDefault();
    if (action === "submit") emit("submit");
    return;
  }
  if (event.key === "Enter" && event.shiftKey) {
    const action = ime.classifyEnter(event);
    if (action === "ime" || action === "confirm") return;
    event.preventDefault();
    if (editor.value) insertComposerLineBreak(editor.value);
    emitChange();
  }
}

function selectedNodes() {
  const root = editor.value;
  const selection = window.getSelection();
  if (!root || !selection?.rangeCount || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  if (!insideField(root, range.commonAncestorContainer)) return null;
  const holder = document.createElement("div");
  holder.append(range.cloneContents());
  const nodes: ComposerNode[] = [];
  for (const child of Array.from(holder.childNodes)) walk(child, nodes);
  return nodes.some((node) => node.type === "chip" || (node.type === "text" && node.value)) ? { range, nodes } : null;
}

function plainClipboardText(nodes: ComposerNode[]) {
  return nodes.map((node) => (node.type === "chip" ? chipLabel(node) : node.value)).join("");
}

function onCopy(event: ClipboardEvent, cut = false) {
  const selected = selectedNodes();
  if (!selected?.nodes.length) return;
  event.preventDefault();
  const plain = plainClipboardText(selected.nodes);
  event.clipboardData?.setData("text/plain", plain);
  event.clipboardData?.setData("text/html", composerClipboardHtml(selected.nodes, plain));
  if (!cut) return;
  for (const chip of [...(editor.value?.querySelectorAll<HTMLElement>(".composer-chip") ?? [])]) {
    if (selected.range.intersectsNode(chip)) unmountChip(chip);
  }
  selected.range.deleteContents();
  emitChange();
}

function insertNodesAtCaret(nodes: ComposerNode[]) {
  const root = editor.value;
  if (!root) return;
  focus();
  const selection = window.getSelection();
  const range = selection?.rangeCount && insideField(root, selection.getRangeAt(0).commonAncestorContainer)
    ? selection.getRangeAt(0)
    : null;
  range?.deleteContents();
  const fragment = document.createDocumentFragment();
  let last: Node | null = null;
  for (const node of nodes) {
    if (node.type === "chip") {
      const chip = createChip("referenceKind" in node ? node : { ...node, id: crypto.randomUUID() });
      const space = document.createTextNode("\u00a0");
      fragment.append(chip, space);
      last = space;
      continue;
    }
    const parts = node.value.split("\n");
    parts.forEach((part, index) => {
      if (part) {
        const text = document.createTextNode(part);
        fragment.append(text);
        last = text;
      }
      if (index < parts.length - 1) {
        const lineBreak = document.createElement("br");
        fragment.append(lineBreak);
        last = lineBreak;
      }
    });
  }
  if (isPlaceholderOnly(root)) root.replaceChildren(fragment);
  else if (range) range.insertNode(fragment);
  else root.append(fragment);
  if (last) placeCaretAfter(last);
  emitChange();
}

function onPaste(event: ClipboardEvent) {
  if (props.disabled) {
    event.preventDefault();
    return;
  }
  const seen = new Set<string>();
  const files: File[] = [];
  const candidates = [
    ...Array.from(event.clipboardData?.files ?? []),
    ...Array.from(event.clipboardData?.items ?? [])
      .map((item) => (item.type.startsWith("image/") ? item.getAsFile() : null))
      .filter((file): file is File => Boolean(file)),
  ];
  for (const file of candidates) {
    if (!file.type.startsWith("image/")) continue;
    const key = `${file.type}:${file.size}:${file.name}`;
    if (seen.has(key)) continue;
    seen.add(key);
    files.push(file);
  }
  if (files.length) {
    event.preventDefault();
    emit("paste-image", files);
    return;
  }
  const copied = decodeComposerClipboard(event.clipboardData?.getData("text/html") ?? "");
  if (copied?.length) {
    event.preventDefault();
    insertNodesAtCaret(copied);
    return;
  }
  const text = event.clipboardData?.getData("text/plain") ?? "";
  if (!text) return;
  event.preventDefault();
  document.execCommand("insertText", false, text);
  emitChange();
}

let externalDropHtml: string | null = null;

function restoreExternalDrop() {
  const root = editor.value;
  const html = externalDropHtml;
  if (!root || html === null || root.innerHTML === html) return;
  externalDropHtml = null;
  // unmountChip 会删 chipText；不在 props.contexts 里的芯片（粘贴、insertNodesAtCaret 创建的）要靠它取正文
  const savedText = new Map(chipText);
  for (const chip of root.querySelectorAll<HTMLElement>(".composer-chip")) unmountChip(chip);
  root.innerHTML = html;
  for (const [id, text] of savedText) chipText.set(id, text);
  emitChange();
}

function beginExternalDrop() {
  if (!editor.value || externalDropHtml !== null) return;
  externalDropHtml = editor.value.innerHTML;
  requestAnimationFrame(() => {
    restoreExternalDrop();
    externalDropHtml = null;
  });
}

function onBeforeInput(event: InputEvent) {
  if (externalDropHtml === null) return;
  event.preventDefault();
}

function onInput() {
  if (externalDropHtml !== null) {
    restoreExternalDrop();
    return;
  }
  emitChange();
}

onMounted(async () => {
  if (props.initialNodes?.length) restore(props.initialNodes);
  else refreshEmpty();
  syncChipContexts();
  await nextTick();
  emitChange();
});

watch(
  () => props.contexts.map((item) => `${item.id}:${item.text}`).join("|"),
  () => syncChipContexts(),
);

onBeforeUnmount(() => {
  ime.dispose();
  if (!editor.value) return;
  for (const chip of editor.value.querySelectorAll<HTMLElement>(".composer-chip")) unmountChip(chip);
});

defineExpose({
  focus,
  focusEnd,
  clear,
  insertChip,
  insertText,
  insertMissing,
  restore,
  snapshot,
  serialize,
  isEmpty,
  beginExternalDrop,
});
</script>

<template>
  <div
    ref="editor"
    class="composer-field vs-scroll is-empty"
    :contenteditable="disabled ? 'false' : 'true'"
    role="textbox"
    aria-multiline="true"
    :data-placeholder="placeholder"
    spellcheck="false"
    @keydown="onKeydown"
    @compositionstart="ime.compositionStart"
    @compositionend="ime.compositionEnd"
    @beforeinput="onBeforeInput"
    @copy="onCopy($event)"
    @cut="onCopy($event, true)"
    @paste="onPaste"
    @input="onInput"
  />
</template>
