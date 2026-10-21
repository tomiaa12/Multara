import { monaco } from "../monaco";

function sleep(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function visible(el: Element) {
  const node = el as HTMLElement;
  return Boolean(node.offsetParent || node.getClientRects().length);
}

function labelOf(el: Element) {
  const node = el as HTMLElement;
  const text = (node.textContent || "").replace(/\s+/g, " ").trim();
  const title = (node.getAttribute("title") || "").replace(/\s+/g, " ").trim();
  return text || title;
}

function labelsOf(el: Element) {
  const node = el as HTMLElement;
  const text = (node.textContent || "").replace(/\s+/g, " ").trim();
  const title = (node.getAttribute("title") || "").replace(/\s+/g, " ").trim();
  return [...new Set([text, title].filter(Boolean))];
}

function fileNameOfLabel(label: string) {
  return label.split(" ·")[0].trim().split("/").pop() ?? label;
}

function matchesClickText(label: string, text: string) {
  if (label === text) return true;
  if (fileNameOfLabel(label) === text) return true;
  if (text.includes(".")) return false;
  if (label.includes(`不${text}`)) return false;
  if (text.length <= 2) return false;
  return label.includes(text);
}

function clickTarget(selector?: string, text?: string) {
  const scope = selector
    ? [...document.querySelectorAll(selector)]
    : [
        ...document.querySelectorAll(
          "button, [role='button'], a, .tree-entry, .editor-tab, .chat-session-tab",
        ),
      ];
  const nodes = scope.filter(visible);
  if (text) {
    const exact = nodes.find((el) => labelsOf(el).includes(text));
    const named = nodes.find((el) => labelsOf(el).some((label) => fileNameOfLabel(label) === text));
    const partial = nodes.find((el) => labelsOf(el).some((label) => matchesClickText(label, text)));
    const el = exact ?? named ?? partial;
    if (!el) throw new Error(`找不到「${text}」${selector ? ` @ ${selector}` : ""}`);
    (el as HTMLElement).scrollIntoView({ block: "nearest" });
    (el as HTMLElement).click();
    return labelOf(el);
  }
  const el = nodes[0];
  if (!el) throw new Error(`找不到 ${selector ?? "可点击元素"}`);
  (el as HTMLElement).click();
  return labelOf(el);
}

function saveButton() {
  return [...document.querySelectorAll("button")].find((el) =>
    labelOf(el).includes("保存"),
  ) as HTMLButtonElement | undefined;
}

function editor() {
  const host = document.querySelector(".monaco-host");
  const editors = monaco.editor.getEditors();
  return (
    editors.find((item) => host?.contains(item.getDomNode())) ??
    editors.at(-1) ??
    editors[0] ??
    null
  );
}

function snapshot() {
  const treeLoading = [...document.querySelectorAll(".tree-empty, .tree-entry.loading")]
    .map((el) => el.textContent?.trim())
    .filter(Boolean);
  const tabs = [...document.querySelectorAll(".editor-tab")].map((el) => ({
    text: labelOf(el),
    preview: el.classList.contains("is-preview"),
    active: el.classList.contains("active"),
  }));
  const sessionTabs = [...document.querySelectorAll(".chat-session-tab")].map((el) => ({
    text: labelOf(el),
    active: el.classList.contains("active"),
  }));
  return {
    href: location.href,
    treeLoading,
    tabs,
    sessionTabs,
    editorValue: editor()?.getValue() ?? document.querySelector<HTMLTextAreaElement>(".editor-fallback")?.value ?? "",
  };
}

async function execute(payload: Record<string, unknown>) {
  const action = String(payload.action ?? "");
  if (action === "snapshot") return snapshot();
  if (action === "click") return { clicked: clickTarget(payload.selector as string | undefined, payload.text as string | undefined) };
  if (action === "clickNth") {
    const selector = String(payload.selector ?? "");
    const index = Number(payload.index ?? 0);
    const nodes = [...document.querySelectorAll(selector)].filter(visible);
    const el = nodes.at(index);
    if (!el) throw new Error(`找不到 ${selector}[${index}]，共 ${nodes.length} 个`);
    (el as HTMLElement).click();
    return { clicked: labelOf(el), count: nodes.length };
  }
  if (action === "open") {
    location.hash = String(payload.hash ?? "");
    await sleep(400);
    return snapshot();
  }
  if (action === "setEditor") {
    const value = String(payload.value ?? "");
    const current = editor();
    if (current) {
      current.focus();
      current.setValue(value);
    } else {
      const fallback = document.querySelector<HTMLTextAreaElement>(".editor-fallback");
      if (!fallback) throw new Error("编辑器还没就绪");
      fallback.value = value;
      fallback.dispatchEvent(new Event("input", { bubbles: true }));
    }
    const started = Date.now();
    while (Date.now() - started < 2000) {
      if (editor()?.getValue() === value && saveButton() && !saveButton()?.disabled) break;
      await sleep(50);
    }
    if (editor()?.getValue() !== value) throw new Error("编辑器内容没有写进去");
    if (saveButton()?.disabled) throw new Error("保存按钮仍禁用，编辑未进入 dirty");
    return { length: value.length, dirty: true };
  }
  if (action === "replaceEditor") {
    const from = String(payload.from ?? "");
    const to = String(payload.to ?? "");
    const current = editor();
    const value = current?.getValue() ?? document.querySelector<HTMLTextAreaElement>(".editor-fallback")?.value ?? "";
    if (!value.includes(from)) throw new Error(`编辑器里没有「${from}」`);
    const next = payload.all ? value.replaceAll(from, to) : value.replace(from, to);
    if (current) {
      current.focus();
      current.setValue(next);
    } else {
      const fallback = document.querySelector<HTMLTextAreaElement>(".editor-fallback");
      if (!fallback) throw new Error("编辑器还没就绪");
      fallback.value = next;
      fallback.dispatchEvent(new Event("input", { bubbles: true }));
    }
    const started = Date.now();
    while (Date.now() - started < 2000) {
      if (editor()?.getValue() === next && saveButton() && !saveButton()?.disabled) break;
      await sleep(50);
    }
    if (saveButton()?.disabled && value !== next) throw new Error("替换后保存按钮仍禁用");
    return { value: next };
  }
  if (action === "save") {
    const current = editor();
    if (current) {
      current.focus();
      const actionId = current.getSupportedActions().find((item) => item.id === "multara-save");
      if (actionId) current.trigger("e2e", "multara-save", null);
      else clickTarget(undefined, "保存");
    } else clickTarget(undefined, "保存");
    await sleep(700);
    return snapshot();
  }
  if (action === "closeTabs") {
    for (let i = 0; i < 12; i += 1) {
      const close = document.querySelector<HTMLElement>(".editor-tab .tab-close");
      if (!close) break;
      close.style.pointerEvents = "auto";
      close.style.opacity = "1";
      close.click();
      await sleep(120);
      const discard = [...document.querySelectorAll("button")].find((el) =>
        labelOf(el).includes("不保存"),
      );
      if (discard) {
        (discard as HTMLElement).click();
        await sleep(120);
      }
    }
    return snapshot();
  }
  if (action === "key") {
    const key = String(payload.key ?? "");
    if (payload.meta && key.toLowerCase() === "z") {
      document.querySelector<HTMLElement>(".monaco-editor textarea.inputarea")?.blur();
    }
    const event = new KeyboardEvent("keydown", {
      key,
      code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
      metaKey: Boolean(payload.meta),
      ctrlKey: Boolean(payload.ctrl || payload.meta),
      shiftKey: Boolean(payload.shift),
      bubbles: true,
      cancelable: true,
      composed: true,
    });
    if (payload.meta) Object.defineProperty(event, "metaKey", { get: () => true });
    if (payload.ctrl || payload.meta) Object.defineProperty(event, "ctrlKey", { get: () => true });
    if (payload.shift) Object.defineProperty(event, "shiftKey", { get: () => true });
    window.dispatchEvent(event);
    document.dispatchEvent(event);
    editor()?.getDomNode()?.dispatchEvent(event);
    await sleep(600);
    return snapshot();
  }
  if (action === "eval") {
    // eslint-disable-next-line no-eval
    return { value: eval(String(payload.js ?? "undefined")) };
  }
  throw new Error(`未知动作 ${action}`);
}

async function loop() {
  while (true) {
    try {
      const response = await fetch("/__e2e/next", { cache: "no-store" });
      if (!response.ok) {
        await sleep(800);
        continue;
      }
      const command = (await response.json()) as { id: number; payload: Record<string, unknown> };
      if (!command?.id || command.payload?.action === "idle") {
        await sleep(200);
        continue;
      }
      let body: Record<string, unknown>;
      try {
        body = { id: command.id, ok: true, result: await execute(command.payload) };
      } catch (cause) {
        body = {
          id: command.id,
          ok: false,
          error: cause instanceof Error ? cause.message : String(cause),
        };
      }
      await fetch("/__e2e/result", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    } catch {
      await sleep(800);
    }
  }
}

void loop();
