<script setup lang="ts">
import { computed, nextTick, onActivated, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Terminal } from "@xterm/xterm";
import type { ILink, IMarker } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { listen } from "@tauri-apps/api/event";
import "@xterm/xterm/css/xterm.css";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import type { TerminalProfile } from "../../services/backend";
import { composerClipboardHtml } from "../chat/composer-clipboard";
import ContextMenu, { type ContextMenuItem } from "../ui/ContextMenu.vue";
import TerminalFind from "./TerminalFind.vue";
import { isMacPlatform, terminalKeyCommand, terminalSendSequence } from "./terminal-keys";
import { detectFileLinks } from "./terminal-links";
import { detectQuickFixes, type TerminalQuickFix } from "./terminal-quick-fix";
import { placeTerminalAddButton, selectionLines, snapshotLines, terminalBlock } from "./terminal-selection";
import { ShellIntegration } from "./terminal-shell-integration";
import {
  rankTerminalSuggestions,
  suggestionInsertText,
  terminalCompletionDirectory,
  terminalInputToken,
  type TerminalSuggestion,
} from "./terminal-suggest";
import {
  TERMINAL_FONT_SIZE_KEY,
  TERMINAL_MAX_FONT_SIZE,
  TERMINAL_MIN_FONT_SIZE,
  terminalFontSize,
  terminalOptions,
  terminalTheme,
} from "./terminal-theme";

export interface TerminalTab {
  id: string;
  title: string;
  cwd: string;
  workspaceId: string;
  live: boolean;
  pending: boolean;
  message: string;
  groupId?: string;
  profileId?: string;
  shell?: string;
  pid?: number;
  icon?: string;
  color?: string;
  customTitle?: string;
  processTitle?: string;
  exitCode?: number | null;
  busy?: boolean;
}

const props = defineProps<{
  sessions: TerminalTab[];
  activeId: string;
  height: number;
  newTitle: string;
  profiles?: TerminalProfile[];
  defaultProfile?: string;
  open?: boolean;
}>();

const emit = defineEmits<{
  close: [];
  new: [profile?: string];
  split: [];
  select: [id: string];
  kill: [id: string];
  reorder: [fromId: string, toId: string, after: boolean];
  resize: [event: PointerEvent];
  patch: [id: string, patch: Partial<TerminalTab>];
  addToChat: [payload: { text: string; startLine: number; endLine: number }];
  openPath: [payload: { path: string; isDirectory: boolean; line?: number; column?: number }];
  openUrl: [url: string];
  defaultProfile: [id: string];
  runFile: [];
  runSelection: [];
}>();

const PUBLISH_DELAY_MS = 400;
const TAB_ICONS = ["terminal", "zap", "beaker", "rocket", "tools", "server", "database", "globe", "debug", "package"];
const TAB_COLORS = ["", "red", "green", "yellow", "blue", "magenta", "cyan"];
const mac = isMacPlatform();
const windows = navigator.platform.startsWith("Win");
const linux = /Linux/.test(navigator.userAgent);

const tabMenu = ref<{ x: number; y: number; id: string } | null>(null);
const emptyMenu = ref<{ x: number; y: number } | null>(null);
const instanceMenu = ref<{ x: number; y: number; id: string } | null>(null);
const newMenu = ref<{ x: number; y: number } | null>(null);
const iconPicker = ref<{ x: number; y: number; id: string } | null>(null);
const colorPicker = ref<{ x: number; y: number; id: string } | null>(null);
const addChat = ref<{ x: number; y: number; text: string; startLine: number; endLine: number } | null>(null);
const renameId = ref("");
const renameValue = ref("");
const renameInput = ref<HTMLInputElement | null>(null);
const findOpen = ref(false);
const findInitial = ref("");
const findSearch = ref<SearchAddon | null>(null);
const findRef = ref<{ focus: (value?: string) => void } | null>(null);
const suggestions = ref<TerminalSuggestion[]>([]);
const suggestIndex = ref(0);
const suggestTokenName = ref("");
const suggestPlace = ref({ x: 0, y: 0 });
const sticky = ref("");
const overlay = ref("");
const bell = ref(false);
const linkHint = ref<{ x: number; y: number } | null>(null);
const quickFix = ref<{ x: number; y: number; id: string; items: TerminalQuickFix[] } | null>(null);
const palette = ref<{ kind: "command" | "directory"; query: string; index: number } | null>(null);
const paletteInput = ref<HTMLInputElement | null>(null);
const sized = new Set<string>();
const paneWeights = ref<Record<string, number>>({});
const pasteConfirm = ref<{ id: string; text: string; lines: number } | null>(null);

type Host = {
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  integration: ShellIntegration;
  stop: () => void;
};

const hosts = new Map<string, Host>();
let suggestNonce = 0;
let commandNames: TerminalSuggestion[] | null = null;
const publishTimers = new Map<string, number>();
let pointerSelecting = false;
let pointerStart: { x: number; y: number } | null = null;
let overlayTimer = 0;
let bellTimer = 0;

const themeObserver = new MutationObserver(() => {
  const theme = terminalTheme();
  for (const { term } of hosts.values()) term.options.theme = theme;
});
const body = ref<HTMLElement | null>(null);
const tabsEl = ref<HTMLElement | null>(null);

const visibleSessions = computed(() => {
  const active = props.sessions.find((item) => item.id === props.activeId);
  if (!active) return props.sessions.filter((item) => item.id === props.activeId);
  const group = active.groupId;
  if (!group) return [active];
  const members = props.sessions.filter((item) => item.groupId === group);
  return members.length ? members : [active];
});

const paletteItems = computed(() => {
  const open = palette.value;
  if (!open) return [];
  const query = open.query.trim().toLowerCase();
  const host = hosts.get(props.activeId);
  if (!host) return [];
  const source = open.kind === "command"
    ? unique(host.integration.commands.map((item) => item.command).filter(Boolean).reverse())
    : unique(host.integration.commands.map((item) => item.cwd).filter(Boolean).reverse());
  return query ? source.filter((item) => item.toLowerCase().includes(query)) : source;
});

const tabMenuItems = computed<ContextMenuItem[]>(() => {
  const id = tabMenu.value?.id;
  if (!id) return [];
  const index = props.sessions.findIndex((item) => item.id === id);
  const session = props.sessions[index];
  return [
    { id: "split", label: t("terminal.split"), key: mac ? "⌘\\" : "Ctrl+Shift+5", disabled: !session?.live },
    { id: "rename", label: t("terminal.rename") },
    { id: "icon", label: t("terminal.changeIcon") },
    { id: "color", label: t("terminal.changeColor") },
    { id: "size", label: t("terminal.sizeToContent") },
    { type: "sep" },
    { id: "close", label: t("common.close") },
    { id: "close-others", label: t("workbench.closeOthers"), disabled: props.sessions.length < 2 },
    { id: "close-right", label: t("workbench.closeRight"), disabled: index < 0 || index >= props.sessions.length - 1 },
    { id: "close-all", label: t("workbench.closeAll"), disabled: props.sessions.length === 0 },
  ];
});

const emptyMenuItems = computed<ContextMenuItem[]>(() => [
  { id: "new", label: t("terminal.new") },
  { id: "profile", label: t("terminal.newWithProfile") },
]);

const instanceMenuItems = computed<ContextMenuItem[]>(() => {
  const id = instanceMenu.value?.id;
  const term = id ? hosts.get(id)?.term : undefined;
  const hasSelection = Boolean(term?.getSelection()?.trim());
  return [
    { id: "copy", label: t("common.copy"), key: mac ? "⌘C" : "Ctrl+C", disabled: !hasSelection },
    { id: "copy-html", label: t("terminal.copyHtml"), disabled: !hasSelection },
    { id: "paste", label: t("workbench.paste"), key: mac ? "⌘V" : "Ctrl+V" },
    { id: "select-all", label: t("terminal.selectAll") },
    { type: "sep" },
    { id: "clear", label: t("terminal.clear") },
    { id: "size", label: t("terminal.sizeToContent") },
    { type: "sep" },
    { id: "kill", label: t("terminal.kill") },
  ];
});

const newMenuItems = computed<ContextMenuItem[]>(() => {
  const profiles = props.profiles ?? [];
  const items: ContextMenuItem[] = profiles.map((item) => ({
    id: `profile:${item.id}`,
    label: item.name,
    checked: item.id === props.defaultProfile,
  }));
  if (items.length) items.push({ type: "sep" });
  items.push(
    { id: "split", label: t("terminal.splitTerminal"), disabled: !props.activeId },
    { id: "run-file", label: t("terminal.runActiveFile") },
    { id: "run-selection", label: t("terminal.runSelection") },
  );
  return items;
});

function unique(values: string[]) {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const value of values) {
    if (seen.has(value)) continue;
    seen.add(value);
    result.push(value);
  }
  return result.slice(0, 30);
}

function tabLabel(session: TerminalTab) {
  return session.customTitle || session.processTitle || session.title;
}

function tabTooltip(session: TerminalTab) {
  const lines = [tabLabel(session)];
  if (session.shell) lines.push(session.shell);
  if (session.pid) lines.push(`PID: ${session.pid}`);
  if (session.cwd) lines.push(session.cwd);
  if (session.exitCode != null) lines.push(t("terminal.exitCode", { code: session.exitCode }));
  return lines.join("\n");
}

function openTabMenu(id: string, event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  window.getSelection()?.removeAllRanges();
  tabMenu.value = { x: event.clientX, y: event.clientY, id };
}

function openEmptyMenu(event: MouseEvent) {
  if ((event.target as HTMLElement | null)?.closest(".bottom-panel-tab")) return;
  event.preventDefault();
  emptyMenu.value = { x: event.clientX, y: event.clientY };
}

function onTabMenu(action: string) {
  const id = tabMenu.value?.id;
  const point = tabMenu.value;
  tabMenu.value = null;
  if (!id) return;
  const index = props.sessions.findIndex((item) => item.id === id);
  if (action === "split") emit("split");
  if (action === "rename") startRename(id);
  if (action === "icon" && point) iconPicker.value = { x: point.x, y: point.y, id };
  if (action === "color" && point) colorPicker.value = { x: point.x, y: point.y, id };
  if (action === "size") toggleSizeToContent(id);
  if (action === "close") emit("kill", id);
  if (action === "close-others") {
    for (const session of props.sessions) {
      if (session.id !== id) emit("kill", session.id);
    }
  }
  if (action === "close-right") {
    for (const session of props.sessions.slice(index + 1)) emit("kill", session.id);
  }
  if (action === "close-all") {
    for (const session of props.sessions) emit("kill", session.id);
  }
}

function onEmptyMenu(action: string) {
  const point = emptyMenu.value;
  emptyMenu.value = null;
  if (action === "new") emit("new");
  if (action === "profile" && point) newMenu.value = point;
}

function onInstanceMenu(action: string) {
  const id = instanceMenu.value?.id;
  instanceMenu.value = null;
  if (!id) return;
  if (action === "copy") void copySelection(id);
  if (action === "copy-html") void copySelection(id, true);
  if (action === "paste") void paste(id);
  if (action === "select-all") hosts.get(id)?.term.selectAll();
  if (action === "clear") hosts.get(id)?.term.clear();
  if (action === "size") toggleSizeToContent(id);
  if (action === "kill") emit("kill", id);
}

function onNewMenu(action: string) {
  newMenu.value = null;
  if (action === "split") emit("split");
  if (action === "run-file") emit("runFile");
  if (action === "run-selection") emit("runSelection");
  if (action.startsWith("profile:")) {
    const id = action.slice("profile:".length);
    emit("defaultProfile", id);
    emit("new", id);
  }
}

function startRename(id: string) {
  const session = props.sessions.find((item) => item.id === id);
  renameId.value = id;
  renameValue.value = tabLabel(session ?? { title: "" } as TerminalTab);
  void nextTick(() => {
    renameInput.value?.focus();
    renameInput.value?.select();
  });
}

function commitRename() {
  const id = renameId.value;
  const value = renameValue.value.trim();
  renameId.value = "";
  if (!id) return;
  emit("patch", id, { customTitle: value || undefined });
}

function onNewButton(event: MouseEvent) {
  if (event.altKey) {
    emit("split");
    return;
  }
  emit("new");
}

function openNewMenu(event: MouseEvent) {
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  newMenu.value = { x: rect.left, y: rect.bottom + 4 };
}

let tabDrag: {
  id: string;
  pointerId: number;
  startX: number;
  moved: boolean;
  el: HTMLElement;
  originX: number;
  offsetX: number;
} | null = null;
let suppressTabClick = false;
let tabReorderBusy = false;

function clearTabDragVisual(el: HTMLElement | null | undefined) {
  if (!el) return;
  el.style.transform = "";
  el.style.zIndex = "";
  el.style.pointerEvents = "";
}

function applyTabDragTransform(clientX: number) {
  if (!tabDrag) return;
  const translateX = clientX - tabDrag.originX + tabDrag.offsetX;
  tabDrag.el.style.transform = `translateX(${translateX}px)`;
  tabDrag.el.style.zIndex = "8";
}

function onTabPointerDown(id: string, event: PointerEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest("i, input")) return;
  const el = target?.closest<HTMLElement>(".bottom-panel-tab");
  if (!el) return;
  tabDrag = {
    id,
    pointerId: event.pointerId,
    startX: event.clientX,
    moved: false,
    el,
    originX: event.clientX,
    offsetX: 0,
  };
  window.addEventListener("pointermove", onTabPointerMove);
  window.addEventListener("pointerup", onTabPointerUp);
  window.addEventListener("pointercancel", onTabPointerUp);
}

async function onTabPointerMove(event: PointerEvent) {
  if (!tabDrag || event.pointerId !== tabDrag.pointerId) return;
  if (!tabDrag.moved && Math.abs(event.clientX - tabDrag.startX) < 4) return;
  if (!tabDrag.moved) {
    tabDrag.moved = true;
    tabDrag.originX = event.clientX;
    tabDrag.offsetX = 0;
    document.body.classList.add("is-dragging-terminal-tab");
  }
  applyTabDragTransform(event.clientX);
  if (tabReorderBusy) return;
  tabDrag.el.style.pointerEvents = "none";
  const hit = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>(".bottom-panel-tab");
  tabDrag.el.style.pointerEvents = "";
  const toId = hit?.dataset.terminalTab;
  if (!hit || !toId || toId === tabDrag.id) return;
  const rect = hit.getBoundingClientRect();
  const after = event.clientX > rect.left + rect.width / 2;
  const visualLeft = tabDrag.el.getBoundingClientRect().left;
  tabReorderBusy = true;
  emit("reorder", tabDrag.id, toId, after);
  await nextTick();
  if (!tabDrag) {
    tabReorderBusy = false;
    return;
  }
  tabDrag.el.style.transform = "";
  const naturalLeft = tabDrag.el.getBoundingClientRect().left;
  tabDrag.offsetX = visualLeft - naturalLeft;
  tabDrag.originX = event.clientX;
  applyTabDragTransform(event.clientX);
  tabReorderBusy = false;
}

function onTabPointerUp(event: PointerEvent) {
  if (!tabDrag || event.pointerId !== tabDrag.pointerId) return;
  suppressTabClick = tabDrag.moved;
  clearTabDragVisual(tabDrag.el);
  tabDrag = null;
  tabReorderBusy = false;
  document.body.classList.remove("is-dragging-terminal-tab");
  window.removeEventListener("pointermove", onTabPointerMove);
  window.removeEventListener("pointerup", onTabPointerUp);
  window.removeEventListener("pointercancel", onTabPointerUp);
}

function onTabsWheel(event: WheelEvent) {
  const root = tabsEl.value;
  if (!root || event.deltaY === 0) return;
  event.preventDefault();
  root.scrollLeft += event.deltaY;
}

function onTabClick(id: string) {
  if (suppressTabClick) {
    suppressTabClick = false;
    return;
  }
  emit("select", id);
}

function onTabDblClick(id: string) {
  startRename(id);
}

function onTabsDblClick(event: MouseEvent) {
  if ((event.target as HTMLElement | null)?.closest(".bottom-panel-tab")) return;
  emit("new");
}

function onResize(event: PointerEvent) {
  const handle = event.currentTarget;
  if (handle instanceof HTMLElement) {
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      /* 合成事件不能捕获指针 */
    }
  }
  emit("resize", event);
}

function hostEl(id: string) {
  return body.value?.querySelector<HTMLElement>(`[data-terminal-id="${id}"]`) ?? null;
}

function showOverlay(text: string) {
  overlay.value = text;
  window.clearTimeout(overlayTimer);
  overlayTimer = window.setTimeout(() => {
    overlay.value = "";
  }, 800);
}

function setFontSize(size: number) {
  const next = Math.min(TERMINAL_MAX_FONT_SIZE, Math.max(TERMINAL_MIN_FONT_SIZE, Math.round(size)));
  localStorage.setItem(TERMINAL_FONT_SIZE_KEY, String(next));
  for (const host of hosts.values()) {
    host.term.options.fontSize = next;
    host.fit.fit();
  }
  const active = hosts.get(props.activeId);
  if (active) showOverlay(`${active.term.cols} × ${active.term.rows}`);
}

function onTerminalWheel(event: WheelEvent) {
  if (!(event.ctrlKey || event.metaKey)) return;
  event.preventDefault();
  setFontSize(terminalFontSize() + (event.deltaY < 0 ? 1 : -1));
}

async function mount(session: TerminalTab) {
  await nextTick();
  const element = hostEl(session.id);
  if (!element || hosts.has(session.id)) return;
  const term = new Terminal(terminalOptions());
  const fit = new FitAddon();
  const search = new SearchAddon();
  const unicode = new Unicode11Addon();
  term.loadAddon(fit);
  term.loadAddon(search);
  term.loadAddon(unicode);
  try {
    term.unicode.activeVersion = "11";
  } catch {
    /* 旧版 xterm 没有 unicode 11 */
  }
  try {
    term.loadAddon(new WebglAddon());
  } catch {
    /* 没有 GPU 时退回 canvas */
  }
  term.loadAddon(new WebLinksAddon((event, uri) => {
    if (event.ctrlKey || event.metaKey) emit("openUrl", uri);
  }));
  term.open(element);
  element.querySelector(".xterm-viewport")?.classList.add("vs-scroll");
  fit.fit();
  const integration = new ShellIntegration(term, session.cwd, {
    onCwd: (cwd) => emit("patch", session.id, { cwd }),
    onFinished: (command) => {
      emit("patch", session.id, { busy: false });
      decorateCommand(term, command.promptMarker as IMarker, command.exitCode);
      const output = integration.commandOutput(command);
      const fixes = detectQuickFixes(command.command, output, command.exitCode);
      if (fixes.length) {
        const decoration = term.registerDecoration({ marker: command.promptMarker, width: 2 });
        decoration?.onRender((el) => {
          el.className = "terminal-quick-fix";
          el.title = t("terminal.quickFix");
          el.onclick = (event) => {
            event.stopPropagation();
            const rect = el.getBoundingClientRect();
            quickFix.value = { x: rect.left, y: rect.bottom, id: session.id, items: fixes };
          };
        });
      }
    },
    onDetected: () => emit("patch", session.id, {}),
  });
  const stops: Array<() => void> = [];
  if (!session.live) {
    term.write(`${session.message || t("terminal.desktopOnly")}\r\n`);
  } else {
    term.onData((data) => {
      const current = props.sessions.find((item) => item.id === session.id);
      if (!current?.live) return;
      void backend.terminalWrite(session.id, data);
    });
    term.attachCustomKeyEventHandler((event) => onTerminalKey(session.id, event));
    element.addEventListener("copy", (event) => onTerminalCopy(session.id, event));
    element.addEventListener("paste", (event) => {
      event.preventDefault();
      void paste(session.id, event.clipboardData?.getData("text/plain"));
    });
    term.onTitleChange((title) => emit("patch", session.id, { processTitle: title }));
    term.onBell(() => {
      bell.value = true;
      window.clearTimeout(bellTimer);
      bellTimer = window.setTimeout(() => {
        bell.value = false;
      }, 400);
    });
    term.onScroll(() => updateSticky(session.id));
    const links = registerFileLinks(term, () => props.sessions.find((item) => item.id === session.id)?.cwd ?? session.cwd);
    void backend.terminalResize(session.id, term.cols, term.rows);
    const data = await listen<{ id: string; data: string }>("terminal-data", (event) => {
      if (event.payload.id !== session.id) return;
      const binary = Uint8Array.from(atob(event.payload.data), (char) => char.charCodeAt(0));
      term.write(binary, () => {
        schedulePublish(session.id);
        emit("patch", session.id, { busy: integration.isExecuting() });
        if (session.id === props.activeId) scheduleSuggest(session.id);
        updateSticky(session.id);
      });
    });
    term.onSelectionChange(() => {
      if (pointerSelecting || session.id !== props.activeId) return;
      showAddChat();
    });
    const exit = await listen<{ id: string; exitCode?: number }>("terminal-exit", (event) => {
      if (event.payload.id !== session.id) return;
      const code = event.payload.exitCode ?? 1;
      term.write(`\r\n\x1b[90m${t("terminal.processExited", { code })}\x1b[0m\r\n`);
      emit("patch", session.id, { live: false, busy: false, exitCode: code });
    });
    stops.push(data, exit, () => links.dispose());
  }
  hosts.set(session.id, {
    term,
    fit,
    search,
    integration,
    stop: () => {
      for (const stop of stops) stop();
      integration.dispose();
      term.dispose();
    },
  });
  if (session.id === props.activeId) term.focus();
}

function decorateCommand(term: Terminal, marker: IMarker | undefined, exitCode: number | undefined) {
  if (!marker || marker.isDisposed) return;
  const decoration = term.registerDecoration({
    marker,
    overviewRulerOptions: {
      color: exitCode ? "#f14c4c" : "#3fb950",
      position: "left",
    },
  });
  decoration?.onRender((el) => {
    el.className = `terminal-command-decoration ${exitCode ? "error" : "success"}`;
    el.title = exitCode ? t("terminal.exitCode", { code: exitCode }) : t("terminal.success");
  });
}

function registerFileLinks(term: Terminal, cwdOf: () => string) {
  return term.registerLinkProvider({
    provideLinks(y, callback) {
      const line = term.buffer.active.getLine(y - 1);
      if (!line) {
        callback(undefined);
        return;
      }
      const text = line.translateToString(true);
      const candidates = detectFileLinks(text);
      if (!candidates.length) {
        callback(undefined);
        return;
      }
      void backend.terminalResolvePaths(cwdOf(), candidates.map((item) => item.path)).then((resolved) => {
        const links: ILink[] = [];
        candidates.forEach((candidate, index) => {
          const hit = resolved[index];
          if (!hit) return;
          links.push({
            text: candidate.path,
            range: { start: { x: candidate.start + 1, y }, end: { x: candidate.end, y } },
            activate(event) {
              if (event.ctrlKey || event.metaKey) {
                emit("openPath", { ...hit, line: candidate.line, column: candidate.column });
              }
            },
            hover(event) {
              const root = body.value?.getBoundingClientRect();
              if (!root) return;
              linkHint.value = {
                x: event.clientX - root.left,
                y: event.clientY - root.top + 16,
              };
            },
            leave() {
              linkHint.value = null;
            },
          });
        });
        callback(links.length ? links : undefined);
      });
    },
  });
}

function unmount(id: string) {
  const timer = publishTimers.get(id);
  if (timer) window.clearTimeout(timer);
  publishTimers.delete(id);
  hosts.get(id)?.stop();
  hosts.delete(id);
}

function clearSuggestions() {
  suggestNonce += 1;
  suggestions.value = [];
  suggestIndex.value = 0;
  suggestTokenName.value = "";
}

function moveSuggestion(delta: number) {
  const count = suggestions.value.length;
  if (!count) return;
  suggestIndex.value = (suggestIndex.value + delta + count) % count;
}

function acceptSuggestion(id: string) {
  const pick = suggestions.value[suggestIndex.value];
  const typed = suggestTokenName.value;
  clearSuggestions();
  if (!pick) return;
  const session = props.sessions.find((item) => item.id === id);
  if (!session?.live) return;
  void backend.terminalWrite(id, suggestionInsertText(typed, pick, windows));
}

function onTerminalKey(id: string, event: KeyboardEvent) {
  if (event.type !== "keydown") return true;
  if (id === props.activeId && suggestions.value.length) {
    if (event.key === "ArrowDown") {
      moveSuggestion(1);
      return false;
    }
    if (event.key === "ArrowUp") {
      moveSuggestion(-1);
      return false;
    }
    if (event.key === "Escape") {
      clearSuggestions();
      return false;
    }
    if (event.key === "Tab" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      acceptSuggestion(id);
      return false;
    }
  }
  const term = hosts.get(id)?.term;
  const command = terminalKeyCommand(event, { hasSelection: Boolean(term?.getSelection()?.trim()) });
  if (command) {
    void runKeyCommand(id, command);
    return false;
  }
  const sequence = terminalSendSequence(event);
  if (!sequence) return true;
  const session = props.sessions.find((item) => item.id === id);
  if (session?.live) void backend.terminalWrite(id, sequence);
  return false;
}

async function runKeyCommand(id: string, command: ReturnType<typeof terminalKeyCommand>) {
  const host = hosts.get(id);
  if (!host || !command) return;
  if (command === "copy") return copySelection(id);
  if (command === "paste") return paste(id);
  if (command === "find") return openFind();
  if (command === "clear") {
    host.term.clear();
    return;
  }
  if (command === "selectAll") {
    host.term.selectAll();
    return;
  }
  if (command === "previousCommand") return jumpCommand(id, -1);
  if (command === "nextCommand") return jumpCommand(id, 1);
  if (command === "scrollLineUp") host.term.scrollLines(-1);
  if (command === "scrollLineDown") host.term.scrollLines(1);
  if (command === "scrollPageUp") host.term.scrollPages(-1);
  if (command === "scrollPageDown") host.term.scrollPages(1);
  if (command === "scrollTop") host.term.scrollToTop();
  if (command === "scrollBottom") host.term.scrollToBottom();
  if (command === "split") emit("split");
  if (command === "focusNext") focusGroup(1);
  if (command === "focusPrevious") focusGroup(-1);
  if (command === "recentCommand") openPalette("command");
  if (command === "recentDirectory") openPalette("directory");
  if (command === "suggest") void refreshSuggestions(id, ++suggestNonce, true);
}

function focusGroup(delta: number) {
  const ids = visibleSessions.value.map((item) => item.id);
  const index = ids.indexOf(props.activeId);
  if (index < 0 || ids.length < 2) return;
  emit("select", ids[(index + delta + ids.length) % ids.length]!);
}

function jumpCommand(id: string, delta: number) {
  const host = hosts.get(id);
  if (!host) return;
  const lines = host.integration.promptLines();
  if (!lines.length) return;
  const current = host.term.buffer.active.viewportY;
  const next = delta < 0
    ? [...lines].reverse().find((line) => line < current)
    : lines.find((line) => line > current);
  if (next == null) return;
  host.term.scrollToLine(next);
}

function openFind() {
  const host = hosts.get(props.activeId);
  findSearch.value = host?.search ?? null;
  const selection = host?.term.getSelection() ?? "";
  findInitial.value = selection.split("\n")[0] ?? "";
  if (findOpen.value) {
    findRef.value?.focus(findInitial.value);
    return;
  }
  findOpen.value = true;
}

function openPalette(kind: "command" | "directory") {
  palette.value = { kind, query: "", index: 0 };
  void nextTick(() => paletteInput.value?.focus());
}

function confirmPalette() {
  const items = paletteItems.value;
  const pick = items[palette.value?.index ?? 0] ?? items[0];
  const kind = palette.value?.kind;
  palette.value = null;
  const session = props.sessions.find((item) => item.id === props.activeId);
  if (!pick || !kind || !session?.live) return;
  const text = kind === "directory" ? `cd ${quotePath(pick)}\n` : `${pick}\n`;
  void backend.terminalWrite(session.id, text);
}

function quotePath(path: string) {
  if (!/[\s()]/.test(path)) return path;
  return windows ? `'${path.replaceAll("'", "''")}'` : `'${path.replaceAll("'", `'\\''`)}'`;
}

function scheduleSuggest(id: string) {
  const nonce = ++suggestNonce;
  window.setTimeout(() => {
    if (nonce === suggestNonce) void refreshSuggestions(id, nonce, false);
  }, 80);
}

async function refreshSuggestions(id: string, nonce: number, forced: boolean) {
  const mounted = hosts.get(id);
  const session = props.sessions.find((item) => item.id === id);
  if (!mounted || !session?.live || id !== props.activeId) {
    if (nonce === suggestNonce) clearSuggestions();
    return;
  }
  const buffer = mounted.term.buffer.active;
  const cursor = buffer.cursorY + buffer.baseY;
  const line = buffer.getLine(cursor)?.translateToString(false) ?? "";
  const start = mounted.integration.inputStart();
  const inputStart = start && start.row === cursor ? start.x : start && start.row < cursor ? 0 : null;
  const token = terminalInputToken(line, buffer.cursorX, inputStart);
  if (!token && !forced) {
    if (nonce === suggestNonce) clearSuggestions();
    return;
  }
  const kind = token?.kind ?? "command";
  const name = token?.name ?? "";
  const candidates = kind === "command"
    ? await loadCommandNames()
    : await loadPathNames(session.cwd, token?.directory ?? "");
  if (nonce !== suggestNonce) return;
  suggestions.value = name ? rankTerminalSuggestions(candidates, name) : candidates.slice(0, 10);
  suggestIndex.value = 0;
  suggestTokenName.value = name;
  const element = hostEl(id);
  if (element) placeSuggestions(mounted.term, element);
}

async function loadCommandNames() {
  if (commandNames) return commandNames;
  try {
    commandNames = (await backend.terminalCommandNames()).map((name) => ({ name, kind: "command" as const }));
  } catch {
    commandNames = [];
  }
  return commandNames;
}

async function loadPathNames(cwd: string, directory: string) {
  const base = terminalCompletionDirectory(cwd, directory, windows);
  try {
    const entries = await backend.listExternalDirectory(base);
    return entries.map((entry) => ({
      name: entry.name,
      kind: entry.isDirectory ? "folder" as const : "file" as const,
    }));
  } catch {
    return [];
  }
}

function placeSuggestions(term: Terminal, element: HTMLElement) {
  const root = body.value;
  const screen = element.querySelector(".xterm-screen");
  const rows = element.querySelector(".xterm-rows");
  if (!root || !(screen instanceof HTMLElement) || !(rows instanceof HTMLElement)) return;
  if (!term.rows || !term.cols) return;
  const buffer = term.buffer.active;
  const cellWidth = rows.clientWidth / term.cols;
  const cellHeight = rows.clientHeight / term.rows;
  const screenRect = screen.getBoundingClientRect();
  const rootRect = root.getBoundingClientRect();
  const cursorX = screenRect.left - rootRect.left + buffer.cursorX * cellWidth;
  const below = screenRect.top - rootRect.top + (buffer.cursorY + 1) * cellHeight;
  const listHeight = Math.min(180, suggestions.value.length * 22 + 8);
  const y = rootRect.height - below < listHeight && below > listHeight
    ? below - listHeight - cellHeight
    : below;
  suggestPlace.value = {
    x: Math.max(0, Math.min(cursorX, rootRect.width - 280)),
    y: Math.max(0, y),
  };
}

function schedulePublish(id: string) {
  const existing = publishTimers.get(id);
  if (existing) window.clearTimeout(existing);
  publishTimers.set(id, window.setTimeout(() => {
    publishTimers.delete(id);
    void publishTerminal(id);
  }, PUBLISH_DELAY_MS));
}

function publishTerminal(id: string) {
  const mounted = hosts.get(id);
  const session = props.sessions.find((item) => item.id === id);
  if (!mounted || !session?.live) return;
  const buffer = mounted.term.buffer.active;
  const lines: string[] = [];
  for (let index = 0; index < buffer.length; index += 1) {
    lines.push(buffer.getLine(index)?.translateToString(true) ?? "");
  }
  const snapshot = snapshotLines(lines);
  void backend.terminalPublish({
    id,
    title: tabLabel(session),
    cwd: session.cwd,
    lineOffset: snapshot.lineOffset,
    text: snapshot.text,
    active: id === props.activeId,
  }).catch(() => undefined);
}

function readSelection(term: Terminal) {
  const text = term.getSelection();
  const position = term.getSelectionPosition();
  if (!text.trim() || !position) return null;
  return { text, ...selectionLines(position.start.y, position.end.y) };
}

function selectionBlockTop(term: Terminal, element: HTMLElement, root: HTMLElement) {
  const position = term.getSelectionPosition();
  const screen = element.querySelector(".xterm-screen");
  const rows = element.querySelector(".xterm-rows");
  if (!position || !(screen instanceof HTMLElement) || !(rows instanceof HTMLElement)) return null;
  if (!term.rows) return null;
  const viewportRow = position.start.y - term.buffer.active.viewportY;
  const cellHeight = rows.clientHeight / term.rows;
  const screenRect = screen.getBoundingClientRect();
  const rootRect = root.getBoundingClientRect();
  return screenRect.top - rootRect.top + viewportRow * cellHeight;
}

function showAddChat() {
  const root = body.value;
  const element = hostEl(props.activeId);
  const term = hosts.get(props.activeId)?.term;
  const selection = term ? readSelection(term) : null;
  if (!root || !element || !term || !selection) {
    addChat.value = null;
    return;
  }
  const top = selectionBlockTop(term, element, root);
  if (top === null) {
    addChat.value = null;
    return;
  }
  const rect = root.getBoundingClientRect();
  const placed = placeTerminalAddButton({ top }, { width: rect.width, height: rect.height });
  addChat.value = { ...placed, text: selection.text, startLine: selection.startLine, endLine: selection.endLine };
}

function onTerminalPointerDown(event: PointerEvent) {
  if (event.button === 1 && linux) {
    event.preventDefault();
    void paste(props.activeId);
    return;
  }
  pointerSelecting = true;
  pointerStart = { x: event.clientX, y: event.clientY };
  addChat.value = null;
}

function onTerminalContextMenu(id: string, event: MouseEvent) {
  event.preventDefault();
  const term = hosts.get(id)?.term;
  const hasSelection = Boolean(term?.getSelection()?.trim());
  if (!mac && !linux) {
    if (hasSelection) {
      void copySelection(id);
      term?.clearSelection();
    } else {
      void paste(id);
    }
    return;
  }
  instanceMenu.value = { x: event.clientX, y: event.clientY, id };
}

function onWindowPointerUp(event: PointerEvent) {
  if (!pointerSelecting) return;
  pointerSelecting = false;
  const root = body.value;
  const start = pointerStart;
  pointerStart = null;
  if (!root || !(event.target instanceof Node) || !root.contains(event.target)) {
    addChat.value = null;
    return;
  }
  const moved = start ? Math.hypot(event.clientX - start.x, event.clientY - start.y) : 0;
  const term = hosts.get(props.activeId)?.term;
  const element = hostEl(props.activeId);
  if (moved < 4 && term && element) selectBlock(term, element, event.clientY);
  window.setTimeout(() => showAddChat(), 0);
}

function bufferLines(term: Terminal) {
  const buffer = term.buffer.active;
  const lines: string[] = [];
  for (let index = 0; index < buffer.length; index += 1) {
    lines.push(buffer.getLine(index)?.translateToString(true) ?? "");
  }
  return lines;
}

function rowFromPointer(term: Terminal, element: HTMLElement, clientY: number) {
  const screen = element.querySelector(".xterm-screen");
  const rows = element.querySelector(".xterm-rows");
  if (!(screen instanceof HTMLElement) || !(rows instanceof HTMLElement) || !term.rows) return null;
  const screenRect = screen.getBoundingClientRect();
  const cellHeight = rows.clientHeight / term.rows;
  if (!cellHeight) return null;
  const viewportRow = Math.floor((clientY - screenRect.top) / cellHeight);
  if (viewportRow < 0 || viewportRow >= term.rows) return null;
  return term.buffer.active.viewportY + viewportRow;
}

function selectBlock(term: Terminal, element: HTMLElement, clientY: number) {
  const row = rowFromPointer(term, element, clientY);
  if (row === null) return;
  const host = [...hosts.values()].find((item) => item.term === term);
  const command = host?.integration.commandAtLine(row);
  if (host && command) {
    const range = host.integration.commandRange(command);
    term.selectLines(range.start, range.end);
    return;
  }
  const block = terminalBlock(bufferLines(term), row);
  if (!block) return;
  term.selectLines(block.start, block.end);
}

function onTerminalCopy(id: string, event: ClipboardEvent) {
  const term = hosts.get(id)?.term;
  const selection = term ? readSelection(term) : null;
  if (!selection || !event.clipboardData) return;
  event.preventDefault();
  writeClipboard(event.clipboardData, selection.text, selection.startLine, selection.endLine);
}

function writeClipboard(clipboard: DataTransfer, text: string, startLine: number, endLine: number) {
  const chip = {
    type: "chip" as const,
    id: crypto.randomUUID(),
    path: t("terminal.label"),
    startLine,
    endLine,
    text,
    source: "terminal" as const,
  };
  clipboard.setData("text/plain", text);
  clipboard.setData("text/html", composerClipboardHtml([chip], text));
}

async function copySelection(id: string, html = false) {
  const term = hosts.get(id)?.term;
  const selection = term ? readSelection(term) : null;
  if (!selection) return;
  const payload = html
    ? `<pre style="font-family:Consolas,monospace">${escapeHtml(selection.text)}</pre>`
    : selection.text;
  try {
    if (html) {
      await navigator.clipboard.write([
        new ClipboardItem({
          "text/plain": new Blob([selection.text], { type: "text/plain" }),
          "text/html": new Blob([payload], { type: "text/html" }),
        }),
      ]);
    } else {
      await navigator.clipboard.writeText(selection.text);
    }
  } catch {
    /* 浏览器可能不允许剪贴板 */
  }
}

function escapeHtml(value: string) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

async function paste(id: string, supplied?: string) {
  const session = props.sessions.find((item) => item.id === id);
  if (!session?.live) return;
  let text = supplied ?? "";
  if (!text) {
    try {
      text = await backend.terminalClipboardText();
    } catch {
      text = await navigator.clipboard.readText().catch(() => "");
    }
  }
  if (!text) return;
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  if (lines.length > 1) {
    pasteConfirm.value = { id, text, lines: lines.length };
    return;
  }
  hosts.get(id)?.term.paste(text);
}

function confirmPaste() {
  const pending = pasteConfirm.value;
  pasteConfirm.value = null;
  if (!pending) return;
  hosts.get(pending.id)?.term.paste(pending.text);
}

function confirmAddChat() {
  const payload = addChat.value;
  addChat.value = null;
  if (!payload?.text.trim()) return;
  emit("addToChat", { text: payload.text, startLine: payload.startLine, endLine: payload.endLine });
  hosts.get(props.activeId)?.term.clearSelection();
}

function toggleSizeToContent(id: string) {
  const host = hosts.get(id);
  const element = hostEl(id);
  if (!host || !element) return;
  if (sized.has(id)) {
    sized.delete(id);
    host.fit.fit();
    showOverlay(`${host.term.cols} × ${host.term.rows}`);
    return;
  }
  let max = 80;
  const buffer = host.term.buffer.active;
  for (let index = 0; index < buffer.length; index += 1) {
    const line = buffer.getLine(index);
    if (!line) continue;
    max = Math.max(max, line.translateToString(true).length);
  }
  host.term.resize(Math.min(500, max + 2), host.term.rows);
  sized.add(id);
  showOverlay(`${host.term.cols} × ${host.term.rows}`);
}

function updateSticky(id: string) {
  if (id !== props.activeId) return;
  const host = hosts.get(id);
  if (!host) {
    sticky.value = "";
    return;
  }
  const command = host.integration.commandAtLine(host.term.buffer.active.viewportY);
  sticky.value = command?.command ?? "";
}

function paneWeight(id: string) {
  return paneWeights.value[id] ?? 1;
}

function beginPaneResize(leftId: string, rightId: string, event: PointerEvent) {
  event.preventDefault();
  const startX = event.clientX;
  const startLeft = paneWeight(leftId);
  const startRight = paneWeight(rightId);
  const width = body.value?.clientWidth ?? 1;
  const move = (moveEvent: PointerEvent) => {
    const delta = (moveEvent.clientX - startX) / Math.max(width, 1) * (startLeft + startRight);
    paneWeights.value = {
      ...paneWeights.value,
      [leftId]: Math.max(0.2, startLeft + delta),
      [rightId]: Math.max(0.2, startRight - delta),
    };
    fitVisible(false);
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

function fitVisible(shouldFocus = true) {
  for (const session of visibleSessions.value) {
    const active = hosts.get(session.id);
    const element = hostEl(session.id);
    if (!active || !element?.clientWidth || !element.clientHeight) continue;
    if (sized.has(session.id)) continue;
    active.fit.fit();
    if (session.live) void backend.terminalResize(session.id, active.term.cols, active.term.rows);
    if (shouldFocus && session.id === props.activeId) active.term.focus();
    if (session.id === props.activeId) showOverlay(`${active.term.cols} × ${active.term.rows}`);
  }
}

async function syncSessions() {
  const ids = new Set(props.sessions.map((item) => item.id));
  for (const id of hosts.keys()) {
    if (!ids.has(id)) unmount(id);
  }
  for (const session of props.sessions) {
    if (session.pending || hosts.has(session.id)) continue;
    await mount(session);
  }
}

async function applyQuickFix(item: TerminalQuickFix) {
  const id = quickFix.value?.id ?? props.activeId;
  quickFix.value = null;
  if (item.kind === "openUrl") {
    emit("openUrl", item.url);
    return;
  }
  if (item.kind === "freePort") {
    try {
      await backend.terminalFreePort(item.port);
    } catch (error) {
      hosts.get(id)?.term.write(`\r\n\x1b[31m${error instanceof Error ? error.message : t("terminal.freePortFailed")}\x1b[0m\r\n`);
    }
    return;
  }
  const session = props.sessions.find((entry) => entry.id === id);
  if (session?.live) void backend.terminalWrite(id, `${item.command}\n`);
}

function writeText(id: string, text: string) {
  const session = props.sessions.find((item) => item.id === id);
  if (!session?.live) return;
  void backend.terminalWrite(id, text);
}

watch(() => props.sessions.map((item) => item.id).join("\n"), () => {
  void syncSessions();
});

watch(() => [props.activeId, props.height, visibleSessions.value.map((item) => item.id).join()], async () => {
  await nextTick();
  fitVisible();
  addChat.value = null;
  updateSticky(props.activeId);
  if (props.activeId) schedulePublish(props.activeId);
});

watch(() => props.open, (value) => {
  if (value) void nextTick(() => fitVisible(true));
});

function onWindowResize() {
  fitVisible(false);
}

onMounted(() => {
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  window.addEventListener("resize", onWindowResize);
  window.addEventListener("pointerup", onWindowPointerUp);
  tabsEl.value?.addEventListener("wheel", onTabsWheel, { passive: false });
  void syncSessions();
});

onActivated(() => {
  void nextTick(() => fitVisible(false));
});

onBeforeUnmount(() => {
  themeObserver.disconnect();
  window.removeEventListener("resize", onWindowResize);
  window.removeEventListener("pointerup", onWindowPointerUp);
  for (const timer of publishTimers.values()) window.clearTimeout(timer);
  publishTimers.clear();
  tabsEl.value?.removeEventListener("wheel", onTabsWheel);
  window.removeEventListener("pointermove", onTabPointerMove);
  window.removeEventListener("pointerup", onTabPointerUp);
  window.removeEventListener("pointercancel", onTabPointerUp);
  clearTabDragVisual(tabDrag?.el);
  tabDrag = null;
  document.body.classList.remove("is-dragging-terminal-tab");
  window.clearTimeout(overlayTimer);
  window.clearTimeout(bellTimer);
  for (const id of [...hosts.keys()]) unmount(id);
});

defineExpose({
  focus: () => fitVisible(true),
  write: writeText,
  clear: () => hosts.get(props.activeId)?.term.clear(),
  search: () => hosts.get(props.activeId)?.search,
});
</script>

<template>
  <section
    class="bottom-panel"
    :class="{ 'is-bell': bell }"
    data-workspace-drop="terminal"
    :style="{ height: `${height}px` }"
  >
    <div
      class="bottom-panel-resize"
      role="separator"
      aria-orientation="horizontal"
      :aria-label="t('terminal.resizeHeight')"
      @pointerdown="onResize"
    />
    <header class="bottom-panel-bar">
      <div
        ref="tabsEl"
        class="bottom-panel-tabs vs-scroll"
        @wheel.ctrl.prevent
        @dblclick="onTabsDblClick"
        @contextmenu="openEmptyMenu"
      >
        <button
          v-for="session in sessions"
          :key="session.id"
          type="button"
          class="bottom-panel-tab"
          :class="{
            active: session.id === activeId || visibleSessions.some((item) => item.id === session.id),
            busy: session.busy,
            error: session.exitCode,
            [`color-${session.color}`]: session.color,
          }"
          :data-terminal-tab="session.id"
          :title="tabTooltip(session)"
          @pointerdown="onTabPointerDown(session.id, $event)"
          @click="onTabClick(session.id)"
          @dblclick.stop="onTabDblClick(session.id)"
          @contextmenu="openTabMenu(session.id, $event)"
        >
          <Codicon :name="session.icon || 'terminal'" :size="14" :spin="session.busy" />
          <span v-if="renameId !== session.id">{{ tabLabel(session) }}</span>
          <input
            v-else
            ref="renameInput"
            v-model="renameValue"
            class="terminal-rename"
            @click.stop
            @keydown.enter.prevent="commitRename"
            @keydown.escape.stop="renameId = ''"
            @blur="commitRename"
          />
          <i :title="t('terminal.kill')" @click.stop="emit('kill', session.id)">
            <Codicon name="close" :size="12" />
          </i>
        </button>
        <span v-if="!sessions.length" class="bottom-panel-empty">{{ t('terminal.label') }}</span>
      </div>
      <div class="bottom-panel-actions">
        <button type="button" :title="newTitle" @click="onNewButton">
          <Codicon name="add" :size="16" />
        </button>
        <button type="button" :title="t('terminal.newMenu')" @click="openNewMenu">
          <Codicon name="chevron-down" :size="12" />
        </button>
        <button type="button" :title="t('terminal.splitTerminal')" :disabled="!activeId" @click="emit('split')">
          <Codicon name="split-horizontal" :size="16" />
        </button>
        <button type="button" :title="t('terminal.clearTerminal')" :disabled="!activeId" @click="hosts.get(activeId)?.term.clear()">
          <Codicon name="clear-all" :size="16" />
        </button>
        <button type="button" :title="t('terminal.findInTerminal')" :disabled="!activeId" @click="openFind">
          <Codicon name="search" :size="16" />
        </button>
        <button type="button" :title="t('terminal.kill')" :disabled="!activeId" @click="emit('kill', activeId)">
          <Codicon name="trash" :size="16" />
        </button>
        <button type="button" :title="t('terminal.hidePanel')" @click="emit('close')">
          <Codicon name="chevron-down" :size="16" />
        </button>
      </div>
    </header>
    <div ref="body" class="terminal-body" @pointerdown="onTerminalPointerDown" @wheel="onTerminalWheel">
      <TerminalFind
        v-if="findOpen && findSearch"
        ref="findRef"
        :search="findSearch"
        :initial="findInitial"
        @close="findOpen = false; findSearch = null"
      />
      <div v-if="sticky" class="terminal-sticky">{{ sticky }}</div>
      <div v-if="overlay" class="terminal-size-overlay">{{ overlay }}</div>
      <div v-if="linkHint" class="terminal-link-hint" :style="{ left: `${linkHint.x}px`, top: `${linkHint.y}px` }">
        {{ mac ? t('terminal.openLinkMac') : t('terminal.openLinkWin') }}
      </div>
      <ul
        v-if="suggestions.length"
        class="terminal-suggest"
        role="listbox"
        :aria-label="t('terminal.completion')"
        :style="{ left: `${suggestPlace.x}px`, top: `${suggestPlace.y}px` }"
      >
        <li
          v-for="(item, index) in suggestions"
          :key="`${item.kind}:${item.name}`"
          :class="{ active: index === suggestIndex }"
          role="option"
          @mousedown.prevent="suggestIndex = index; acceptSuggestion(activeId)"
        >
          <Codicon :name="item.kind === 'folder' ? 'folder' : item.kind === 'file' ? 'file' : 'terminal'" :size="14" />
          {{ item.name }}
        </li>
      </ul>
      <button
        v-if="addChat"
        type="button"
        class="terminal-add-chat"
        :style="{ left: `${addChat.x}px`, top: `${addChat.y}px` }"
        @pointerdown.stop.prevent
        @click="confirmAddChat"
      >
        {{ t('workbench.attachChat') }}
      </button>
      <div class="terminal-panes">
        <template v-for="(session, index) in sessions" :key="session.id">
          <div
            v-show="visibleSessions.some((item) => item.id === session.id)"
            class="terminal-pane"
            :class="{ active: session.id === activeId }"
            :style="{ flex: paneWeight(session.id) }"
            @mousedown="emit('select', session.id)"
            @contextmenu="onTerminalContextMenu(session.id, $event)"
          >
            <div class="terminal-host" :data-terminal-id="session.id" />
          </div>
          <div
            v-if="index < sessions.length - 1 && visibleSessions.some((item) => item.id === session.id) && visibleSessions.some((item) => item.id === sessions[index + 1]?.id)"
            class="terminal-split-resize"
            @pointerdown.stop="beginPaneResize(session.id, sessions[index + 1]!.id, $event)"
          />
        </template>
      </div>
      <p v-if="!sessions.length" class="terminal-placeholder">{{ t('terminal.noOpen') }}</p>
    </div>
    <div v-if="palette" class="terminal-palette">
      <input
        ref="paletteInput"
        v-model="palette.query"
        :placeholder="palette.kind === 'command' ? t('terminal.recentCommands') : t('terminal.recentDirectories')"
        @keydown.escape.stop="palette = null"
        @keydown.enter.prevent="confirmPalette"
        @keydown.down.prevent="palette.index = Math.min(palette.index + 1, Math.max(0, paletteItems.length - 1))"
        @keydown.up.prevent="palette.index = Math.max(0, palette.index - 1)"
      />
      <button
        v-for="(item, index) in paletteItems"
        :key="item"
        type="button"
        :class="{ active: index === palette.index }"
        @mouseenter="palette.index = index"
        @click="confirmPalette"
      >{{ item }}</button>
      <p v-if="!paletteItems.length">{{ t('terminal.noHistory') }}</p>
    </div>
    <div v-if="pasteConfirm" class="terminal-paste-confirm">
      <p>{{ t('terminal.pasteConfirm', { lines: pasteConfirm.lines }) }}</p>
      <button type="button" @click="confirmPaste">{{ t('workbench.paste') }}</button>
      <button type="button" @click="pasteConfirm = null">{{ t('common.cancel') }}</button>
    </div>
    <ContextMenu
      v-if="tabMenu"
      :x="tabMenu.x"
      :y="tabMenu.y"
      :items="tabMenuItems"
      @select="onTabMenu"
      @close="tabMenu = null"
    />
    <ContextMenu
      v-if="emptyMenu"
      :x="emptyMenu.x"
      :y="emptyMenu.y"
      :items="emptyMenuItems"
      @select="onEmptyMenu"
      @close="emptyMenu = null"
    />
    <ContextMenu
      v-if="instanceMenu"
      :x="instanceMenu.x"
      :y="instanceMenu.y"
      :items="instanceMenuItems"
      @select="onInstanceMenu"
      @close="instanceMenu = null"
    />
    <ContextMenu
      v-if="newMenu"
      :x="newMenu.x"
      :y="newMenu.y"
      :items="newMenuItems"
      @select="onNewMenu"
      @close="newMenu = null"
    />
    <ContextMenu
      v-if="iconPicker"
      :x="iconPicker.x"
      :y="iconPicker.y"
      :items="TAB_ICONS.map((name) => ({ id: name, label: name, icon: name }))"
      @select="(name) => { emit('patch', iconPicker!.id, { icon: name }); iconPicker = null; }"
      @close="iconPicker = null"
    />
    <ContextMenu
      v-if="colorPicker"
      :x="colorPicker.x"
      :y="colorPicker.y"
      :items="TAB_COLORS.map((name) => ({ id: name || 'none', label: name || t('terminal.colorDefault') }))"
      @select="(name) => { emit('patch', colorPicker!.id, { color: name === 'none' ? undefined : name }); colorPicker = null; }"
      @close="colorPicker = null"
    />
    <ContextMenu
      v-if="quickFix"
      :x="quickFix.x"
      :y="quickFix.y"
      :items="quickFix.items.map((item, index) => ({ id: String(index), label: item.label }))"
      @select="(id) => applyQuickFix(quickFix!.items[Number(id)]!)"
      @close="quickFix = null"
    />
  </section>
</template>
