const app = document.querySelector('#cp-app');
const scenarioSelect = document.querySelector('#cp-scenario');
const modes = {
  auto: { label: '自动', icon: 'sparkles', description: '由运行时决定是否先规划，再开始执行' },
  plan: { label: '规划', icon: 'file', description: '只读研究并交付计划，不修改项目' },
  agent: { label: '执行', icon: 'play', description: '允许修改文件、运行命令并执行计划' },
};
const profiles = {
  native: { name: 'Claude Code', modes: ['auto', 'plan', 'agent'], native: true, modeChanges: true },
  acp: { name: 'ACP Runtime', modes: ['auto', 'plan', 'agent'], native: false, modeChanges: true },
  basic: { name: '基础运行时', modes: ['auto', 'agent'], native: false, modeChanges: false },
};
const steps = [
  ['统一数据与运行时能力', '补齐事件类型、模式快照和能力声明'],
  ['接入计划审阅与执行', '复用选择器，打通批准、修订和拒绝'],
  ['展示原生子代理执行树', '保留完整父子关系，限制视觉缩进'],
  ['验证恢复与能力降级', '覆盖历史回放、排队消息和异常状态'],
];
const storageKey = 'multara.chat-plan.prototype.v1';
const icon = (name, size = 14, className = '') => `<span class="${className}"><i data-icon="${name}" data-size="${size}"></i></span>`;
const chevron = () => '<span class="cp-chevron"><i data-icon="chevron-right" data-size="12"></i></span>';
const escape = value => String(value).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const button = (action, label, glyph, primary = false, extra = '') => `<button type="button" class="cp-button ${primary ? 'primary' : 'cp-button-quiet'}" data-action="${action}" ${extra}>${glyph ? icon(glyph, 12) : ''}${label}</button>`;
const badge = (text, tone = '') => `<span class="cp-badge" ${tone ? `data-tone="${tone}"` : ''}>${text}</span>`;
const avatar = () => '<span class="cp-avatar" aria-hidden="true">M</span>';

function initialState(scenario = 'review') {
  return {
    scenario, surface: 'chat', profile: scenario === 'limited' ? 'basic' : 'native',
    nextMode: 'auto', requestedMode: scenario === 'limited' ? 'auto' : 'plan',
    planStatus: scenario === 'completed' ? 'executed' : scenario === 'running' || scenario === 'error' ? 'approved' : 'awaiting_approval',
    revision: 1, oldRevisions: [], refinement: '', draft: '', messages: [], queue: [],
    execution: scenario === 'completed' ? 'completed' : scenario === 'running' || scenario === 'error' ? 'running' : '',
    floatingOpen: true, readOnly: false,
  };
}
let state = initialState();
try {
  const saved = JSON.parse(localStorage.getItem(storageKey));
  if (saved && ['review', 'running', 'completed', 'limited', 'error'].includes(saved.scenario)
    && ['chat', 'workbench', 'floating', 'inbox'].includes(saved.surface)
    && profiles[saved.profile] && modes[saved.nextMode] && modes[saved.requestedMode]
    && Array.isArray(saved.messages) && Array.isArray(saved.queue) && Array.isArray(saved.oldRevisions)) {
    state = { ...state, ...saved };
  }
} catch { /* Storage is optional for this preview. */ }
let menuOpen = false;
let refinementOpen = false;
let toastTimer;
const profile = () => profiles[state.profile];
const running = () => state.execution === 'running';
const planSupported = () => profile().modes.includes('plan');
const persist = () => {
  try { localStorage.setItem(storageKey, JSON.stringify(state)); } catch { /* Storage is optional for this preview. */ }
};
function notify(text) {
  const toast = document.querySelector('#cp-toast');
  clearTimeout(toastTimer);
  toast.textContent = text;
  toast.hidden = false;
  toastTimer = setTimeout(() => { toast.hidden = true; }, 3500);
}

function sidebar() {
  return `<aside class="cp-sidebar" aria-label="应用导航示意">
    <div class="cp-sidebar-group"><span class="cp-overline">工作区</span>
      ${[['folder', '需求', 'workbench'], ['message', '聊天', 'chat'], ['inbox', '收件箱', 'inbox']].map(([glyph, text, surface]) =>
        `<button type="button" class="cp-nav-item" data-surface="${surface}" ${state.surface === surface ? 'aria-current="page"' : ''}>${icon(glyph)}${text}${surface === 'inbox' ? '<small>1</small>' : ''}</button>`).join('')}
    </div>
    <div class="cp-sidebar-group"><span class="cp-overline">当前需求</span><div class="cp-nav-item">${icon('branch')}聊天能力升级</div><div class="cp-nav-item">${icon('folder')}Multara <small>main</small></div></div>
    <div class="cp-sidebar-group"><span class="cp-overline">最近对话</span><div class="cp-nav-item" ${state.surface === 'chat' ? 'aria-current="page"' : ''}>${icon('file')}Plan 与子代理</div><div class="cp-nav-item">${icon('message')}运行时能力梳理</div></div>
    <div class="cp-sidebar-note">先把要做的事想清楚，<br>再一起把它做完。</div>
  </aside>`;
}
function inspector() {
  const effective = state.scenario === 'limited' ? 'auto' : 'plan';
  const current = state.execution ? 'agent' : effective;
  return `<aside class="cp-inspector" aria-label="运行上下文">
    <section><h2>运行上下文</h2><dl class="cp-facts">
      <div><dt>智能体</dt><dd>Miku</dd></div><div><dt>运行时</dt><dd>${escape(profile().name)}</dd></div>
      <div><dt>会话</dt><dd><code>chat_plan_demo</code></dd></div><div><dt>用户指定</dt><dd>${modes[state.requestedMode].label}</dd></div>
      <div><dt>启动模式</dt><dd>${modes[effective].label}</dd></div><div><dt>当前模式</dt><dd>${badge(modes[current].label, current === 'plan' ? 'purple' : '')}</dd></div>
      <div><dt>下一条消息</dt><dd>${modes[state.nextMode].label}</dd></div>
    </dl></section>
    <section><span class="cp-overline">运行时能力 · 模拟数据</span>
      <select id="cp-runtime" class="ui-select cp-runtime-select" aria-label="运行时能力示例">${Object.entries(profiles).map(([id, p]) => `<option value="${id}" ${id === state.profile ? 'selected' : ''}>${p.name}（能力示例）</option>`).join('')}</select>
      ${[[planSupported(), '原生只读规划'], [profile().modeChanges, '可观测模式变化'], [planSupported(), '结构化计划'], [profile().native, '原生嵌套子代理'], [false, '运行中手动切换']].map(([supported, text]) => `<div class="cp-capability ${supported ? '' : 'is-unavailable'}">${icon(supported ? 'check-circle' : 'x', 12)}${text}</div>`).join('')}
      <p>能力仅用于演示 UI，不代表本机 CLI 检测结果。模式选择只影响下一条消息。</p>
    </section>
    <section><span class="cp-overline">工作区</span><div class="cp-capability">${icon('folder', 12)}Multara</div><p>计划里的命令和路径仅作预览，批准不会直接执行文本中的 shell。</p></section>
    <section><span class="cp-overline">关于子代理</span><p class="cp-boundary">当前展示运行时原生子代理。</p><p>Multara managed 子运行属于后续阶段，本原型不提供创建或单独停止操作。</p></section>
  </aside>`;
}
function transition(to, source = '用户指定', text = '') {
  return `<div class="cp-transition" data-mode="${to}">${icon(modes[to].icon, 12)}<span>${text || (to === 'plan' ? '已进入规划模式' : '已切换到执行模式')}</span><small>· ${source}</small></div>`;
}
function userMessage(text, mode, meta = '14:32') {
  return `<article class="cp-user"><p>${escape(text)}</p><div class="cp-message-meta"><span class="cp-mode-label">${icon(modes[mode].icon, 10)}${modes[mode].label}</span><span>·</span>${escape(meta)}</div></article>`;
}
function speaker(run = 'run_01') {
  return `<div class="cp-speaker">${avatar()}<strong>Miku</strong><span>${run}</span></div>`;
}
function planMarkdown() {
  return `# 聊天 Plan 模式与原生子代理支持\n\n修订：v${state.revision}\n\n先统一运行时能力和事件契约，再贯通「规划 → 审阅 → 执行」，四种聊天入口共用组件。\n\n${steps.map(([title, description], i) => `${i + 1}. ${title}：${description}`).join('\n')}\n\n## 风险\n不同 CLI 的 Plan 权限不同；不支持原生只读规划的运行时必须禁用规划选项。\n\n## 边界\n本阶段只展示 native sub-agent，不实现 managed 子运行。\n${state.refinement ? `\n## 补充意见\n${state.refinement}\n` : ''}`;
}
function planCard() {
  const statuses = {
    awaiting_approval: ['等待确认', 'purple'], approved: ['已批准', 'success'],
    executed: ['已执行', 'success'], rejected: ['已拒绝', ''],
  };
  const [label, tone] = statuses[state.planStatus];
  const actionable = state.planStatus === 'awaiting_approval';
  return `${state.oldRevisions.map(rev => `<details class="cp-old-plan" data-detail="revision-${rev}"><summary>${chevron()}${icon('file', 12)}聊天 Plan 模式与原生子代理支持 · v${rev}<span>已被 v${rev + 1} 替代</span></summary><p>旧修订仅供查看，请以最新计划为准。</p><button type="button" class="cp-button" disabled>旧版本不可执行</button></details>`).join('')}
    <article class="cp-plan" data-plan-id="plan-chat-demo" data-revision="${state.revision}">
      <div class="cp-plan-topline">${icon('file', 13)}计划产物<code>v${state.revision}</code>${badge(label, tone)}</div>
      <div class="cp-plan-content"><h2>聊天 Plan 模式与原生子代理支持</h2>
        <p class="cp-plan-description">先统一能力和事件契约，再贯通「规划 → 审阅 → 执行」。四种聊天入口复用同一套组件，不重复实现。</p>
        <div class="cp-plan-counts"><span>${icon('list', 11)}4 个步骤</span><span>${icon('alert', 11)}1 项风险</span><span>${icon('folder', 11)}Multara</span></div>
        <ol class="cp-steps">${steps.map(([title, description]) => `<li><span><strong>${title}</strong><br>${description}</span></li>`).join('')}</ol>
        <details class="cp-plan-expand" data-detail="plan-full"><summary>${chevron()}查看完整计划</summary><div class="cp-plan-full">
          <h3>实现范围</h3><p>普通聊天、工作台聊天、气泡聊天和收件箱共享模式选择器、计划卡片与子代理展示，不增加四套业务逻辑。</p>
          <h3>数据与模式</h3><p>区分 requested / effective / current mode。发送时冻结模式；会话默认值只影响后续消息。规范事件持久化到 <code>run_events</code>，刷新后从同一事实来源恢复。</p>
          <h3>计划与 Todo</h3><p>计划是带修订号的待审批产物；Todo 只记录当前执行进度。批准引用准确的 <code>planId + revision</code>，新一轮固定为执行模式。</p>
          <h3>验证方式</h3><ul><li>模式排队快照和恢复后一致。</li><li>四层子代理不丢事件，也不覆盖根回复。</li><li>不支持 Plan 的运行时明确禁用，显示原因。</li></ul>
          <h3>本期边界</h3><p>只观察运行时原生子代理。Multara managed 子运行、调度和并发控制留到第三阶段。</p>
          ${state.refinement ? `<h3>本次修订意见</h3><p>${escape(state.refinement)}</p>` : ''}
        </div></details>
        <div class="cp-plan-risk">${icon('alert', 12)}<span>运行时的只读权限不同；未验证原生 Plan 的运行时不开放规划模式。</span></div>
      </div>
      <footer class="cp-plan-actions">
        ${actionable ? `${button('approve', '批准并执行', 'play', true, !planSupported() ? 'disabled title="当前运行时不支持该 Plan 流程"' : '')}${button('refine', '继续完善', 'edit', false, !planSupported() ? 'disabled' : '')}${button('reject', '拒绝', 'x')}` : `<span class="cp-plan-status-note">${icon(state.planStatus === 'rejected' ? 'x' : 'check-circle', 12)}${state.planStatus === 'approved' ? `v${state.revision} 已批准 · 关联执行轮 run_02` : state.planStatus === 'executed' ? '根执行轮已完成' : '已拒绝，不会启动执行'}</span>`}
        <button type="button" class="cp-icon-button cp-copy" data-action="copy" aria-label="复制计划" title="复制计划">${icon('copy', 13)}</button>
      </footer>
      ${refinementOpen ? `<form class="cp-refinement" id="cp-refinement"><label for="cp-feedback">你希望怎么完善这份计划？</label><textarea id="cp-feedback" placeholder="例如：先完成 UI，暂不接后端…" required maxlength="4000">${escape(state.refinement)}</textarea><div class="cp-refinement-actions">${button('cancel-refine', '取消', '')}<button class="cp-button primary" type="submit">${icon('file', 12)}以规划模式继续</button></div></form>` : ''}
    </article>`;
}
function agentCard({ id, name, task, status, depth, duration, tools, files = 0, result, children = [], open = false }) {
  const labels = { done: '完成', running: '运行中', failed: '失败', cancelled: '已取消' };
  return `<details class="cp-agent" data-detail="agent-${id}" data-status="${status}" data-depth="${depth}" ${open ? 'open' : ''}>
    <summary>${chevron()}${icon('bot', 13)}<span class="cp-agent-identity"><span class="cp-agent-name">${name}<em>L${depth}</em></span><small>${task}</small></span><span class="cp-agent-state">${labels[status]}</span></summary>
    <div class="cp-agent-body"><div class="cp-agent-metrics"><span>${duration}</span><span>${tools} 次工具调用</span><span>${files} 个改动文件</span></div><p class="cp-agent-result">${result}</p>
      ${children.length ? `<div class="cp-agent-children">${children.map(agentCard).join('')}</div>` : ''}
    </div></details>`;
}
function subagents(research = false) {
  const finished = research || state.execution === 'completed';
  const stopped = state.execution === 'stopped';
  const status = finished ? 'done' : stopped ? 'cancelled' : 'running';
  const nodes = research ? [
    { id: 'research', name: 'Explore', task: '梳理现有聊天组件与运行时协议', status: 'done', depth: 1, duration: '18 秒', tools: 6, result: '四种入口可复用 ChatPanel 和同一套时间线数据。' },
    { id: 'permissions', name: 'Explore', task: '核对原生 Plan 的权限边界', status: 'done', depth: 1, duration: '12 秒', tools: 4, result: '原生能力需由适配器声明，不能用提示词冒充只读。' },
  ] : [
    { id: 'implementation', name: 'Implement', task: '接入聊天模式与计划卡片', status, depth: 1, duration: finished ? '1 分 42 秒' : '1 分 08 秒', tools: 12, files: 4, open: true,
      result: finished ? '模式选择和计划审批 UI 已完成，结果已交回根代理。' : stopped ? '随根运行取消，保留已产生的工具事件。' : '已接入共享模式选择器，正在核对事件恢复逻辑。', children: [
        { id: 'inspect', name: 'Explore', task: '检查事件树与历史恢复', status: 'done', depth: 2, duration: '21 秒', tools: 5, open: true, result: '已确认孙级事件需要从完整索引挂接。', children: [
          { id: 'fixture', name: 'Verify', task: '验证深层事件 fixture', status: 'done', depth: 3, duration: '8 秒', tools: 2, open: true, result: '三层父子关系完整，继续核对第四层。', children: [
            { id: 'deep', name: 'Inspect', task: '核对叶节点与根回复隔离', status: 'done', depth: 4, duration: '3 秒', tools: 1, result: 'L4 事件完整保留，不再增加缩进，也不会覆盖根助手正文。' },
          ] },
        ] },
      ] },
    { id: 'tests', name: 'Test', task: '验证 Plan 权限与能力降级', status: state.scenario === 'error' ? 'failed' : status, depth: 1, duration: '32 秒', tools: 7, files: 1,
      result: state.scenario === 'error' ? '子代理事件流中断。失败已保留；父代理继续处理，不自动判定根运行失败。' : finished ? '模式快照、审批幂等与禁用态验证通过。' : stopped ? '随根运行取消。' : '正在检查不支持 Plan 的运行时是否正确禁用规划入口。' },
  ];
  return `<section class="cp-subagents" aria-label="原生子代理执行树"><header>${icon('workflow', 12)}原生子代理${badge('native')}<small>${research ? '2 个子代理 · 已完成' : `2 个直接子代理 · 深度 4`}</small></header>${nodes.map(agentCard).join('')}</section>`;
}
function todoCard() {
  const complete = state.execution === 'completed';
  return `<section class="cp-todo" aria-label="执行 Todo"><header>${icon('list', 13)}<strong>执行进度</strong><small>Todo · ${complete ? '4' : '2'} / 4</small></header><ul>${steps.map(([title], i) => {
    const status = complete || i < 2 ? 'done' : i === 2 && running() ? 'running' : 'pending';
    return `<li data-status="${status}">${status === 'pending' ? '<span class="cp-todo-circle"></span>' : icon(status === 'done' ? 'check-circle' : 'clock', 12)}<span>${title}</span></li>`;
  }).join('')}</ul></section>`;
}
function execution() {
  if (!state.execution) return '';
  const finished = state.execution === 'completed';
  const stopped = state.execution === 'stopped';
  return `<section class="cp-execution">${userMessage(`批准并执行计划 v${state.revision}，按已确认的范围推进。`, 'agent', `引用 plan-chat-demo · v${state.revision}`)}
    <article class="cp-assistant">${speaker('run_02')}${transition('agent', '计划批准 · 新一轮')}
      <p>${finished ? '计划执行轮已完成。下面是最终 Todo 和原生子代理结果，根回复与子代理正文分别保留。' : stopped ? '根运行已停止，原生子代理随所在进程结束。已批准的计划保留，未标记为执行完成。' : '开始按已批准的计划推进。我会先完成共享组件，再验证恢复与降级；子代理的过程会分别保留。'}</p>
      ${state.scenario === 'error' ? `<div class="cp-alert" data-tone="error">${icon('alert', 13)}<p>Test 子代理流中断，已标记失败。根运行仍在继续，可展开该子代理查看结果。</p></div>` : ''}
      ${todoCard()}${profile().native ? subagents() : `<div class="cp-alert">${icon('info', 13)}<p>该能力示例未提供原生子代理父子关系，不展示推测的执行树。</p></div>`}
      <div class="cp-execution-actions">${running() ? `${button('complete', '模拟根运行完成', 'check-circle')}${button('stop', '停止根运行', 'x')}` : `<span class="cp-message-meta">${icon(finished ? 'check-circle' : 'x', 12)}${finished ? '已完成 · 计划已标记为已执行' : '已停止 · 计划仍为已批准'}</span>`}</div>
    </article></section>`;
}
function thread() {
  const limited = state.scenario === 'limited';
  return `<div class="cp-date">今天 · 聊天能力升级</div>
    ${userMessage(limited ? '帮我看看聊天里如何加入 Plan 模式和子代理展示。' : '按方案实现聊天 Plan 模式和 Sub-agent。先梳理一份计划，确认后再开始修改。', state.requestedMode)}
    <article class="cp-assistant">${speaker()}
      ${limited ? `<p>当前运行时仅支持自动和执行模式，没有已验证的原生只读 Plan 能力。</p><div class="cp-alert">${icon('info', 13)}<p>规划选项已禁用。你仍可在自动模式讨论方案，但这不等于原生 Plan，也不承诺只读权限。</p></div>` : `
      <details class="cp-process" data-detail="research"><summary>${chevron()}已完成只读研究<span>· 10 次工具调用</span><small>28 秒</small></summary><div class="cp-process-body">
        ${transition('plan')}
        <div class="cp-tool-row">${icon('check', 11)}<code>Read</code><span>聊天组件与运行时协议</span></div>
        <div class="cp-tool-row">${icon('check', 11)}<code>Search</code><span>Plan、Todo 与 parentCallId</span></div>
        ${profile().native ? subagents(true) : ''}
      </div></details>
      ${transition('plan', '运行时确认', '规划轮已结束 · 等待你的审阅')}
      <p>方案已整理好。本轮只做了研究，没有修改项目文件。建议先打通原生能力与展示，暂不引入 managed 子运行。</p>
      ${planCard()}`}
      <div class="cp-message-meta">14:33<span>·</span>${limited ? '自动模式' : '规划轮已完成'}</div>
    </article>${execution()}
    ${state.messages.map(message => `${userMessage(message.text, message.mode, '刚刚 · 本地演示')}<article class="cp-assistant"><p>${message.mode === 'plan' ? '已按规划模式接收。本原型仅展示消息和模式快照，不启动真实只读运行。' : message.mode === 'agent' ? '已按执行模式接收。本原型不会实际修改文件或运行命令。' : '已按自动模式接收。本原型不会调用真实运行时。'}</p></article>`).join('')}`;
}
function modeMenu() {
  return `<div class="cp-mode-menu" id="cp-mode-menu" role="listbox" aria-label="下一条消息的模式" ${menuOpen ? '' : 'hidden'}><p>下一条消息的模式</p>${Object.entries(modes).map(([id, mode]) => {
    const available = profile().modes.includes(id);
    return `<button type="button" class="cp-mode-option" role="option" data-mode="${id}" aria-selected="${id === state.nextMode}" aria-disabled="${!available}">${icon(mode.icon, 14)}<span><strong>${mode.label}</strong><small>${available ? mode.description : '此运行时未验证原生只读 Plan，不支持规划模式'}</small></span>${id === state.nextMode ? icon('check', 12) : '<span></span>'}</button>`;
  }).join('')}<div class="cp-mode-menu-footer">${running() ? '当前运行继续执行，切换只影响后续消息。' : '发送时冻结模式；切换不会修改已排队消息。'}</div></div>`;
}
function composer() {
  if (state.surface === 'inbox' && state.readOnly) return '<div class="cp-read-only">只读消息 · 模式与计划操作不可用</div>';
  return `<div class="cp-composer-dock">
    <div class="cp-queue" aria-label="排队消息">${state.queue.map((message, index) => `<div class="cp-queue-item">${icon('clock', 11)}<span>${escape(message.text)}</span><span class="cp-mode-label">${icon(modes[message.mode].icon, 10)}${modes[message.mode].label} · 已冻结</span><button class="cp-icon-button" type="button" data-remove-queue="${index}" aria-label="移除排队消息">${icon('x', 10)}</button></div>`).join('')}</div>
    <form class="cp-composer" id="cp-composer">${modeMenu()}<textarea id="cp-draft" aria-label="消息内容" placeholder="${state.nextMode === 'plan' ? '描述你的目标，先一起把计划想清楚…' : running() ? '补充说明，将加入下一轮队列…' : '发送消息，或选择规划模式先讨论方案…'}" maxlength="8000">${escape(state.draft)}</textarea>
      <div class="cp-composer-tools"><button type="button" class="cp-mode-trigger" data-action="mode-menu" data-mode="${state.nextMode}" aria-haspopup="listbox" aria-expanded="${menuOpen}" aria-controls="cp-mode-menu">${icon(modes[state.nextMode].icon, 12)}${modes[state.nextMode].label}${icon('chevron-down', 10)}</button>
        <span class="cp-composer-target">${avatar()}Miku${icon('chevron-down', 10)}</span>
        <button type="submit" class="primary cp-send" aria-label="${running() ? '加入队列' : '发送消息'}" title="${running() ? '加入队列' : '发送消息'}" ${state.draft.trim() ? '' : 'disabled'}>${icon(running() ? 'plus' : 'arrow-up', 15)}</button>
      </div>
    </form><div class="cp-composer-hint"><span>${state.nextMode === 'plan' ? '只读规划 · 不修改项目文件' : running() ? '本轮执行中 · 新消息按所选模式排队' : '所选模式仅用于下一条消息'}</span><span>Enter 发送 · Shift + Enter 换行</span></div>
  </div>`;
}
function chat() {
  return `<section class="cp-chat" aria-label="与 Miku 的聊天"><header class="cp-chat-header"><div class="cp-chat-heading">${avatar()}<div><strong>聊天 Plan 模式与 Sub-agent</strong><small>Miku · ${escape(profile().name)} · 能力示例</small></div></div><div class="cp-chat-header-actions"><span class="cp-live"><i class="cp-dot"></i>${running() ? '执行中' : '工作区已连接'}</span>${state.surface === 'floating' ? `<button type="button" class="cp-icon-button" data-action="floating-toggle" aria-label="收起气泡聊天">${icon('x', 14)}</button>` : ''}</div></header><div class="cp-thread"><div class="cp-messages">${thread()}</div></div>${composer()}</section>`;
}
function editor() {
  return `<section class="cp-editor" aria-label="工作台布局示意"><div class="cp-editor-tabs">${icon('file', 13)}runtime-capabilities.ts ${badge('预览')}</div><div class="cp-editor-content"><code> 1  <b>export type</b> RunMode =
 2    <span>"auto"</span> | <span>"plan"</span> | <span>"agent"</span>
 3
 4  <b>interface</b> RuntimeCapabilities {
 5    requestedModes: RunMode[]
 6    observableModeChanges: boolean
 7    structuredPlan: boolean
 8    nativeSubagents: boolean
 9    nestedSubagents: boolean
10  }</code><div class="cp-editor-note"><p>这是工作台布局示意，代码不可编辑。聊天侧栏复用普通聊天的模式选择、计划和子代理组件。</p></div></div><div class="cp-editor-footer">${icon('branch', 11)}main · 工作区只读预览</div></section>`;
}
function inbox() {
  return `<aside class="cp-inbox-list"><h2>收件箱</h2><article class="is-active"><strong>聊天 Plan 模式与 Sub-agent</strong><p>Miku · ${running() ? '正在执行已批准的计划' : '计划已准备好，等待审阅'}</p>${badge(running() ? '执行中' : '等待确认', 'purple')}</article><article><strong>运行时能力梳理</strong><p>已整理各适配器的能力边界。</p>${badge('已完成', 'success')}</article><article><strong>事件树恢复验证</strong><p>四层嵌套 fixture 已通过。</p>${badge('已完成', 'success')}</article><button type="button" class="cp-button cp-button-quiet" data-action="read-only">${icon(state.readOnly ? 'edit' : 'file', 12)}${state.readOnly ? '切回允许回复' : '预览只读状态'}</button></aside>`;
}
function floating() {
  return `<div class="cp-floating-stage"><div class="cp-floating-backdrop"><span class="cp-overline">MULTARA WORKSPACE</span><h1>把想法变成下一步</h1><p>聊天保持在手边，不打断当前工作。</p><div class="cp-placeholder-list" aria-hidden="true"><div></div><div></div><div></div></div></div>${state.floatingOpen ? `<div class="cp-floating-chat">${chat()}</div>` : '<span class="cp-floating-empty">模式、计划和草稿均已保留</span>'}<button type="button" class="primary cp-floating-toggle" data-action="floating-toggle" aria-label="${state.floatingOpen ? '收起气泡聊天' : '打开气泡聊天'}">${icon(state.floatingOpen ? 'chevron-down' : 'message', 17)}</button></div>`;
}
function render({ preserve = true, scrollToBottom = false } = {}) {
  const threadElement = app.querySelector('.cp-thread');
  const scrollTop = preserve ? threadElement?.scrollTop || 0 : 0;
  const details = new Map(preserve ? [...app.querySelectorAll('details[data-detail]')].map(el => [el.dataset.detail, el.open]) : []);
  app.dataset.surface = state.surface;
  app.innerHTML = `<div class="cp-window"><header class="cp-titlebar"><div class="cp-window-controls" aria-hidden="true"><i></i><i></i><i></i></div><strong>${icon('logo', 13)}Multara</strong><span>聊天能力升级 / ${state.surface === 'chat' ? '普通聊天' : state.surface === 'workbench' ? '工作台' : state.surface === 'inbox' ? '收件箱' : '气泡聊天'}</span><span>UI Prototype</span></header><div class="cp-window-body">${sidebar()}${state.surface === 'floating' ? floating() : `<div class="cp-content">${state.surface === 'workbench' ? editor() : state.surface === 'inbox' ? inbox() : ''}${chat()}${state.surface === 'chat' ? inspector() : ''}</div>`}</div><footer class="cp-statusbar"><span>${icon('folder', 10)}Multara · 本地预览数据</span><span>${running() ? '根运行执行中 · 子代理仅观察' : '计划与执行 Todo 分开记录'}</span></footer></div>`;
  if (state.readOnly && state.surface === 'inbox') app.querySelectorAll('.cp-plan-actions button, .cp-refinement button, .cp-execution-actions button').forEach(el => { el.disabled = true; });
  app.querySelectorAll('details[data-detail]').forEach(el => { if (details.has(el.dataset.detail)) el.open = details.get(el.dataset.detail); });
  window.renderIcons(document);
  const nextThread = app.querySelector('.cp-thread');
  if (nextThread) nextThread.scrollTop = scrollToBottom ? nextThread.scrollHeight : scrollTop;
  document.querySelectorAll('.cp-surfaces [data-surface]').forEach(el => el.setAttribute('aria-pressed', String(el.dataset.surface === state.surface)));
  scenarioSelect.value = state.scenario;
  persist();
}
function reset(scenario) {
  const { surface, floatingOpen } = state;
  state = { ...initialState(scenario), surface, floatingOpen };
  menuOpen = false;
  refinementOpen = false;
  render({ preserve: false, scrollToBottom: true });
}
function submitMessage() {
  const text = state.draft.trim();
  if (!text) return;
  if (!profile().modes.includes(state.nextMode)) { notify('该运行时不支持所选模式，草稿已保留。'); return; }
  const message = { text, mode: state.nextMode };
  if (running()) state.queue.push(message); else state.messages.push(message);
  state.draft = '';
  menuOpen = false;
  render({ scrollToBottom: true });
  app.querySelector('#cp-draft')?.focus();
  if (running()) notify(`已加入队列，模式固定为「${modes[message.mode].label}」。`);
}
const actions = {
  'mode-menu': () => { menuOpen = !menuOpen; render(); if (menuOpen) app.querySelector('.cp-mode-option[aria-selected="true"]')?.focus(); },
  approve: () => {
    if (state.planStatus !== 'awaiting_approval' || !planSupported() || state.readOnly) return;
    state.planStatus = 'approved'; state.execution = 'running'; refinementOpen = false;
    render({ scrollToBottom: true }); notify(`已引用 plan-chat-demo / v${state.revision}，模拟启动执行轮 run_02。`);
  },
  refine: () => { if (state.planStatus !== 'awaiting_approval' || !planSupported()) return; refinementOpen = true; render(); app.querySelector('#cp-feedback')?.focus(); },
  'cancel-refine': () => { refinementOpen = false; render(); },
  reject: () => { if (state.planStatus !== 'awaiting_approval' || state.readOnly) return; state.planStatus = 'rejected'; refinementOpen = false; render(); notify('计划已拒绝，没有启动执行。'); },
  copy: async () => { try { await navigator.clipboard.writeText(planMarkdown()); notify('已复制完整计划。'); } catch { notify('浏览器未授予剪贴板权限，请展开计划后手动复制。'); } },
  complete: () => { if (!running() || state.readOnly) return; state.execution = 'completed'; state.planStatus = 'executed'; state.messages.push(...state.queue); state.queue = []; render(); notify('模拟根运行完成：计划已执行；排队消息已保留其原始模式。'); },
  stop: () => { if (!running() || state.readOnly) return; state.execution = 'stopped'; render(); notify('模拟停止根运行及原生子代理；排队消息保留，计划未标记为完成。'); },
  'floating-toggle': () => { state.floatingOpen = !state.floatingOpen; menuOpen = false; render(); },
  'read-only': () => { state.readOnly = !state.readOnly; refinementOpen = false; render(); },
  reset: () => { reset(state.scenario); notify('当前演示已重置。'); },
  theme: () => { if (window.parent !== window) window.parent.postMessage({ type: 'pg-toggle-theme' }, window.location.origin); else document.documentElement.dataset.theme = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark'; },
};
document.addEventListener('click', event => {
  const target = event.target.closest('button');
  if (target?.disabled) return;
  if (target?.dataset.surface) { state.surface = target.dataset.surface; state.readOnly = false; menuOpen = false; render({ preserve: false, scrollToBottom: true }); return; }
  if (target?.classList.contains('cp-mode-option')) {
    if (target.getAttribute('aria-disabled') === 'true') { notify('该运行时不支持原生 Plan，未切换模式。'); return; }
    state.nextMode = target.dataset.mode; menuOpen = false; render(); app.querySelector('.cp-mode-trigger')?.focus(); return;
  }
  if (target?.hasAttribute('data-remove-queue')) { state.queue.splice(Number(target.dataset.removeQueue), 1); render(); return; }
  if (target?.dataset.action && actions[target.dataset.action]) { actions[target.dataset.action](); return; }
  if (menuOpen && !event.target.closest('.cp-mode-menu')) { menuOpen = false; render(); }
});
app.addEventListener('input', event => {
  if (event.target.id === 'cp-draft') {
    state.draft = event.target.value;
    app.querySelector('.cp-send').disabled = !state.draft.trim(); persist();
  }
});
app.addEventListener('submit', event => {
  event.preventDefault();
  if (event.target.id === 'cp-composer') submitMessage();
  if (event.target.id === 'cp-refinement' && state.planStatus === 'awaiting_approval') {
    const feedback = app.querySelector('#cp-feedback').value.trim();
    if (!feedback) return;
    state.oldRevisions.push(state.revision); state.revision += 1; state.refinement = feedback;
    refinementOpen = false; render(); notify(`以规划模式生成 v${state.revision}；旧修订已失效，等待重新批准。`);
  }
});
app.addEventListener('keydown', event => {
  if (event.target.id === 'cp-draft' && event.key === 'Enter' && !event.shiftKey && !event.isComposing) { event.preventDefault(); submitMessage(); }
  if (event.target.closest('.cp-mode-menu') && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
    event.preventDefault(); const options = [...app.querySelectorAll('.cp-mode-option')]; const current = options.indexOf(document.activeElement);
    const index = event.key === 'Home' ? 0 : event.key === 'End' ? options.length - 1 : (current + (event.key === 'ArrowDown' ? 1 : -1) + options.length) % options.length;
    options[index].focus();
  }
});
document.addEventListener('keydown', event => { if (event.key === 'Escape' && menuOpen) { menuOpen = false; render(); app.querySelector('.cp-mode-trigger')?.focus(); } });
app.addEventListener('change', event => {
  if (event.target.id !== 'cp-runtime') return;
  state.profile = event.target.value;
  if (!profile().modes.includes(state.nextMode)) { state.nextMode = 'auto'; notify('当前能力不支持规划，下一条模式已恢复为自动；草稿不变。'); }
  render();
});
scenarioSelect.addEventListener('change', () => reset(scenarioSelect.value));
window.addEventListener('message', event => {
  if (event.source !== window.parent || event.origin !== window.location.origin || event.data?.type !== 'pg-state') return;
  document.documentElement.dataset.theme = event.data.theme === 'light' ? 'light' : 'dark';
  document.documentElement.style.cssText = event.data.cssText || '';
});
render({ preserve: false, scrollToBottom: true });
