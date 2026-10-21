/*
 * Dropdown Select（Playground 演示实现，原生 JS，无依赖）
 *
 * 用法：DsSelect.mount(element, config)
 *
 * config:
 *   options      选项数组；或用 groups 传分组 [{ label, options }]
 *   placeholder  占位文字
 *   searchable   展开后显示搜索框（支持拼音首字母 / 全拼）
 *   multiple     多选（标签展示）
 *   maxTagCount  多选时最多展示的标签数，超出折叠为 +N
 *   max          多选上限
 *   clearable    可清空
 *   creatable    可创建：输入不存在的内容后回车创建新选项
 *   load(query)  远程搜索：返回 Promise<options>，输入时防抖调用
 *   disabled / invalid / size("sm"|"md"|"lg") / prefixIcon / value / emptyText
 *
 * option: { value, label, description?, avatar?(emoji 或图片 URL), meta?, disabled?, pinyin?, keywords? }
 *   pinyin：可选的全拼（按汉字以空格分隔，如 "qian duan"），提供后支持全拼及混拼搜索；
 *           不提供时自动按拼音首字母匹配（基于 Intl.Collator 中文拼音排序）。
 */
(() => {
  /* ── 拼音 ─────────────────────────────────────────────────────── */
  // 原理：在「拼音排序」规则下，每个声母区间取排在最前面的汉字作为分界；
  // 某个汉字落在哪两个分界字之间，就属于哪个首字母。
  // 分界字经过逐个校验（如 M 段用「妈」、T 段用「他」），改动需重新验证。
  // 注：无 I / U / V 开头的汉字拼音，因此字母表跳过这三个。
  const PINYIN_LETTERS = "ABCDEFGHJKLMNOPQRSTWXYZ";
  const PINYIN_BOUNDARY = "阿八嚓哒妸发旮哈讥咔垃妈拏噢妑七呥扨他穵夕丫帀";
  const collator = new Intl.Collator("zh-Hans-CN-u-co-pinyin");
  const initialCache = new Map();
  const isHan = (ch) => /[\u3400-\u9fff]/.test(ch);

  function pinyinInitial(ch) {
    if (!isHan(ch)) return ch.toLowerCase();
    if (initialCache.has(ch)) return initialCache.get(ch);
    let letter = "";
    for (let i = 0; i < PINYIN_BOUNDARY.length; i += 1) {
      if (collator.compare(PINYIN_BOUNDARY[i], ch) <= 0) letter = PINYIN_LETTERS[i];
      else break;
    }
    const result = letter.toLowerCase();
    initialCache.set(ch, result);
    return result;
  }

  // 把文本拆成「每个字符可匹配的音节」：汉字 → 全拼（若提供）或首字母；其他字符 → 自身小写
  function toUnits(text, pinyin) {
    const syllables = (pinyin ?? "").trim().toLowerCase().split(/\s+/).filter(Boolean);
    let hanIndex = 0;
    return [...text].map((ch) => {
      if (/\s/.test(ch)) return "";
      if (!isHan(ch)) return ch.toLowerCase();
      const full = syllables[hanIndex];
      hanIndex += 1;
      return full ?? pinyinInitial(ch);
    });
  }

  /**
   * 在 text 中查找 query，返回命中的字符区间 [start, end) 或 null。
   * 1) 普通子串（忽略大小写）
   * 2) 拼音：从任意字符开始，每个连续字符消耗其音节的一个非空前缀，直到 query 被完全消耗。
   *    因此 cs / ceshi / ces / cesh 都能命中「测试」。
   */
  function matchText(text, query, pinyin) {
    const q = query.toLowerCase().replace(/\s+/g, "");
    if (!q) return null;
    const plainIndex = text.toLowerCase().indexOf(query.toLowerCase().trim());
    const chars = [...text];
    if (plainIndex >= 0) {
      const start = [...text.slice(0, plainIndex)].length;
      return [start, start + [...query.trim()].length];
    }
    const units = toUnits(text, pinyin);
    const consume = (k, qi) => {
      if (qi === q.length) return k;
      if (k >= units.length) return -1;
      const unit = units[k];
      if (unit === "") return consume(k + 1, qi);
      for (let len = Math.min(unit.length, q.length - qi); len >= 1; len -= 1) {
        if (unit.slice(0, len) === q.slice(qi, qi + len)) {
          const end = consume(k + 1, qi + len);
          if (end >= 0) return end;
        }
      }
      return -1;
    };
    for (let start = 0; start < chars.length; start += 1) {
      if (units[start] === "") continue;
      const end = consume(start, 0);
      if (end >= 0) return [start, end];
    }
    return null;
  }

  /* ── 工具 ─────────────────────────────────────────────────────── */
  const escapeHtml = (value) =>
    String(value).replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);

  function highlight(text, range) {
    const chars = [...text];
    if (!range) return escapeHtml(text);
    const [s, e] = range;
    return (
      escapeHtml(chars.slice(0, s).join("")) +
      `<mark>${escapeHtml(chars.slice(s, e).join(""))}</mark>` +
      escapeHtml(chars.slice(e).join(""))
    );
  }

  const svg = (name, size = 14) =>
    `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] ?? ""}</svg>`;

  function avatarHtml(avatar, size) {
    if (!avatar) return "";
    const isImage = /^(https?:|data:|\.{0,2}\/)/.test(avatar);
    const style = size ? ` style="width:${size}px;height:${size}px"` : "";
    return `<span class="ds-avatar"${style}>${isImage ? `<img src="${escapeHtml(avatar)}" alt="" />` : escapeHtml(avatar)}</span>`;
  }

  let uid = 0;

  /* ── 组件 ─────────────────────────────────────────────────────── */
  class Select {
    constructor(root, config) {
      this.root = root;
      this.config = { placeholder: "请选择", emptyText: "无匹配结果", maxTagCount: Infinity, ...config };
      this.id = `ds-select-${(uid += 1)}`;
      this.groups = config.groups ?? [{ label: "", options: config.options ?? [] }];
      const initial = config.value ?? (config.multiple ? [] : null);
      this.value = config.multiple ? [...initial] : initial;
      this.open = false;
      this.query = "";
      this.activeIndex = -1;
      this.loading = false;
      this.remoteOptions = null;
      this.build();
      this.renderTrigger();
    }

    get allOptions() {
      return [...this.groups.flatMap((g) => g.options), ...(this.remoteOptions ?? []), ...(this.extraSelected ?? [])];
    }

    findOption(value) {
      return this.allOptions.find((option) => option.value === value);
    }

    isSelected(value) {
      return this.config.multiple ? this.value.includes(value) : this.value === value;
    }

    get hasValue() {
      return this.config.multiple ? this.value.length > 0 : this.value != null;
    }

    reachedMax() {
      return this.config.multiple && this.config.max != null && this.value.length >= this.config.max;
    }

    build() {
      const { root, config } = this;
      root.classList.add("ds-select");
      if (config.size && config.size !== "md") root.dataset.size = config.size;
      if (config.disabled) root.dataset.disabled = "";
      if (config.invalid) root.dataset.invalid = "";
      if (config.clearable) root.dataset.clearable = "";
      root.innerHTML = `
        <div class="ds-select-trigger" role="combobox" tabindex="${config.disabled ? -1 : 0}"
          aria-haspopup="listbox" aria-expanded="false" aria-controls="${this.id}-list"
          ${config.disabled ? 'aria-disabled="true"' : ""}></div>
        <div class="ds-select-popover" hidden>
          ${
            config.searchable || config.load
              ? `<label class="ds-select-search">${svg("search")}<input type="text" placeholder="${escapeHtml(config.searchPlaceholder ?? "搜索…")}" aria-controls="${this.id}-list" autocomplete="off" spellcheck="false" /></label>`
              : ""
          }
          <div class="ds-select-list" role="listbox" id="${this.id}-list" ${config.multiple ? 'aria-multiselectable="true"' : ""}></div>
          ${config.multiple ? '<div class="ds-select-footer"></div>' : ""}
        </div>`;
      this.trigger = root.querySelector(".ds-select-trigger");
      this.popover = root.querySelector(".ds-select-popover");
      this.list = root.querySelector(".ds-select-list");
      this.input = root.querySelector(".ds-select-search input");
      this.footer = root.querySelector(".ds-select-footer");

      this.trigger.addEventListener("click", (event) => {
        if (config.disabled) return;
        if (event.target.closest("[data-remove]")) {
          this.toggle(event.target.closest("[data-remove]").dataset.remove);
          return;
        }
        if (event.target.closest(".ds-select-clear")) {
          this.clear();
          return;
        }
        this.setOpen(!this.open);
      });
      root.addEventListener("keydown", (event) => this.onKeydown(event));
      this.input?.addEventListener("input", () => {
        this.query = this.input.value;
        this.activeIndex = 0;
        if (config.load) this.loadRemote();
        else this.renderList();
      });
      // 用 mousedown + preventDefault 选择，避免点击时搜索框失焦导致闪烁
      this.list.addEventListener("mousedown", (event) => event.preventDefault());
      this.list.addEventListener("click", (event) => {
        const item = event.target.closest("[data-index]");
        if (item) this.choose(Number(item.dataset.index));
      });
      this.list.addEventListener("mousemove", (event) => {
        const item = event.target.closest("[data-index]");
        if (item && Number(item.dataset.index) !== this.activeIndex) this.setActive(Number(item.dataset.index), false);
      });
      this.footer?.addEventListener("click", (event) => {
        const action = event.target.closest("[data-action]")?.dataset.action;
        if (action === "all") this.selectAllVisible();
        if (action === "clear") this.clear();
      });
      document.addEventListener("pointerdown", (event) => {
        // 点击外部关闭时不抢回焦点，让焦点自然落到用户点击的位置
        if (this.open && !root.contains(event.target)) this.setOpen(false, { restoreFocus: false });
      });
    }

    /* 触发器 */
    renderTrigger() {
      const { config } = this;
      const prefix = config.prefixIcon ? `<span class="ds-select-prefix">${svg(config.prefixIcon)}</span>` : "";
      let body;
      if (!this.hasValue) {
        body = `<span class="ds-select-value"><span class="ds-select-placeholder">${escapeHtml(config.placeholder)}</span></span>`;
      } else if (config.multiple) {
        const shown = this.value.slice(0, config.maxTagCount);
        const rest = this.value.slice(config.maxTagCount);
        const tags = shown.map((value) => {
          const option = this.findOption(value) ?? { value, label: value };
          return `<span class="ds-select-tag">${avatarHtml(option.avatar)}<span class="ds-select-text">${escapeHtml(option.label)}</span>${
            config.disabled ? "" : `<span class="ds-select-tag-remove" data-remove="${escapeHtml(value)}" role="button" aria-label="移除 ${escapeHtml(option.label)}">${svg("x", 12)}</span>`
          }</span>`;
        });
        if (rest.length) {
          const names = rest.map((value) => this.findOption(value)?.label ?? value).join("、");
          tags.push(`<span class="ds-select-tag is-rest" title="${escapeHtml(names)}">+${rest.length}</span>`);
        }
        body = `<span class="ds-select-tags">${tags.join("")}</span>`;
      } else {
        const option = this.findOption(this.value) ?? { label: this.value };
        body = `<span class="ds-select-value">${avatarHtml(option.avatar, 20)}<span class="ds-select-text">${escapeHtml(option.label)}</span></span>`;
      }
      this.trigger.innerHTML = `${prefix}${body}
        <span class="ds-select-clear" role="button" aria-label="清空">${svg("x", 13)}</span>
        <span class="ds-select-arrow">${svg("chevron-down", 15)}</span>`;
      this.root.toggleAttribute("data-has-value", this.hasValue);
    }

    /* 列表：根据搜索词计算可见项，并扁平化成 items 供键盘导航 */
    computeItems() {
      const query = this.query.trim();
      const sourceGroups = this.remoteOptions ? [{ label: "", options: this.remoteOptions }] : this.groups;
      const groups = [];
      this.items = [];
      for (const group of sourceGroups) {
        const options = [];
        for (const option of group.options) {
          let range = null;
          let descRange = null;
          if (query && !this.config.load) {
            range = matchText(option.label, query, option.pinyin);
            if (!range && option.description) descRange = matchText(option.description, query);
            const keywordHit = !range && !descRange && option.keywords?.some((k) => matchText(k, query));
            if (!range && !descRange && !keywordHit) continue;
          }
          const disabled = option.disabled || (this.reachedMax() && !this.isSelected(option.value));
          this.items.push({ option, disabled });
          options.push({ option, range, descRange, disabled, index: this.items.length - 1 });
        }
        if (options.length) groups.push({ label: group.label, options });
      }
      // 可创建：输入内容与任何选项都不完全相同时，追加「创建」项
      const exists = this.allOptions.some((option) => option.label.toLowerCase() === query.toLowerCase());
      this.createIndex = -1;
      if (this.config.creatable && query && !exists && !this.reachedMax()) {
        this.items.push({ create: query });
        this.createIndex = this.items.length - 1;
      }
      return groups;
    }

    renderList() {
      const groups = this.computeItems();
      const html = [];
      if (this.loading) {
        html.push(`<div class="ds-select-empty"><span class="ds-spinner"></span>加载中…</div>`);
      } else {
        for (const group of groups) {
          html.push(`<div class="ds-select-group" role="group" ${group.label ? `aria-label="${escapeHtml(group.label)}"` : ""}>`);
          if (group.label) {
            html.push(`<div class="ds-select-group-label"><span>${escapeHtml(group.label)}</span><span>${group.options.length}</span></div>`);
          }
          for (const { option, range, descRange, disabled, index } of group.options) {
            const selected = this.isSelected(option.value);
            html.push(`
              <div class="ds-select-option${option.description ? " has-description" : ""}" role="option" id="${this.id}-opt-${index}"
                data-index="${index}" aria-selected="${selected}" ${disabled ? 'aria-disabled="true"' : ""}>
                ${avatarHtml(option.avatar)}
                <span class="ds-select-option-body">
                  <span class="ds-select-option-label">${highlight(option.label, range)}</span>
                  ${option.description ? `<span class="ds-select-option-desc">${highlight(option.description, descRange)}</span>` : ""}
                </span>
                ${option.meta ? `<span class="ds-select-option-meta">${escapeHtml(option.meta)}</span>` : ""}
                <span class="ds-select-check">${svg("check", 14)}</span>
              </div>`);
          }
          html.push("</div>");
        }
        if (this.createIndex >= 0) {
          html.push(`<div class="ds-select-option is-create" role="option" id="${this.id}-opt-${this.createIndex}" data-index="${this.createIndex}" aria-selected="false">
            ${svg("plus", 14)}<span class="ds-select-option-body"><span class="ds-select-option-label">创建「${escapeHtml(this.query.trim())}」</span></span></div>`);
        }
        if (!this.items.length) {
          const text = this.config.load && !this.query.trim() ? "输入关键词开始搜索" : this.config.emptyText;
          html.push(`<div class="ds-select-empty">${svg("search", 18)}${escapeHtml(text)}</div>`);
        }
      }
      this.list.innerHTML = html.join("");
      if (this.footer) {
        const max = this.config.max != null ? ` / ${this.config.max}` : "";
        this.footer.innerHTML = `<span>已选 ${this.value.length}${max} 项</span><div>
          <button type="button" class="ui-button" data-variant="ghost" data-size="small" data-action="all">全选</button>
          <button type="button" class="ui-button" data-variant="ghost" data-size="small" data-action="clear" ${this.hasValue ? "" : "disabled"}>清空</button></div>`;
      }
      // 保持活动项合法
      if (this.activeIndex >= this.items.length || this.items[this.activeIndex]?.disabled) {
        this.activeIndex = this.nextEnabled(-1, 1);
      }
      this.setActive(this.activeIndex);
    }

    nextEnabled(from, step) {
      const count = this.items.length;
      for (let i = 1; i <= count; i += 1) {
        const index = (from + step * i + count * 2) % count;
        if (!this.items[index]?.disabled) return index;
      }
      return -1;
    }

    setActive(index, scroll = true) {
      this.activeIndex = index;
      this.list.querySelectorAll("[data-active]").forEach((el) => el.removeAttribute("data-active"));
      const el = index >= 0 ? this.list.querySelector(`[data-index="${index}"]`) : null;
      el?.setAttribute("data-active", "");
      if (scroll) el?.scrollIntoView({ block: "nearest" });
      const owner = this.input ?? this.trigger;
      if (el) owner.setAttribute("aria-activedescendant", el.id);
      else owner.removeAttribute("aria-activedescendant");
    }

    setOpen(open, { restoreFocus = true } = {}) {
      if (open === this.open) return;
      this.open = open;
      this.root.toggleAttribute("data-open", open);
      this.trigger.setAttribute("aria-expanded", String(open));
      this.popover.hidden = !open;
      if (open) {
        // 下方空间不足时向上展开
        const rect = this.trigger.getBoundingClientRect();
        const below = window.innerHeight - rect.bottom;
        this.root.dataset.placement = below < 320 && rect.top > below ? "top" : "bottom";
        this.query = "";
        if (this.input) this.input.value = "";
        // 打开时定位到当前选中项
        this.renderList();
        const selectedIndex = this.items.findIndex((item) => item.option && this.isSelected(item.option.value));
        if (selectedIndex >= 0) this.setActive(selectedIndex);
        this.input?.focus();
      } else {
        this.remoteOptions = null;
        this.loading = false;
        if (restoreFocus && this.root.contains(document.activeElement)) this.trigger.focus();
      }
    }

    choose(index) {
      const item = this.items[index];
      if (!item || item.disabled) return;
      if (item.create) {
        const option = { value: item.create, label: item.create };
        this.groups[this.groups.length - 1].options.push(option);
        this.toggle(option.value);
        return;
      }
      this.toggle(item.option.value);
    }

    toggle(value) {
      if (this.config.multiple) {
        if (this.value.includes(value)) this.value = this.value.filter((v) => v !== value);
        else if (!this.reachedMax()) this.value = [...this.value, value];
        // 远程选项被选中后需要保留，以便触发器在关闭后仍能显示其标签
        const option = this.remoteOptions?.find((o) => o.value === value);
        if (option && !this.groups.some((g) => g.options.includes(option))) {
          this.extraSelected = [...(this.extraSelected ?? []), option];
        }
        this.query = "";
        if (this.input) this.input.value = "";
        if (this.open) this.renderList();
      } else {
        const option = this.remoteOptions?.find((o) => o.value === value);
        if (option) this.extraSelected = [option];
        this.value = value;
        this.setOpen(false);
      }
      this.renderTrigger();
      this.emit();
    }

    clear() {
      this.value = this.config.multiple ? [] : null;
      this.renderTrigger();
      if (this.open) this.renderList();
      this.emit();
    }

    selectAllVisible() {
      const values = this.items.filter((item) => item.option && !item.option.disabled).map((item) => item.option.value);
      const merged = [...new Set([...this.value, ...values])];
      this.value = this.config.max != null ? merged.slice(0, this.config.max) : merged;
      this.renderTrigger();
      this.renderList();
      this.emit();
    }

    loadRemote() {
      clearTimeout(this.loadTimer);
      const query = this.query.trim();
      if (!query) {
        this.loading = false;
        this.remoteOptions = null;
        this.renderList();
        return;
      }
      this.loading = true;
      this.renderList();
      // 防抖 300ms；用 token 丢弃过期请求的结果
      const token = (this.loadToken = Symbol("load"));
      this.loadTimer = setTimeout(async () => {
        const options = await this.config.load(query);
        if (token !== this.loadToken || !this.open) return;
        this.loading = false;
        this.remoteOptions = options;
        this.activeIndex = 0;
        this.renderList();
      }, 300);
    }

    onKeydown(event) {
      if (this.config.disabled) return;
      const { key } = event;
      if (!this.open) {
        if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(key) && event.target === this.trigger) {
          event.preventDefault();
          this.setOpen(true);
        } else if (key === "Backspace" && this.config.clearable && event.target === this.trigger) {
          this.clear();
        }
        return;
      }
      switch (key) {
        case "ArrowDown":
        case "ArrowUp":
          event.preventDefault();
          this.setActive(this.nextEnabled(this.activeIndex, key === "ArrowDown" ? 1 : -1));
          break;
        case "Home":
        case "End":
          if (event.target === this.input) break;
          event.preventDefault();
          this.setActive(key === "Home" ? this.nextEnabled(-1, 1) : this.nextEnabled(this.items.length, -1));
          break;
        case "Enter":
          event.preventDefault();
          if (this.activeIndex >= 0) this.choose(this.activeIndex);
          break;
        case "Escape":
          event.preventDefault();
          this.setOpen(false);
          break;
        case "Tab":
          this.setOpen(false, { restoreFocus: false });
          break;
        case "Backspace":
          if (event.target === this.input && !this.input.value && this.config.multiple && this.value.length) {
            this.toggle(this.value[this.value.length - 1]);
          }
          break;
        default:
          // 不可搜索时支持「按字母跳转」：输入字母定位到首字母匹配的选项
          if (!this.input && key.length === 1 && /\S/.test(key)) {
            const start = this.activeIndex;
            for (let i = 1; i <= this.items.length; i += 1) {
              const index = (start + i) % this.items.length;
              const item = this.items[index];
              const first = item.option && [...item.option.label][0];
              if (!item.disabled && first && pinyinInitial(first) === key.toLowerCase()) {
                this.setActive(index);
                break;
              }
            }
          }
      }
    }

    emit() {
      this.root.dispatchEvent(new CustomEvent("ds-change", { detail: { value: this.value }, bubbles: true }));
    }
  }

  window.DsSelect = {
    mount: (element, config) => new Select(element, config),
    matchText,
    pinyinInitial,
  };
})();
