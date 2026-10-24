/*
 * UI Playground 交互脚本（无构建依赖，直接在浏览器运行）。
 *
 * 所有调节都通过改写 <html> 上的 CSS 变量实现，和应用使用同一套 Token，
 * 因此在这里调出的效果就是应用里会看到的效果。
 */

const root = document.documentElement;

/* ── 可调参数定义 ─────────────────────────────────────────────── */
// 默认值必须和 src/design/tokens.css 保持一致
const BASE = {
  spacing: { "0-5": 2, 1: 4, "1-5": 6, 2: 8, "2-5": 10, 3: 12, 4: 16, 5: 20, 6: 24, 8: 32, 10: 40, 12: 48, 16: 64, 24: 96 },
  radius: { xs: 3, sm: 4, md: 6, lg: 8, xl: 12 },
  font: { xs: 11, sm: 12, md: 13, lg: 14, xl: 16, "2xl": 20, "3xl": 24 },
  control: { sm: 26, md: 32, lg: 36 },
};

const DEFAULTS = {
  theme: "dark",
  primary: "",
  radius: 1,
  spacing: 1,
  fontSize: 13,
  fontFamily: "inter",
  lineHeight: 1.5,
  headingWeight: 600,
  sidebarWidth: 208,
  headerHeight: 40,
  contentWidth: 1160,
  contentPadding: 32,
  gridColumns: 4,
  cardGap: 12,
  sectionGap: 32,
};

const FONT_FAMILIES = {
  inter: 'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
  system: 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif',
  geist: '"Geist", "SF Pro Text", system-ui, "PingFang SC", sans-serif',
  mono: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace',
};

// 预设方案：用于快速对比不同的 UI 取向
const PRESETS = {
  默认: {},
  紧凑: { spacing: 0.85, fontSize: 12, radius: 0.75, headerHeight: 36, sidebarWidth: 192, cardGap: 8, sectionGap: 24, contentPadding: 24 },
  宽松: { spacing: 1.2, fontSize: 14, lineHeight: 1.6, headerHeight: 44, sidebarWidth: 232, cardGap: 16, sectionGap: 40, contentPadding: 40 },
  圆润: { radius: 1.6, primary: "#7c5cff" },
  锐利: { radius: 0.4, headingWeight: 700, primary: "#2f6bff" },
  青绿: { primary: "#14a38b" },
};

const PRIMARY_SWATCHES = ["", "#2f6bff", "#7c5cff", "#14a38b", "#e5484d", "#f08c00", "#d6409f"];

let state = { ...DEFAULTS };
const slots = { A: null, B: null };

/* ── 应用状态到 CSS 变量 ──────────────────────────────────────── */
function px(value) {
  return `${Math.round(value * 100) / 100}px`;
}

function apply() {
  const s = state;
  root.dataset.theme = s.theme;
  const set = (name, value) => root.style.setProperty(name, value);
  const unset = (name) => root.style.removeProperty(name);

  for (const [key, value] of Object.entries(BASE.spacing)) set(`--spacing-${key}`, px(value * s.spacing));
  for (const [key, value] of Object.entries(BASE.radius)) set(`--radius-${key}`, px(value * s.radius));
  for (const [key, value] of Object.entries(BASE.control)) set(`--control-height-${key}`, px(value * (0.5 + s.spacing / 2)));

  // 字号整体按正文基准缩放，保持层级比例
  const ratio = s.fontSize / 13;
  for (const [key, value] of Object.entries(BASE.font)) set(`--font-size-${key}`, px(value * ratio));

  set("--font-family-sans", FONT_FAMILIES[s.fontFamily]);
  set("--line-height-normal", String(s.lineHeight));
  set("--font-weight-semibold", String(s.headingWeight));
  set("--layout-sidebar-width", px(s.sidebarWidth));
  set("--layout-header-height", px(s.headerHeight));
  set("--layout-content-max-width", px(s.contentWidth));
  set("--layout-content-padding", px(s.contentPadding));
  set("--layout-grid-columns", String(s.gridColumns));
  set("--layout-card-gap", px(s.cardGap));
  set("--layout-section-gap", px(s.sectionGap));

  const primaryVars = ["--color-primary", "--color-primary-hover", "--color-primary-active", "--color-primary-subtle", "--color-primary-border", "--color-focus", "--color-focus-ring", "--color-selection"];
  if (s.primary) {
    const mixTo = s.theme === "light" ? "black" : "white";
    set("--color-primary", s.primary);
    set("--color-primary-hover", `color-mix(in srgb, ${s.primary} 86%, ${mixTo})`);
    set("--color-primary-active", `color-mix(in srgb, ${s.primary} 86%, ${s.theme === "light" ? "white" : "black"})`);
    set("--color-primary-subtle", `color-mix(in srgb, ${s.primary} 14%, transparent)`);
    set("--color-primary-border", `color-mix(in srgb, ${s.primary} 42%, transparent)`);
    set("--color-focus", s.primary);
    set("--color-focus-ring", `color-mix(in srgb, ${s.primary} 28%, transparent)`);
    set("--color-selection", `color-mix(in srgb, ${s.primary} 24%, transparent)`);
  } else {
    primaryVars.forEach(unset);
  }

  syncControls();
  updateColorValues();
  updateExport();
  sendToPreview();
  localStorage.setItem("multara.playground", JSON.stringify(state));
}

/* ── 控件绑定 ─────────────────────────────────────────────────── */
function syncControls() {
  document.querySelectorAll("[data-bind]").forEach((input) => {
    const key = input.dataset.bind;
    if (input.type === "range" || input.tagName === "SELECT") input.value = String(state[key]);
    const output = document.querySelector(`output[for="${input.id}"]`);
    if (output) output.textContent = `${state[key]}${input.dataset.unit ?? ""}`;
  });
  document.querySelectorAll("[data-theme-set]").forEach((button) => {
    button.classList.toggle("active", button.dataset.themeSet === state.theme);
  });
  document.querySelectorAll(".pg-swatches [data-primary]").forEach((button) => {
    button.classList.toggle("active", button.dataset.primary === state.primary);
  });
}

function bindControls() {
  document.querySelectorAll("[data-bind]").forEach((input) => {
    input.addEventListener("input", () => {
      const key = input.dataset.bind;
      state[key] = input.tagName === "SELECT" ? input.value : Number(input.value);
      apply();
    });
  });
  document.querySelectorAll("[data-theme-set]").forEach((button) => {
    button.addEventListener("click", () => {
      state.theme = button.dataset.themeSet;
      apply();
    });
  });

  const swatches = document.querySelector(".pg-swatches");
  for (const color of PRIMARY_SWATCHES) {
    const button = document.createElement("button");
    button.type = "button";
    button.dataset.primary = color;
    button.title = color || "默认主色";
    // 空值代表"使用 tokens.css 中的主题默认主色"
    button.style.background = color || "var(--color-primary)";
    if (!color) button.style.backgroundImage = "linear-gradient(135deg, #f2f2f4 50%, #000000 50%)";
    button.addEventListener("click", () => {
      state.primary = color;
      apply();
    });
    swatches.append(button);
  }
  const picker = document.createElement("input");
  picker.type = "color";
  picker.title = "自定义主色";
  picker.value = "#f2f2f4";
  picker.addEventListener("input", () => {
    state.primary = picker.value;
    apply();
  });
  swatches.append(picker);

  const presets = document.querySelector(".pg-presets");
  for (const [name, patch] of Object.entries(PRESETS)) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "ui-button";
    button.dataset.size = "small";
    button.textContent = name;
    button.addEventListener("click", () => {
      state = { ...DEFAULTS, theme: state.theme, ...patch };
      apply();
    });
    presets.append(button);
  }

  document.querySelector("[data-action=reset]").addEventListener("click", () => {
    state = { ...DEFAULTS, theme: state.theme };
    apply();
  });

  // A/B 方案对比：保存两套参数，一键来回切换
  document.querySelectorAll("[data-slot-save]").forEach((button) => {
    button.addEventListener("click", () => {
      slots[button.dataset.slotSave] = { ...state };
      refreshSlots();
    });
  });
  document.querySelectorAll("[data-slot-load]").forEach((button) => {
    button.addEventListener("click", () => {
      const saved = slots[button.dataset.slotLoad];
      if (!saved) return;
      state = { ...saved };
      apply();
    });
  });
  document.querySelector("[data-action=copy]").addEventListener("click", async (event) => {
    const text = document.querySelector(".pg-export").value;
    try {
      await navigator.clipboard.writeText(text);
      event.currentTarget.textContent = "已复制";
    } catch {
      document.querySelector(".pg-export").select();
    }
    setTimeout(() => (event.currentTarget.textContent = "复制 CSS"), 1200);
  });
}

function refreshSlots() {
  for (const slot of ["A", "B"]) {
    const load = document.querySelector(`[data-slot-load="${slot}"]`);
    load.disabled = !slots[slot];
  }
  localStorage.setItem("multara.playground.slots", JSON.stringify(slots));
}

function updateExport() {
  const lines = [];
  for (const name of Array.from(root.style)) {
    if (name.startsWith("--")) lines.push(`  ${name}: ${root.style.getPropertyValue(name).trim()};`);
  }
  document.querySelector(".pg-export").value = `/* 在 tokens.css 中覆盖 */\n:root {\n${lines.join("\n")}\n}`;
}

/* ── 色板 ─────────────────────────────────────────────────────── */
const COLOR_GROUPS = {
  品牌: ["primary", "primary-hover", "primary-active", "primary-subtle", "primary-border", "on-primary"],
  背景: ["bg", "bg-secondary", "bg-tertiary", "surface", "surface-raised", "surface-hover", "surface-active", "surface-emphasis"],
  文字: ["text-strong", "text", "text-secondary", "text-tertiary", "text-disabled", "text-placeholder"],
  边框: ["border-light", "border", "border-strong", "field", "field-border", "focus"],
  状态: ["success", "success-subtle", "warning", "warning-subtle", "error", "error-subtle", "info", "info-subtle"],
  辅助: ["accent", "purple", "git-added", "git-modified", "git-deleted", "git-untracked"],
};

function renderColors() {
  const host = document.querySelector("#pg-colors");
  for (const [group, names] of Object.entries(COLOR_GROUPS)) {
    const section = document.createElement("div");
    section.className = "pg-section";
    section.innerHTML = `<span class="ds-overline">${group}</span><div class="pg-colors">${names
      .map((name) => `<div class="pg-color"><i style="background: var(--color-${name})"></i><span><strong>--color-${name}</strong><code data-color-var="--color-${name}"></code></span></div>`)
      .join("")}</div>`;
    host.append(section);
  }
}

function updateColorValues() {
  const style = getComputedStyle(root);
  document.querySelectorAll("[data-color-var]").forEach((el) => {
    el.textContent = style.getPropertyValue(el.dataset.colorVar).trim();
  });
}

/* ── 视图切换 / 交互演示 ──────────────────────────────────────── */
function bindTabs() {
  document.querySelectorAll("[data-view-tab]").forEach((tab) => {
    tab.addEventListener("click", () => {
      document.querySelectorAll("[data-view-tab]").forEach((other) => other.classList.toggle("active", other === tab));
      document.querySelectorAll(".pg-view").forEach((view) => (view.hidden = view.id !== tab.dataset.viewTab));
      localStorage.setItem("multara.playground.view", tab.dataset.viewTab);
    });
  });
  const saved = localStorage.getItem("multara.playground.view");
  document.querySelector(`[data-view-tab="${saved}"]`)?.click();
}

function bindDemos() {
  // 分段控件 / 标签页：点击切换选中态
  document.querySelectorAll(".pg-content .ds-segmented, .pg-content .ds-tabs").forEach((group) => {
    group.addEventListener("click", (event) => {
      const target = event.target.closest("button");
      if (!target || target.disabled || !group.contains(target)) return;
      group.querySelectorAll(":scope > button").forEach((button) => button.classList.toggle("active", button === target));
    });
  });
  // 加载按钮演示
  document.querySelectorAll("[data-demo=loading]").forEach((button) => {
    button.addEventListener("click", () => {
      button.classList.add("is-loading");
      setTimeout(() => button.classList.remove("is-loading"), 1400);
    });
  });
  bindTextRotateDemo();
}

/* ── 文字轮播演示：和 UiShimmerText 的 Transition 用同一组 class ── */
const ROTATE_LABELS = ["思考中…", "读取文件", "修改中", "执行命令", "输入中"];
function rotateText(host, text) {
  const previous = host.lastElementChild;
  const next = document.createElement("span");
  next.className = "ds-text-shimmer ds-text-rotate-enter-active";
  next.textContent = text;
  host.append(next);
  // 和 UiShimmerText 里的 ROTATE_DURATION 保持一致
  setTimeout(() => next.classList.remove("ds-text-rotate-enter-active"), 180);
  if (previous) {
    previous.classList.add("ds-text-rotate-leave-active");
    setTimeout(() => previous.remove(), 180);
  }
}
function bindTextRotateDemo() {
  const host = document.getElementById("pg-text-rotate");
  const button = document.getElementById("pg-text-rotate-next");
  if (!host || !button) return;
  let index = 0;
  button.addEventListener("click", () => {
    index = (index + 1) % ROTATE_LABELS.length;
    rotateText(host, ROTATE_LABELS[index]);
  });
}

/* ── 下拉框演示 ───────────────────────────────────────────────── */
const RUNTIME_OPTIONS = [
  { value: "claude", label: "Claude Code", meta: "默认" },
  { value: "codex", label: "Codex" },
  { value: "gemini", label: "Gemini CLI" },
  { value: "cursor", label: "Cursor Agent", meta: "未安装", disabled: true },
];
// pinyin 为可选全拼（按汉字空格分隔），不提供时仍可用首字母搜索
const DEPARTMENT_OPTIONS = [
  { value: "fe", label: "前端", pinyin: "qian duan" },
  { value: "be", label: "后端", pinyin: "hou duan" },
  { value: "qa", label: "测试", pinyin: "ce shi" },
  { value: "design", label: "设计", pinyin: "she ji" },
  { value: "pm", label: "产品", pinyin: "chan pin" },
  { value: "ops", label: "运维", pinyin: "yun wei" },
  { value: "data", label: "数据", pinyin: "shu ju" },
  { value: "algo", label: "算法", pinyin: "suan fa" },
];
const MEMBER_OPTIONS = [
  { value: "multara", label: "Multara 助手", description: "内置智能体", avatar: "../src/assets/logo.svg", meta: "在线" },
  { value: "zhang", label: "张三", description: "前端工程师", avatar: "🦊", meta: "在线" },
  { value: "li", label: "李四", description: "后端工程师", avatar: "🐼", meta: "忙碌" },
  { value: "wang", label: "王五", description: "测试工程师", avatar: "🐯" },
  { value: "zhao", label: "赵六", description: "产品设计师", avatar: "🐨" },
  { value: "qian", label: "钱七", description: "运维工程师", avatar: "🐧" },
  { value: "sun", label: "孙八", description: "已离职", avatar: "🐻", disabled: true },
];
const AGENT_GROUPS = [
  {
    label: "前端小队",
    options: [
      { value: "reviewer", label: "代码审查员", description: "Claude Code · 审查 PR", avatar: "🔍" },
      { value: "ui", label: "UI 还原师", description: "Codex · 设计稿转代码", avatar: "🎨" },
    ],
  },
  {
    label: "后端小队",
    options: [
      { value: "api", label: "接口设计师", description: "Claude Code · OpenAPI", avatar: "🧩" },
      { value: "db", label: "数据库专家", description: "Gemini CLI · SQL 优化", avatar: "🗄️" },
    ],
  },
  {
    label: "质量保障",
    options: [
      { value: "tester", label: "测试工程师", description: "Codex · 单测与 E2E", avatar: "🧪" },
      { value: "perf", label: "性能分析师", description: "Claude Code · 火焰图分析", avatar: "📈", disabled: true },
    ],
  },
];
const TAG_OPTIONS = [
  { value: "bug", label: "bug" },
  { value: "feature", label: "feature" },
  { value: "refactor", label: "重构", pinyin: "chong gou" },
  { value: "docs", label: "文档", pinyin: "wen dang" },
  { value: "perf", label: "性能", pinyin: "xing neng" },
];
const REPOSITORIES = ["multara/desktop", "multara/server", "multara/docs", "acme/web-app", "acme/mobile", "acme/design-tokens", "tools/cli", "tools/scripts"];
const BRANCH_OPTIONS = ["main", "develop", "feat/dropdown", "fix/login-crash", "release/1.2"].map((name) => ({ value: name, label: name }));

function bindSelectDemos() {
  const mount = (name, config) => {
    const el = document.querySelector(`[data-select-demo="${name}"]`);
    return el && DsSelect.mount(el, config);
  };
  mount("basic", { options: RUNTIME_OPTIONS, value: "claude", clearable: true, placeholder: "选择运行时" });
  mount("pinyin", { options: DEPARTMENT_OPTIONS, searchable: true, clearable: true, placeholder: "选择部门", searchPlaceholder: "搜索部门，支持拼音" });
  mount("avatar", { options: MEMBER_OPTIONS, searchable: true, value: "zhang", placeholder: "选择负责人", searchPlaceholder: "搜索成员" });
  mount("group", { groups: AGENT_GROUPS, searchable: true, clearable: true, placeholder: "选择智能体", searchPlaceholder: "搜索智能体" });
  mount("multiple", {
    options: MEMBER_OPTIONS,
    multiple: true,
    searchable: true,
    clearable: true,
    maxTagCount: 2,
    max: 5,
    value: ["zhang", "li", "wang"],
    placeholder: "选择参与成员",
  });
  mount("creatable", { options: TAG_OPTIONS, multiple: true, searchable: true, creatable: true, value: ["bug"], placeholder: "添加标签", searchPlaceholder: "搜索或创建标签" });
  mount("remote", {
    placeholder: "搜索仓库",
    searchPlaceholder: "输入仓库名，如 multara",
    clearable: true,
    // 模拟接口：600ms 后返回匹配结果
    load: (query) =>
      new Promise((resolve) =>
        setTimeout(
          () =>
            resolve(
              REPOSITORIES.filter((name) => name.includes(query.toLowerCase())).map((name) => ({
                value: name,
                label: name,
                description: `${name.length * 13} stars`,
              })),
            ),
          600,
        ),
      ),
  });
  const prefix = mount("prefix", { options: BRANCH_OPTIONS, value: "main", searchable: true, prefixIcon: "branch", placeholder: "选择分支" });
  prefix?.root.addEventListener("ds-change", (event) => {
    document.querySelector('[data-select-output="prefix"]').textContent = `当前值：${event.detail.value ?? "—"}`;
  });
  mount("small", { options: RUNTIME_OPTIONS, value: "codex", size: "sm" });
  mount("large", { options: RUNTIME_OPTIONS, value: "gemini", size: "lg" });
  mount("disabled", { options: RUNTIME_OPTIONS, value: "claude", disabled: true });
  mount("invalid", { options: RUNTIME_OPTIONS, invalid: true, placeholder: "选择运行时" });
}

/* ── 页面预览 ─────────────────────────────────────────────────── */
// 页面预览放在 iframe（pages.html）中，视口宽度变化会真实触发媒体查询。
// 用 postMessage 同步，保证 file:// 直接打开时也能工作。
const VIEWPORTS = { desktop: "100%", tablet: "820px", mobile: "390px" };
let previewPage = localStorage.getItem("multara.playground.page") ?? "dashboard";

function previewFrame() {
  return document.querySelector("#view-pages .pg-viewport iframe");
}

function sendToPreview() {
  const message = { type: "pg-state", theme: state.theme, cssText: root.style.cssText, page: previewPage };
  previewFrame()?.contentWindow?.postMessage(message, "*");
  document.querySelector("#pg-chat-plan-frame")?.contentWindow?.postMessage(message, "*");
}

function bindPreview() {
  const viewport = document.querySelector("#view-pages .pg-viewport");
  const nav = document.querySelector("#pg-page-select");

  function show(page) {
    previewPage = page;
    nav.value = page;
    localStorage.setItem("multara.playground.page", page);
    sendToPreview();
  }

  nav.value = previewPage;
  nav.addEventListener("change", () => show(nav.value));
  previewFrame().addEventListener("load", sendToPreview);
  document.querySelector("#pg-chat-plan-frame")?.addEventListener("load", sendToPreview);
  window.addEventListener("message", (event) => {
    const data = event.data ?? {};
    if (data.type === "pg-page") show(data.page);
    if (data.type === "pg-toggle-theme") {
      state.theme = state.theme === "light" ? "dark" : "light";
      apply();
    }
  });

  document.querySelectorAll("[data-viewport]").forEach((button) => {
    button.addEventListener("click", () => {
      document.querySelectorAll("[data-viewport]").forEach((other) => other.classList.toggle("active", other === button));
      viewport.style.width = VIEWPORTS[button.dataset.viewport];
    });
  });
}

function bindTags() {
  const tags = [
    { id: "fe", name: "前端", color: "#3b82f6" },
    { id: "be", name: "后端", color: "#22c55e" },
    { id: "design", name: "设计", color: "#a855f7" },
  ];
  let selected = ["fe"];
  const trigger = document.getElementById("pg-tag-trigger");
  const pop = document.getElementById("pg-tag-pop");
  const options = document.getElementById("pg-tag-options");
  const query = document.getElementById("pg-tag-query");
  if (!trigger || !pop || !options || !query) return;

  const renderTrigger = () => {
    const chosen = tags.filter((tag) => selected.includes(tag.id));
    trigger.innerHTML = chosen.length
      ? chosen.map((tag) => `<span class="tag-chip"><i class="tag-dot" style="background:${tag.color}"></i><span>${tag.name}</span></span>`).join("")
      : `<span>选择标签</span>`;
  };
  const renderOptions = () => {
    const text = query.value.trim().toLowerCase();
    const shown = tags.filter((tag) => tag.name.toLowerCase().includes(text));
    options.innerHTML = shown.map((tag) => `
      <button type="button" class="tag-option" data-id="${tag.id}">
        <i class="tag-dot" style="background:${tag.color}"></i>
        <span>${tag.name}</span>
        ${selected.includes(tag.id) ? `<span class="tag-check">✓</span>` : ""}
      </button>`).join("")
      || `<p class="tag-empty">没有标签</p>`;
  };
  trigger.addEventListener("click", () => {
    pop.hidden = !pop.hidden;
    renderOptions();
  });
  options.addEventListener("click", (event) => {
    const button = event.target.closest("[data-id]");
    if (!button) return;
    const id = button.dataset.id;
    selected = selected.includes(id) ? selected.filter((item) => item !== id) : [...selected, id];
    renderTrigger();
    renderOptions();
  });
  query.addEventListener("input", renderOptions);
  renderTrigger();

  const chips = document.getElementById("pg-req-chips");
  if (!chips) return;
  chips.innerHTML = tags.map((tag) => `<button type="button" class="tag-filter-chip"><i class="tag-dot" style="background:${tag.color}"></i>${tag.name}</button>`).join("");
  const cardTrigger = document.getElementById("pg-req-trigger");
  if (cardTrigger) cardTrigger.innerHTML = trigger.innerHTML;
}

/* ── 启动 ─────────────────────────────────────────────────────── */
try {
  state = { ...DEFAULTS, ...JSON.parse(localStorage.getItem("multara.playground") ?? "{}") };
  Object.assign(slots, JSON.parse(localStorage.getItem("multara.playground.slots") ?? "{}"));
} catch {
  state = { ...DEFAULTS };
}

bindTags();
renderColors();
renderIcons();
bindControls();
bindTabs();
bindDemos();
bindSelectDemos();
bindPreview();
refreshSlots();
apply();
