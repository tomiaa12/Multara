/** 注入子页面的选元素 / 区域截图脚本（无 IPC，结果写到 window 上由 eval 读回）。 */

export const ELEMENT_PICK_SCRIPT = `(() => {
  const mode = __MODE__;
  const existing = document.getElementById('__multara-pick-root');
  if (existing) existing.remove();
  window.__multaraPick = null;
  window.__multaraPickActive = true;
  const root = document.createElement('div');
  root.id = '__multara-pick-root';
  root.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483646;cursor:crosshair;';
  const tip = document.createElement('div');
  tip.textContent = mode === 'comment'
    ? '点击元素以添加评论 · Esc 取消'
    : '点击元素添加到聊天 · Esc 取消';
  tip.style.cssText = 'position:fixed;left:12px;top:12px;padding:6px 10px;background:#1e1e1e;color:#fff;font:12px/1.4 system-ui,sans-serif;border-radius:6px;pointer-events:none;z-index:1;';
  const box = document.createElement('div');
  box.style.cssText = 'position:fixed;pointer-events:none;border:2px solid #3794ff;background:rgba(55,148,255,.12);display:none;z-index:0;';
  root.appendChild(box);
  root.appendChild(tip);
  document.documentElement.appendChild(root);

  function esc(value) {
    try { return CSS.escape(value); } catch (e) {
      return String(value).replace(/[^a-zA-Z0-9_-]/g, '\\\\$&');
    }
  }
  function cssPath(el) {
    if (!(el instanceof Element)) return '';
    const parts = [];
    let node = el;
    while (node && node.nodeType === 1 && parts.length < 6) {
      let part = node.nodeName.toLowerCase();
      if (node.id) {
        part += '#' + esc(node.id);
        parts.unshift(part);
        break;
      }
      const parent = node.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter((c) => c.nodeName === node.nodeName);
        if (siblings.length > 1) part += ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')';
      }
      parts.unshift(part);
      node = parent;
    }
    return parts.join(' > ');
  }

  function targetFrom(event) {
    root.style.pointerEvents = 'none';
    const el = document.elementFromPoint(event.clientX, event.clientY);
    root.style.pointerEvents = 'auto';
    return el instanceof Element ? el : null;
  }

  function onMove(event) {
    const el = targetFrom(event);
    if (!el || el === document.documentElement || el === document.body) {
      box.style.display = 'none';
      return;
    }
    const r = el.getBoundingClientRect();
    box.style.display = 'block';
    box.style.left = r.left + 'px';
    box.style.top = r.top + 'px';
    box.style.width = r.width + 'px';
    box.style.height = r.height + 'px';
  }

  function finish(el) {
    if (!el) return;
    const html = (el.outerHTML || '').slice(0, 12000);
    const text = (el.innerText || '').trim().slice(0, 2000);
    window.__multaraPick = {
      mode,
      tag: el.tagName.toLowerCase(),
      selector: cssPath(el),
      text,
      html,
    };
    cleanup();
  }

  function cleanup() {
    window.__multaraPickActive = false;
    window.removeEventListener('keydown', onKey, true);
    root.remove();
  }

  function onClick(event) {
    event.preventDefault();
    event.stopPropagation();
    finish(targetFrom(event));
  }

  function onKey(event) {
    if (event.key === 'Escape') {
      event.preventDefault();
      window.__multaraPick = { cancelled: true };
      cleanup();
    }
  }

  root.addEventListener('mousemove', onMove, true);
  root.addEventListener('click', onClick, true);
  window.addEventListener('keydown', onKey, true);
  true;
})()`;

export const AREA_PICK_SCRIPT = `(() => {
  const existing = document.getElementById('__multara-area-root');
  if (existing) existing.remove();
  window.__multaraArea = null;
  window.__multaraAreaActive = true;
  const root = document.createElement('div');
  root.id = '__multara-area-root';
  root.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483646;cursor:crosshair;background:rgba(0,0,0,.12);';
  const tip = document.createElement('div');
  tip.textContent = '拖选区域截图 · Esc 取消';
  tip.style.cssText = 'position:fixed;left:12px;top:12px;padding:6px 10px;background:#1e1e1e;color:#fff;font:12px/1.4 system-ui,sans-serif;border-radius:6px;pointer-events:none;';
  const box = document.createElement('div');
  box.style.cssText = 'position:fixed;border:1px dashed #fff;background:rgba(55,148,255,.2);display:none;pointer-events:none;';
  root.appendChild(tip);
  root.appendChild(box);
  document.documentElement.appendChild(root);
  let start = null;

  function cleanup() {
    window.__multaraAreaActive = false;
    window.removeEventListener('keydown', onKey, true);
    root.remove();
  }

  function onKey(event) {
    if (event.key === 'Escape') {
      event.preventDefault();
      window.__multaraArea = { cancelled: true };
      cleanup();
    }
  }

  root.addEventListener('mousedown', (event) => {
    event.preventDefault();
    start = { x: event.clientX, y: event.clientY };
    box.style.display = 'block';
    box.style.left = start.x + 'px';
    box.style.top = start.y + 'px';
    box.style.width = '0px';
    box.style.height = '0px';
  });
  root.addEventListener('mousemove', (event) => {
    if (!start) return;
    const x = Math.min(start.x, event.clientX);
    const y = Math.min(start.y, event.clientY);
    const w = Math.abs(event.clientX - start.x);
    const h = Math.abs(event.clientY - start.y);
    box.style.left = x + 'px';
    box.style.top = y + 'px';
    box.style.width = w + 'px';
    box.style.height = h + 'px';
  });
  root.addEventListener('mouseup', (event) => {
    if (!start) return;
    const x = Math.min(start.x, event.clientX);
    const y = Math.min(start.y, event.clientY);
    const w = Math.abs(event.clientX - start.x);
    const h = Math.abs(event.clientY - start.y);
    start = null;
    if (w < 4 || h < 4) {
      window.__multaraArea = { cancelled: true };
      cleanup();
      return;
    }
    window.__multaraArea = { x, y, width: w, height: h };
    cleanup();
  });
  window.addEventListener('keydown', onKey, true);
  true;
})()`;

export const STOP_PICK_SCRIPT = `(() => {
  const pick = document.getElementById('__multara-pick-root');
  if (pick) pick.remove();
  const area = document.getElementById('__multara-area-root');
  if (area) area.remove();
  const menu = document.getElementById('__multara-chat-menu');
  if (menu) menu.remove();
  window.__multaraPickActive = false;
  window.__multaraAreaActive = false;
  window.__multaraChatMenuActive = false;
  true;
})()`;

export function elementPickScript(mode: "select" | "comment"): string {
  return ELEMENT_PICK_SCRIPT.replace("__MODE__", JSON.stringify(mode));
}

export type ChatMenuItemSpec = {
  id: string;
  label: string;
  key?: string;
  sep?: boolean;
  checked?: boolean;
};

/** 页内悬浮菜单（盖在网页上，不挤占 Multara 布局高度）。 */
export function chatMenuScript(items: ChatMenuItemSpec[]): string {
  return `(() => {
  const existing = document.getElementById('__multara-chat-menu');
  if (existing) existing.remove();
  window.__multaraChatMenu = null;
  window.__multaraChatMenuActive = true;
  const items = ${JSON.stringify(items)};
  const root = document.createElement('div');
  root.id = '__multara-chat-menu';
  root.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483647;font-family:system-ui,-apple-system,sans-serif;';
  const backdrop = document.createElement('div');
  backdrop.style.cssText = 'position:absolute;inset:0;background:transparent;';
  const panel = document.createElement('div');
  panel.style.cssText = 'position:absolute;top:10px;right:10px;min-width:240px;padding:4px 0;background:#252526;color:#cccccc;border:1px solid #454545;border-radius:6px;box-shadow:0 8px 24px rgba(0,0,0,.45);';
  function finish(id) {
    window.__multaraChatMenu = { id: id };
    window.__multaraChatMenuActive = false;
    window.removeEventListener('keydown', onKey, true);
    root.remove();
  }
  function onKey(event) {
    if (event.key === 'Escape') {
      event.preventDefault();
      finish('');
    }
  }
  backdrop.addEventListener('mousedown', function(event) {
    event.preventDefault();
    finish('');
  });
  items.forEach(function(item) {
    if (item.sep) {
      const hr = document.createElement('div');
      hr.style.cssText = 'height:1px;margin:4px 0;background:#454545;';
      panel.appendChild(hr);
      return;
    }
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.style.cssText = 'all:unset;box-sizing:border-box;display:flex;align-items:center;gap:12px;width:100%;padding:6px 12px;cursor:pointer;font-size:12px;line-height:1.4;';
    btn.onmouseenter = function() { btn.style.background = '#094771'; btn.style.color = '#ffffff'; };
    btn.onmouseleave = function() { btn.style.background = 'transparent'; btn.style.color = '#cccccc'; };
    const label = document.createElement('span');
    label.textContent = item.label;
    label.style.cssText = 'flex:1 1 auto;';
    btn.appendChild(label);
    if (item.checked) {
      const mark = document.createElement('span');
      mark.textContent = '✓';
      mark.style.cssText = 'opacity:.9;';
      btn.appendChild(mark);
    }
    if (item.key) {
      const kbd = document.createElement('span');
      kbd.textContent = item.key;
      kbd.style.cssText = 'opacity:.65;font-size:11px;';
      btn.appendChild(kbd);
    }
    btn.addEventListener('mousedown', function(event) {
      event.preventDefault();
      event.stopPropagation();
      finish(item.id);
    });
    panel.appendChild(btn);
  });
  root.appendChild(backdrop);
  root.appendChild(panel);
  document.documentElement.appendChild(root);
  window.addEventListener('keydown', onKey, true);
  true;
})()`;
}
