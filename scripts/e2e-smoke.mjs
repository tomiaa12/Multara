/**
 * 真实 Tauri 冒烟：要求本机已运行 `pnpm tauri:dev`（Vite :1420 + WebView bridge）。
 * 对测 docs/测试用例.md §19；不代启桌面进程。
 */
// Vite 在 Windows 上常只监听 [::1]；用 localhost 才能连上，不要写死 127.0.0.1
const BASE = process.env.MULTARA_URL || "http://localhost:1420";

const NAV_ROUTES = [
  { hash: "#/projects", id: "projects" },
  { hash: "#/inbox", id: "inbox" },
  { hash: "#/tasks", id: "tasks" },
  { hash: "#/automation", id: "automation" },
  { hash: "#/agents", id: "agents" },
  { hash: "#/squads", id: "squads" },
  { hash: "#/stats", id: "stats" },
  { hash: "#/runtimes", id: "runtimes" },
  { hash: "#/skills", id: "skills" },
  { hash: "#/mcp", id: "mcp" },
  { hash: "#/channels", id: "channels" },
  { hash: "#/settings", id: "settings" },
];

/** @typedef {{ status: "PASS" | "FAIL" | "SKIP"; id: string; detail?: string }} CaseResult */

/** @type {CaseResult[]} */
const results = [];

function log(line) {
  console.log(line);
}

async function probe() {
  try {
    const response = await fetch(BASE, { method: "GET" });
    return response.ok;
  } catch {
    return false;
  }
}

/**
 * @param {Record<string, unknown>} payload
 * @returns {Promise<unknown>}
 */
async function e2e(payload) {
  const response = await fetch(`${BASE}/__e2e/run`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  const body = await response.json().catch(() => ({}));
  if (response.status === 504) {
    throw new Error(body?.error || "e2e command timed out");
  }
  if (!response.ok) {
    throw new Error(body?.error || `HTTP ${response.status}`);
  }
  if (body?.ok === false) {
    throw new Error(body?.error || "e2e failed");
  }
  return body?.result;
}

/**
 * @param {string} id
 * @param {() => Promise<string | void>} fn
 */
async function runCase(id, fn) {
  try {
    const detail = await fn();
    results.push({ status: "PASS", id, detail: detail || undefined });
    log(`PASS  ${id}${detail ? ` — ${detail}` : ""}`);
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    if (message.startsWith("SKIP:")) {
      const detail = message.slice(5).trim();
      results.push({ status: "SKIP", id, detail });
      log(`SKIP  ${id} — ${detail}`);
      return;
    }
    results.push({ status: "FAIL", id, detail: message });
    log(`FAIL  ${id} — ${message}`);
  }
}

function skip(reason) {
  throw new Error(`SKIP: ${reason}`);
}

async function dismissLeavePrompt() {
  const clicked = await e2e({
    action: "eval",
    js: `(() => {
      const btn = [...document.querySelectorAll("button")].find((el) => {
        const text = (el.textContent || "").replace(/\\s+/g, " ").trim();
        return text === "不保存" || text === "Don't Save" || text === "Discard";
      });
      if (!btn) return null;
      btn.click();
      return (btn.textContent || "").trim();
    })()`,
  });
  if (clicked?.value) await new Promise((r) => setTimeout(r, 300));
}

async function openHash(hash) {
  await dismissLeavePrompt();
  await e2e({ action: "open", hash });
  await dismissLeavePrompt();
  // 设置页是 .settings-shell（无 h1/.page）；收件箱是 .inbox-page
  await e2e({
    action: "waitFor",
    selector: "h1, .page, .workbench, .settings-shell, .inbox-page, .error-card",
    timeoutMs: 20_000,
  });
}

async function main() {
  log(`Multara E2E smoke → ${BASE}`);
  if (!(await probe())) {
    console.error(`无法连接 ${BASE}。请先在本机运行：pnpm tauri:dev`);
    process.exit(1);
  }

  await runCase("TC-BOOT-02", async () => {
    await e2e({ action: "assertInTauri" });
    await openHash("#/projects");
    const snap = await e2e({ action: "snapshot" });
    if (!snap?.inTauri) throw new Error("snapshot.inTauri 为 false");
    if (!String(snap?.href ?? "").includes("/projects")) throw new Error(`路由不对：${snap?.href}`);
    return snap.title || "projects";
  });

  await runCase("TC-NAV-01", async () => {
    const opened = [];
    for (const route of NAV_ROUTES) {
      await openHash(route.hash);
      const snap = await e2e({ action: "snapshot" });
      const path = String(snap?.href ?? "");
      if (!path.includes(route.hash.slice(1)) && !path.includes(route.id)) {
        throw new Error(`${route.hash} 打开后 href=${path}`);
      }
      const blank = await e2e({
        action: "eval",
        js: `Boolean(document.querySelector('.error-card')) ? document.querySelector('.error-card').textContent : ''`,
      });
      if (blank?.value) throw new Error(`${route.hash} 出现错误：${String(blank.value).trim()}`);
      opened.push(route.id);
    }
    return opened.join(", ");
  });

  await runCase("TC-RT-01", async () => {
    await openHash("#/runtimes");
    await e2e({ action: "waitFor", selector: "h1", timeoutMs: 15_000 });
    const labels = ["重新检测", "重新偵測", "Detect again"];
    let ok = false;
    for (const text of labels) {
      try {
        await e2e({ action: "waitFor", text, timeoutMs: 3_000 });
        ok = true;
        break;
      } catch {
        // try next locale label
      }
    }
    if (!ok) throw new Error("运行时页未找到重新检测按钮");
    return "runtimes page";
  });

  await runCase("TC-AG-01", async () => {
    await openHash("#/agents");
    await e2e({ action: "waitFor", selector: "h1", timeoutMs: 15_000 });
    const found = await e2e({ action: "waitFor", text: "Miku", timeoutMs: 10_000 }).catch(() => null);
    if (!found) {
      const count = await e2e({
        action: "eval",
        js: `document.querySelectorAll('.list-card, .collection-card, article').length`,
      });
      if (!Number(count?.value)) throw new Error("智能体列表为空且未找到 Miku");
      return `agents=${count.value}`;
    }
    return "Miku";
  });

  await runCase("TC-TK-01", async () => {
    await openHash("#/tasks");
    await e2e({ action: "waitFor", selector: ".task-column", timeoutMs: 15_000 });
    const snap = await e2e({ action: "snapshot" });
    if (!snap?.taskColumns) throw new Error("没有看板列");
    return `columns=${snap.taskColumns}`;
  });

  await runCase("TC-IN-01", async () => {
    await openHash("#/inbox");
    await e2e({ action: "waitFor", selector: "h1, .inbox-page", timeoutMs: 15_000 });
    return "inbox";
  });

  await runCase("TC-SET-01/02", async () => {
    await openHash("#/settings");
    await e2e({ action: "waitFor", selector: ".settings-shell, .settings-body", timeoutMs: 15_000 });
    // 串行探测，避免并行 waitFor 挤满队列；限定在设置页节点上，避免命中标题栏主题按钮
    const themeOk = await e2e({
      action: "waitFor",
      selector: ".settings-shell strong, .settings-row, .settings-heading",
      text: "主题",
      timeoutMs: 5_000,
    }).then(() => true).catch(() =>
      e2e({
        action: "waitFor",
        selector: ".settings-shell strong, .settings-row, .settings-heading",
        text: "Theme",
        timeoutMs: 5_000,
      }).then(() => true).catch(() => false),
    );
    const langOk = await e2e({
      action: "waitFor",
      selector: ".settings-shell strong, .settings-row, .settings-heading",
      text: "语言",
      timeoutMs: 5_000,
    }).then(() => true).catch(() =>
      e2e({
        action: "waitFor",
        selector: ".settings-shell strong, .settings-row, .settings-heading",
        text: "Language",
        timeoutMs: 5_000,
      }).then(() => true).catch(() => false),
    );
    if (!themeOk || !langOk) throw new Error(`设置页缺少控件 theme=${themeOk} language=${langOk}`);
    return "settings theme/language";
  });

  await runCase("TC-WB-01/11/20", async () => {
    await openHash("#/projects");
    const snap = await e2e({ action: "snapshot" });
    if (!snap?.projectRows) skip("没有需求/工作区，跳过工作台文件编辑");
    await e2e({ action: "click", selector: ".req-table-row" });
    await e2e({ action: "waitFor", selector: ".workbench, .workspace-file-tree, .tree-entry", timeoutMs: 20_000 });
    // 避免把 `.github/workflows` 这类目录当成文件（旧正则会命中 `.github`）
    const fileLabel = await e2e({
      action: "eval",
      js: `([...document.querySelectorAll('.tree-entry')]
        .map((el) => (el.textContent || "").replace(/\\s+/g, " ").trim())
        .map((t) => t.split(" ·")[0].trim().split("/").pop() || "")
        .find((name) => /^(package\\.json|README\\.md|README-zh\\.md|index\\.html|vite\\.config\\.ts|tsconfig\\.json)$/i.test(name)
          || (/^[^.\\/][^\\/]*\\.[A-Za-z0-9]{1,10}$/.test(name) && !name.startsWith("."))) || "")`,
    });
    const shortName = String(fileLabel?.value ?? "").trim();
    if (!shortName) skip("工作区文件树里没有可打开的文件");
    await e2e({ action: "click", selector: ".tree-entry", text: shortName });
    await e2e({ action: "waitFor", selector: ".editor-tab, .monaco-host, .editor-fallback", timeoutMs: 20_000 });
    // 等 Monaco/内容就绪
    const readyAt = Date.now();
    let before = await e2e({ action: "snapshot" });
    while (Date.now() - readyAt < 10_000 && !String(before.editorValue ?? "").length && !before.tabs?.length) {
      await new Promise((r) => setTimeout(r, 200));
      before = await e2e({ action: "snapshot" });
    }
    if (!before.tabs?.length && !String(before.editorValue ?? "")) {
      throw new Error(`点击 ${shortName} 后编辑器未打开`);
    }
    const original = before.editorValue || "";
    const marker = `\n/* multara-e2e-smoke ${Date.now()} */\n`;
    await e2e({ action: "setEditor", value: `${original}${marker}` });
    await e2e({ action: "save" });
    const after = await e2e({ action: "snapshot" });
    if (!String(after.editorValue ?? "").includes("multara-e2e-smoke")) {
      throw new Error("保存后编辑器内容未包含冒烟标记");
    }
    // 还原，避免污染用户仓库
    await e2e({ action: "setEditor", value: original });
    await e2e({ action: "save" });
    return shortName;
  });

  await runCase("TC-CH-01", async () => {
    const snap = await e2e({ action: "snapshot" });
    if (!String(snap?.href ?? "").includes("/projects/")) {
      await openHash("#/projects");
      const projects = await e2e({ action: "snapshot" });
      if (!projects?.projectRows) skip("没有需求工作台，仅测浮动聊天输入");
      await e2e({ action: "click", selector: ".req-table-row" });
      await e2e({ action: "waitFor", selector: ".composer-field, .workbench", timeoutMs: 20_000 });
    }
    const has = await e2e({ action: "snapshot" });
    if (!has?.hasComposer) {
      await openHash("#/tasks");
      await e2e({ action: "waitFor", selector: ".composer-field", timeoutMs: 15_000 }).catch(() => {
        skip("找不到聊天输入框（工作台/浮动）");
      });
    }
    await e2e({ action: "typeComposer", value: "multara e2e smoke — do not send" });
    return "composer typed (send skipped — needs Agent CLI)";
  });

  await runCase("TC-NAV-03", async () => {
    await openHash("#/tasks");
    const onTasks = await e2e({
      action: "eval",
      js: `(() => {
        const host = document.querySelector(".floating-chat-host");
        const fab = document.querySelector(".chat-fab");
        const hostShown = Boolean(host && getComputedStyle(host).display !== "none");
        const fabShown = Boolean(fab && fab.getClientRects().length);
        return { hostShown, fabShown };
      })()`,
    });
    if (!onTasks?.value?.hostShown || !onTasks?.value?.fabShown) {
      throw new Error(`非工作台应显示浮动气泡 host=${onTasks?.value?.hostShown} fab=${onTasks?.value?.fabShown}`);
    }
    await openHash("#/projects");
    const projects = await e2e({ action: "snapshot" });
    if (!projects?.projectRows) skip("没有需求，无法验证工作台隐藏浮动窗");
    await e2e({ action: "click", selector: ".req-table-row" });
    await e2e({ action: "waitFor", selector: ".workbench", timeoutMs: 20_000 });
    const onWb = await e2e({
      action: "eval",
      js: `(() => {
        const host = document.querySelector(".floating-chat-host");
        return { hostShown: Boolean(host && getComputedStyle(host).display !== "none") };
      })()`,
    });
    if (onWb?.value?.hostShown) throw new Error("工作台仍显示浮动聊天气泡");
    return "fab on /tasks, hidden on workbench";
  });

  await runCase("TC-CH-02", async () => {
    await openHash("#/tasks");
    await e2e({ action: "waitFor", selector: ".chat-fab", timeoutMs: 10_000 });
    await e2e({ action: "click", selector: ".chat-fab" });
    await e2e({ action: "waitFor", selector: ".floating-chat.open .composer-field, .floating-chat.open", timeoutMs: 10_000 });
    await e2e({ action: "typeComposer", value: "floating chat smoke — do not send" });
    // 点最小化收起，避免挡后续用例
    await e2e({
      action: "eval",
      js: `(() => {
        const btn = [...document.querySelectorAll(".floating-chat button")].find((el) => {
          const tip = [el.getAttribute("tip"), el.getAttribute("title"), el.getAttribute("aria-label"), el.textContent]
            .filter(Boolean).join(" ");
          return /最小化|Minimize|minimize/i.test(tip);
        });
        btn?.click();
        return Boolean(btn);
      })()`,
    });
    return "opened fab + typed (send skipped)";
  });

  await runCase("TC-AG-02", async () => {
    await openHash("#/agents");
    await e2e({ action: "click", text: "新建智能体" }).catch(() => e2e({ action: "click", text: "New agent" }));
    await e2e({ action: "waitFor", selector: ".modal, .domain-modal", timeoutMs: 10_000 });
    await e2e({
      action: "waitFor",
      selector: ".modal h2, .domain-modal h2",
      text: "新建",
      timeoutMs: 5_000,
    }).catch(() =>
      e2e({ action: "waitFor", selector: ".modal h2, .domain-modal h2", text: "New", timeoutMs: 5_000 }),
    );
    // 只验证表单可开，关闭以免写入测试智能体
    await e2e({
      action: "eval",
      js: `(() => {
        const close = document.querySelector(".modal .icon-button, .domain-modal .icon-button");
        close?.click();
        return Boolean(close);
      })()`,
    });
    return "new-agent modal opened then closed";
  });

  await runCase("TC-TK-02", async () => {
    await openHash("#/tasks");
    await e2e({ action: "click", text: "新建任务" }).catch(() => e2e({ action: "click", text: "New task" }));
    await e2e({ action: "waitFor", selector: ".task-create-dialog", timeoutMs: 10_000 });
    await e2e({
      action: "eval",
      js: `(() => {
        const close = document.querySelector(".task-create-dialog button[aria-label], .task-create-dialog .icon-button, .modal-backdrop:not(.ds-motion-ghost)");
        const x = [...document.querySelectorAll(".task-create-dialog button, .modal button")].find((el) => {
          const t = (el.textContent || "").replace(/\\s+/g, " ").trim();
          return t === "取消" || t === "Cancel" || el.classList.contains("icon-button");
        });
        (x || close)?.click();
        return true;
      })()`,
    });
    return "create-task dialog opened then closed";
  });

  await runCase("TC-WB-30", async () => {
    await openHash("#/projects");
    const snap = await e2e({ action: "snapshot" });
    if (!snap?.projectRows) skip("没有需求/工作区");
    await e2e({ action: "click", selector: ".req-table-row" });
    await e2e({ action: "waitFor", selector: ".workbench", timeoutMs: 20_000 });
    await e2e({ action: "click", selector: ".sidebar-tab, button", text: "Git" });
    await e2e({
      action: "waitFor",
      selector: ".source-control-panel, .scm-changes-pane",
      timeoutMs: 10_000,
    });
    return "git/scm panel visible";
  });

  await runCase("TC-PJ-01/03", async () => skip("新建需求与 worktree 需手工选目录"));
  await runCase("TC-WB-23", async () => skip("Diff 跳转需有智能体改动场景"));
  await runCase("TC-TK-05", async () => skip("任务执行需 Agent CLI"));
  await runCase("TC-SQ-02", async () => skip("小队执行顺序需 Agent CLI"));
  await runCase("TC-CH-06", async () => skip("停止运行需有进行中的 CLI run"));
  await runCase("TC-WB-22", async () => skip("dirty 与智能体改盘对抗需 CLI"));

  const passed = results.filter((item) => item.status === "PASS").length;
  const failed = results.filter((item) => item.status === "FAIL").length;
  const skipped = results.filter((item) => item.status === "SKIP").length;
  log("");
  log(`合计 PASS=${passed} FAIL=${failed} SKIP=${skipped}`);
  if (failed > 0) process.exit(1);
}

main().catch((cause) => {
  console.error(cause instanceof Error ? cause.message : cause);
  process.exit(1);
});
