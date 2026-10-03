/// Dashboard HTML — 内嵌的完整 Web 管理界面
///
/// 功能：工具卡片（卸载/重载/启用，单击卡片筛选日志）、资源与提示词页签、
/// 服务器断连横幅（SSE 断开时显示；恢复由断连看门狗接管——探到服务器存活就整体
/// 重建事件流，不依赖浏览器后台自动重连，Firefox 后台标签会把重连请求无限推迟）。

pub fn dashboard_html() -> &'static str {
    r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>MCP 控制台 — 风见血月</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  :root {
    --bg: #0d1117; --surface: #161b22; --border: #30363d;
    --text: #e6edf3; --text-muted: #8b949e; --accent: #58a6ff;
    --green: #3fb950; --red: #f85149; --yellow: #d29922; --purple: #a371f7;
  }
  body { font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace; background: var(--bg); color: var(--text); height: 100vh; display: flex; flex-direction: column; }
  #banner { display: none; text-align: center; padding: 8px 16px; font-size: 13px; font-weight: 600; }
  #banner.reconnecting { background: var(--yellow); color: #0d1117; }
  #banner.down { background: var(--red); color: #fff; }
  header { background: var(--surface); border-bottom: 1px solid var(--border); padding: 12px 20px; display: flex; align-items: center; justify-content: space-between; }
  header h1 { font-size: 16px; font-weight: 600; }
  header h1 span { color: var(--red); }
  .status { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-muted); }
  .status .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--green); animation: pulse 2s infinite; }
  .status.disconnected .dot { background: var(--red); animation: none; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
  .main { display: flex; flex: 1; overflow: hidden; }
  .panel-left { width: 380px; min-width: 300px; background: var(--surface); border-right: 1px solid var(--border); display: flex; flex-direction: column; }
  .panel-left.stale { opacity: .5; }
  .panel-left.stale .btn { pointer-events: none; }
  .panel-left.stale::after { content: '断开前的快照'; position: sticky; bottom: 0; text-align: center; font-size: 11px; color: var(--red); padding: 4px; background: rgba(248, 81, 73, 0.12); }
  .tabs { display: flex; border-bottom: 1px solid var(--border); }
  .tab { flex: 1; padding: 10px 0; text-align: center; font-size: 13px; cursor: pointer; color: var(--text-muted); border-bottom: 2px solid transparent; user-select: none; }
  .tab.active { color: var(--accent); border-bottom-color: var(--accent); }
  .panel-header { padding: 12px 16px; border-bottom: 1px solid var(--border); display: flex; align-items: center; justify-content: space-between; }
  .panel-header h2 { font-size: 13px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; color: var(--text-muted); }
  .btn { padding: 4px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); cursor: pointer; font-size: 12px; font-family: inherit; transition: all 0.15s; }
  .btn:hover { background: #21262d; border-color: var(--text-muted); }
  .btn:active { transform: scale(0.97); }
  .btn-unload { border-color: var(--red); color: var(--red); }
  .btn-unload:hover { background: rgba(248, 81, 73, 0.15); }
  .btn-load { border-color: var(--green); color: var(--green); }
  .btn-load:hover { background: rgba(63, 185, 80, 0.15); }
  .btn-reload { border-color: var(--accent); color: var(--accent); }
  .btn-reload:hover { background: rgba(88, 166, 255, 0.15); }
  .tool-list { flex: 1; overflow-y: auto; padding: 8px; }
  .group-header { display: flex; align-items: center; gap: 6px; padding: 8px 6px; margin-bottom: 4px; font-size: 13px; font-weight: 600; cursor: pointer; user-select: none; border-radius: 6px; }
  .group-header:hover { background: rgba(88, 166, 255, 0.08); color: var(--accent); }
  .group-header .arrow { font-size: 10px; color: var(--text-muted); width: 12px; }
  .group-header .group-count { color: var(--text-muted); font-weight: 400; font-size: 12px; }
  .tool-card { background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 14px; margin-bottom: 8px; transition: all 0.2s; cursor: pointer; }
  .tool-card:hover { border-color: var(--accent); }
  .tool-card.selected { border-color: var(--accent); background: rgba(88, 166, 255, 0.08); }
  .tool-card.disabled { opacity: 0.45; }
  .tool-card .tool-name { font-size: 14px; font-weight: 600; margin-bottom: 4px; display: flex; align-items: center; gap: 8px; }
  .tool-card .tool-name .badge { font-size: 10px; padding: 1px 6px; border-radius: 4px; font-weight: 500; }
  .badge-on { background: rgba(63, 185, 80, 0.2); color: var(--green); }
  .badge-off { background: rgba(248, 81, 73, 0.2); color: var(--red); }
  .tool-card .tool-desc { font-size: 12px; color: var(--text-muted); margin-bottom: 10px; line-height: 1.5; }
  .tool-card .tool-actions { display: flex; gap: 6px; }
  .entry-card { background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 12px 14px; margin-bottom: 8px; }
  .entry-card .name { font-size: 13px; font-weight: 600; margin-bottom: 4px; }
  .entry-card .desc { font-size: 12px; color: var(--text-muted); line-height: 1.5; }
  .entry-card .meta { font-size: 11px; color: var(--accent); margin-top: 6px; }
  .panel-right { flex: 1; display: flex; flex-direction: column; background: var(--bg); }
  .rtab { padding: 8px 14px; font-size: 12px; border: none; background: transparent; color: var(--text-muted); cursor: pointer; border-bottom: 2px solid transparent; font-family: inherit; }
  .rtab-active { color: var(--accent); border-bottom-color: var(--accent); }
  .task-card { background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 10px 12px; margin-bottom: 8px; }
  .task-card .thead { display: flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 600; margin-bottom: 6px; }
  .task-card .targs { font-size: 12px; color: var(--text-muted); margin-bottom: 6px; }
  .task-card .tout { background: var(--surface); border: 1px solid var(--border); border-radius: 6px; padding: 8px; font-size: 12px; line-height: 1.5; white-space: pre-wrap; word-break: break-all; max-height: 220px; overflow-y: auto; color: var(--text); }
  .tbadge { font-size: 10px; padding: 1px 6px; border-radius: 4px; font-weight: 500; }
  .tbadge-running { background: rgba(88, 166, 255, 0.2); color: var(--accent); }
  .tbadge-ok { background: rgba(63, 185, 80, 0.2); color: var(--green); }
  .tbadge-failed, .tbadge-killed { background: rgba(248, 81, 73, 0.2); color: var(--red); }
  .log-area { flex: 1; overflow-y: auto; padding: 12px 16px; font-size: 13px; line-height: 1.7; }
  .log-entry { padding: 2px 0; display: flex; gap: 10px; align-items: baseline; }
  .log-time { color: var(--text-muted); flex-shrink: 0; font-size: 12px; }
  .log-level { flex-shrink: 0; font-size: 11px; padding: 0 5px; border-radius: 3px; font-weight: 600; min-width: 48px; text-align: center; }
  .log-level.INFO { color: var(--green); background: rgba(63, 185, 80, 0.1); }
  .log-level.WARN { color: var(--yellow); background: rgba(210, 153, 34, 0.1); }
  .log-level.ERROR { color: var(--red); background: rgba(248, 81, 73, 0.1); }
  .log-level.DEBUG { color: var(--text-muted); background: rgba(139, 148, 158, 0.1); }
  .log-msg { flex: 1; word-break: break-all; white-space: pre-wrap; }
  .log-tool { flex-shrink: 0; font-size: 11px; padding: 0 6px; border-radius: 4px; background: rgba(88, 166, 255, 0.15); color: var(--accent); }
  .empty-state { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted); font-size: 13px; }
  ::-webkit-scrollbar { width: 6px; } ::-webkit-scrollbar-track { background: transparent; } ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
  /* ===== 上下文看板 ===== */
  .board-entry { border-color: rgba(88,166,255,.55); }
  .board-entry:hover { border-color: var(--accent); }
  .board-entry .tool-name { color: var(--accent); }
  .board-wrap { flex: 1; overflow-y: auto; padding: 12px 16px; font-size: 13px; }
  .bcard { background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 12px 14px; margin-bottom: 10px; cursor: pointer; }
  .bcard:hover { border-color: var(--accent); }
  .bcard .bhead { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 6px; }
  .bcard .btitle { font-weight: 600; font-size: 14px; }
  .bcard .bsub { color: var(--text-muted); font-size: 11px; }
  .bchip { font-size: 10px; padding: 1px 6px; border-radius: 4px; background: rgba(139,148,158,.15); color: var(--text-muted); }
  .bchip.plan { background: rgba(88,166,255,.15); color: var(--accent); }
  .bchip.model { background: rgba(63,185,80,.15); color: var(--green); }
  .bchip.warn { background: rgba(210,153,34,.2); color: var(--yellow); }
  .bchip.gone { background: rgba(248,81,73,.2); color: var(--red); }
  .bsection { margin: 16px 0 6px; font-weight: 600; color: var(--text-muted); }
  .ctxrow { display: flex; align-items: baseline; gap: 12px; font-size: 12px; flex-wrap: wrap; }
  .ctxbar { position: relative; height: 12px; background: var(--surface); border: 1px solid var(--border); border-radius: 6px; margin: 8px 0 3px; }
  .ctxbar .fill { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 6px 0 0 6px; opacity: .7; z-index: 1; }
  .ctxbar .tick { position: absolute; top: -3px; bottom: -3px; width: 2px; z-index: 2; }
  .ctxbar .tick.trigger { background: var(--red); }
  .ctxbar .tick.switchline { background: var(--accent); }
  .ctxbar .tick.y { background: var(--yellow); }
  .ctxbar .tick.p { background: var(--purple); }
  .ctxlegend { font-size: 10px; color: var(--text-muted); display: flex; gap: 14px; flex-wrap: wrap; }
  .btoolbar { position: sticky; top: 0; z-index: 5; background: var(--bg); display: flex; align-items: center; gap: 12px; margin-bottom: 10px; padding: 6px 0; flex-wrap: wrap; border-bottom: 1px solid var(--border); }
  .btable { width: 100%; border-collapse: collapse; font-size: 12px; margin-top: 6px; }
  .btable th { color: var(--text-muted); font-weight: 500; text-align: left; }
  .btable th, .btable td { border-bottom: 1px solid var(--border); padding: 5px 8px; white-space: nowrap; }
  .btable tbody tr:hover td { background: rgba(88,166,255,.05); }
  .curve-box { background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 10px; margin: 8px 0 12px; }
  .status-chip { font-size: 10px; padding: 1px 6px; border-radius: 4px; }
  /* 图例悬停气泡：hover 显示 data-tip 内容，向上弹出 */
  .tip { position: relative; cursor: help; }
  .tip::after {
    content: attr(data-tip);
    position: absolute; left: 0; bottom: calc(100% + 7px);
    background: #1c2128; color: var(--text);
    border: 1px solid var(--border); border-radius: 6px;
    padding: 8px 10px; font-size: 11px; line-height: 1.7;
    width: max-content; max-width: 340px; white-space: normal; text-align: left;
    box-shadow: 0 4px 12px rgba(0,0,0,.5);
    opacity: 0; visibility: hidden; transition: opacity .12s;
    z-index: 20; pointer-events: none;
  }
  .tip::before {
    content: ''; position: absolute; left: 8px; bottom: calc(100% + 2px);
    border: 5px solid transparent; border-top-color: var(--border);
    opacity: 0; visibility: hidden; transition: opacity .12s; z-index: 21; pointer-events: none;
  }
  .tip:hover::after, .tip:hover::before { opacity: 1; visibility: visible; }
  .st-completed { background: rgba(63,185,80,.2); color: var(--green); }
  .st-cancelled { background: rgba(210,153,34,.2); color: var(--yellow); }
  .st-error { background: rgba(248,81,73,.2); color: var(--red); }
</style>
</head>
<body>
<div id="banner"><span id="bannerMsg">⚠️ 连接已中断 — 正在自动重连（服务器运行中无需刷新页面）</span> <button class="btn" onclick="forceReconnect()" title="不刷新页面，立即重建事件流">⟳ 立即重连</button></div>
<div id="bye" style="display:none; position:fixed; inset:0; z-index:50; background:var(--bg); color:var(--text); flex-direction:column; align-items:center; justify-content:center; gap:12px; text-align:center; padding:24px;">
  <div style="font-size:20px; font-weight:600;">⏻ 服务器已优雅退出</div>
  <div style="font-size:14px; color:var(--text-muted);">服务已停止，此标签页可以关闭了</div>
  <div style="font-size:12px; color:var(--text-muted);">重新使用请到终端运行 cargo server</div>
</div>
<header>
  <h1>🔮 <span>血月</span> MCP 控制台</h1>
  <div class="status" id="statusBox"><div class="dot"></div><span id="connStatus">已连接 · 端口 58081</span><button class="btn" id="btnShutdown" onclick="shutdownServer()" title="等价于在终端按下 Ctrl+C：优雅退出服务器（成功后本页显示告别屏，浏览器允许时直接关闭标签页）">⏻ 关闭</button></div>
</header>
<div class="main">
  <div class="panel-left" id="leftPanel">
    <div class="tabs">
      <div class="tab active" data-tab="tools" onclick="switchTab('tools')">🧰 工具</div>
      <div class="tab" data-tab="prompts" onclick="switchTab('prompts')">💬 提示词</div>
      <div class="tab" data-tab="resources" onclick="switchTab('resources')">📦 资源</div>
    </div>
    <div class="panel-header">
      <h2 id="listTitle">工具列表 (<span id="toolCount">0</span>)</h2>
      <div style="display:flex; gap:6px;">
        <button class="btn" id="btnRescan" onclick="rescanTools()" title="扫描发现目录，登记新增的 kzm-* 插件（已有工具不受影响，无需重启服务器）">🔍 扫描新插件</button>
        <button class="btn" id="btnReload" style="display:none" onclick="reloadCurrent()">⟳ 从磁盘重载</button>
        <button class="btn" onclick="refreshCurrent()">🔄 刷新</button>
      </div>
    </div>
    <div class="tool-list" id="listArea">
      <div class="empty-state">加载中…</div>
    </div>
  </div>
  <div class="panel-right">
    <div class="tabs">
      <button class="rtab rtab-active" id="rtabLogs" onclick="switchRight('logs')">📋 日志</button>
      <button class="rtab" id="rtabTasks" onclick="switchRight('tasks')">⚡ 任务</button>
      <button class="rtab" id="rtabBoard" style="display:none" onclick="switchRight('board')">📊 看板</button>
    </div>
    <div class="panel-header">
      <h2 id="rightTitle">📋 运行日志 <span id="filterChip" style="display:none; cursor:pointer; color:var(--accent);" onclick="clearFilter()" title="点击取消筛选">[筛选中，点击取消]</span></h2>
      <button class="btn" onclick="clearLogs()">🗑️ 清空</button>
    </div>
    <div class="log-area" id="logArea">
      <div class="empty-state" id="logEmpty">等待日志…</div>
    </div>
    <div class="log-area" id="taskArea" style="display:none">
      <div class="empty-state">暂无任务</div>
    </div>
    <div class="board-wrap" id="boardArea" style="display:none">
      <div class="empty-state">正在加载看板…</div>
    </div>
  </div>
</div>
<script>
let tools = [], prompts = [], resources = [];
let currentTab = 'tools';
let logs = [];
let currentFilter = null;   // null 或工具名（需求三：单击工具卡片筛选日志，再次点击/点筛选条取消）
let es = null;                  // 日志 SSE（全局：关闭流程需要主动释放连接）
let connected = true;           // false 时徽章统一显示「已断开」（断开后的快照态）
let rightTab = 'logs';          // 右侧页签：logs | tasks
let tasksData = [];             // 任务快照记录
let taskOutputs = {};           // id → {lines: [], done: bool}（实时进展缓冲）
let taskEs = null;              // 任务事件流
let expandedGroups = {};        // 工具分组展开状态（key=分组名，跨重渲染保留）
let promptGroupsOpen = {};      // 提示词分组展开状态
let disconnectNotified = false; // 断开态守卫：EventSource 每次重连失败都会触发 onerror，只处理第一次
let byeShown = false;           // 优雅退出告别屏已显示（此后看门狗停手，别把告别屏当断线救活）

function switchTab(tab) {
  currentTab = tab;
  document.querySelectorAll('.tab').forEach(t => t.classList.toggle('active', t.dataset.tab === tab));
  document.getElementById('btnReload').style.display = (tab !== 'tools') ? '' : 'none';
  document.getElementById('btnRescan').style.display = (tab === 'tools') ? '' : 'none';
  refreshCurrent();
}

async function refreshCurrent() {
  if (currentTab === 'tools') await refreshTools();
  else if (currentTab === 'prompts') await refreshPrompts();
  else await refreshResources();
}

async function refreshTools() {
  try {
    const res = await fetch('/api/tools');
    tools = await res.json();
    renderTools();
  } catch (e) { addLogDirect('ERROR', '刷新工具列表失败: ' + e.message); }
}

function cardHtml(t) {
  return `
    <div class="tool-card ${t.enabled ? '' : 'disabled'} ${currentFilter === t.name ? 'selected' : ''}" onclick="toggleFilter('${t.name}')">
      <div class="tool-name">
        ${t.name}
        <span class="badge ${(connected && t.enabled) ? 'badge-on' : 'badge-off'}">${connected ? (t.enabled ? '运行中' : '已卸载') : '已断开'}</span>
      </div>
      <div class="tool-desc">${t.description}</div>
      <div class="tool-actions" onclick="event.stopPropagation()">
        <button class="btn btn-reload" onclick="reloadTool('${t.name}')" title="重新运行磁盘上的插件并读取最新定义（改完代码 cargo build 后点这里即生效），顺带恢复启用">⟳ 重载</button>
        ${t.enabled
          ? `<button class="btn btn-unload" onclick="unloadTool('${t.name}')" title="在内存中禁用该工具（不碰磁盘），调用将被拒绝；点「启用」可恢复">⏏ 卸载</button>`
          : `<button class="btn btn-load" onclick="loadTool('${t.name}')" title="重新启用该工具（恢复接受调用）">↩ 启用</button>`
        }
      </div>
    </div>
  `;
}

// 上下文看板工具卡：左栏置顶，单击才进入看板详情（右栏「📊 看板」页签仅在查看时存在）
function boardCardHtml() {
  const sel = rightTab === 'board' ? 'selected' : '';
  return `
    <div class="tool-card board-entry ${sel}" onclick="openBoard()" title="单击查看：ZCode 会话上下文 / 压缩 / 轮次 / 计费总览">
      <div class="tool-name">📊 上下文看板 <span class="badge badge-on" style="background:rgba(88,166,255,.2);color:var(--accent)">本机</span></div>
      <div class="tool-desc">ZCode 会话上下文 / 压缩 / 轮次 / 计费总览（只读本机引擎库；WebUI 专属，不进 MCP 协议）</div>
    </div>
  `;
}

// 按功能分组渲染（category 由各插件 decl 声明），单击组标题展开/收起
function renderTools() {
  document.getElementById('listTitle').innerHTML = '工具列表 (<span id="toolCount">' + tools.length + '</span>)';
  const el = document.getElementById('listArea');
  if (tools.length === 0) { el.innerHTML = boardCardHtml() + '<div class="empty-state">没有工具（检查 kzm-* 插件是否已构建）</div>'; return; }
  const groups = {};
  for (const t of tools) {
    const g = t.category || '未分类';
    (groups[g] = groups[g] || []).push(t);
  }
  const names = Object.keys(groups).sort((a, b) => a.localeCompare(b, 'zh'));
  el.innerHTML = boardCardHtml() + names.map(g => {
    const open = !!expandedGroups[g];
    return `
      <div class="group-header" onclick="toggleGroup('${g}')">
        <span class="arrow">${open ? '▼' : '▶'}</span> ${g}
        <span class="group-count">(${groups[g].length})</span>
      </div>
      ${open ? groups[g].map(cardHtml).join('') : ''}
    `;
  }).join('');
}

function toggleGroup(g) {
  expandedGroups[g] = !expandedGroups[g];
  renderTools();
}

async function unloadTool(name) {
  try { await fetch(`/api/tools/${name}/unload`, { method: 'POST' }); await refreshTools(); }
  catch (e) { addLogDirect('ERROR', '卸载失败: ' + e.message); }
}

async function loadTool(name) {
  try { await fetch(`/api/tools/${name}/load`, { method: 'POST' }); await refreshTools(); }
  catch (e) { addLogDirect('ERROR', '加载失败: ' + e.message); }
}

// 扫描发现目录，登记新增插件（已有工具不动；新工具无需重启服务器）
async function rescanTools() {
  try {
    const res = await fetch('/api/tools/rescan', { method: 'POST' });
    const data = await res.json();
    addLogDirect('INFO', `扫描完成：新增 ${data.count} 个插件` + (data.added.length ? `（${data.added.join(', ')}）` : ''));
    await refreshTools();
  } catch (e) { addLogDirect('ERROR', '扫描失败: ' + e.message); }
}

// 从磁盘热重载插件工具（改动代码并 cargo build 后点击即可生效，无需重启服务器）
async function reloadTool(name) {
  try {
    const res = await fetch(`/api/tools/${name}/reload`, { method: 'POST' });
    const text = await res.text();
    addLogDirect(res.ok ? 'INFO' : 'ERROR', `重载 ${name}: ${text}`);
    await refreshTools();
  } catch (e) { addLogDirect('ERROR', '重载失败: ' + e.message); }
}

async function reloadCurrent() {
  if (currentTab === 'prompts') {
    const res = await fetch('/api/prompts/reload', { method: 'POST' });
    addLogDirect('INFO', await res.text());
    await refreshPrompts();
  } else if (currentTab === 'resources') {
    const res = await fetch('/api/resources/reload', { method: 'POST' });
    addLogDirect('INFO', await res.text());
    await refreshResources();
  }
}

// ============ 提示词 / 资源（需求四：列出它们） ============

async function refreshPrompts() {
  try {
    const res = await fetch('/api/prompts');
    prompts = await res.json();
    document.getElementById('listTitle').textContent = `提示词 (${prompts.length})`;
    const groups = {};
    for (const p of prompts) {
      const g = p.category || '未分类';
      (groups[g] = groups[g] || []).push(p);
    }
    const names = Object.keys(groups).sort((a, b) => a.localeCompare(b, 'zh'));
    document.getElementById('listArea').innerHTML = names.length === 0
      ? '<div class="empty-state">无提示词（mcp_data/prompts 下放 .json / .md 文件后点「从磁盘重载」）</div>'
      : names.map(g => {
          const open = !!promptGroupsOpen[g];
          return `
            <div class="group-header" onclick="togglePromptGroup('${g}')">
              <span class="arrow">${open ? '▼' : '▶'}</span> ${g}
              <span class="group-count">(${groups[g].length})</span>
            </div>
            ${open ? groups[g].map(p => `
              <div class="entry-card">
                <div class="name">💬 ${p.title || p.name} <span style="color:var(--text-muted);font-size:11px">${p.name}</span></div>
                <div class="desc">${p.description || ''}</div>
                ${p.arguments ? `<div class="meta">参数: ${p.arguments.map(a => a.name + (a.required ? '*' : '')).join(', ')}</div>` : ''}
              </div>`).join('') : ''}
          `;
        }).join('');
  } catch (e) { addLogDirect('ERROR', '刷新提示词失败: ' + e.message); }
}

function togglePromptGroup(g) {
  promptGroupsOpen[g] = !promptGroupsOpen[g];
  refreshPrompts();
}

async function refreshResources() {
  try {
    const res = await fetch('/api/resources');
    resources = await res.json();
    document.getElementById('listTitle').textContent = `资源 (${resources.length})`;
    document.getElementById('listArea').innerHTML = resources.length === 0
      ? '<div class="empty-state">无资源（mcp_data/resources 下放 .json 文件后点「从磁盘重载」）</div>'
      : resources.map(r => `
        <div class="entry-card">
          <div class="name">📦 ${r.name}</div>
          <div class="desc">${r.description || ''}</div>
          <div class="meta">${r.uri}${r.mimeType ? ' · ' + r.mimeType : ''}</div>
        </div>`).join('');
  } catch (e) { addLogDirect('ERROR', '刷新资源失败: ' + e.message); }
}

// ============ 日志（需求三：按工具筛选） ============

function toggleFilter(name) {
  // 看板打开时点工具卡：先退出看板回日志，否则筛选发生在被遮住的日志页里，点击像没响应
  if (rightTab === 'board') switchRight('logs');
  currentFilter = (currentFilter === name) ? null : name;
  document.getElementById('filterChip').style.display = currentFilter ? '' : 'none';
  if (currentTab === 'tools') renderTools();
  rerenderLogs();
}

function clearFilter() {
  currentFilter = null;
  document.getElementById('filterChip').style.display = 'none';
  if (currentTab === 'tools') renderTools();
  rerenderLogs();
}

function addLogDirect(level, message) {
  appendEntry({ timestamp: new Date().toTimeString().slice(0, 8), level, message });
}

function appendEntry(entry) {
  logs.push(entry);
  if (logs.length > 500) logs.shift();
  if (!currentFilter || entry.tool === currentFilter) appendLogDom(entry);
}

function appendLogDom(entry) {
  const area = document.getElementById('logArea');
  document.getElementById('logEmpty')?.remove();
  const div = document.createElement('div');
  const LEVEL_ZH = { INFO: '信息', WARN: '警告', ERROR: '错误', DEBUG: '调试' };
  div.className = 'log-entry';
  div.innerHTML = `<span class="log-time">${entry.timestamp}</span><span class="log-level ${entry.level}">${LEVEL_ZH[entry.level] || entry.level}</span>${entry.tool ? `<span class="log-tool">${entry.tool}</span>` : ''}<span class="log-msg">${escapeHtml(entry.message)}</span>`;
  area.appendChild(div);
  while (area.children.length > 500) area.removeChild(area.firstChild); // DOM 条数与 logs 数组同步封顶（长挂页面防无限膨胀）
  area.scrollTop = area.scrollHeight;
}

function rerenderLogs() {
  const area = document.getElementById('logArea');
  area.innerHTML = '';
  const list = currentFilter ? logs.filter(e => e.tool === currentFilter) : logs;
  if (list.length === 0) { area.innerHTML = '<div class="empty-state" id="logEmpty">' + (currentFilter ? `无 ${currentFilter} 的日志` : '等待日志…') + '</div>'; return; }
  list.forEach(appendLogDom);
  area.scrollTop = area.scrollHeight;
}

function clearLogs() {
  logs = [];
  document.getElementById('logArea').innerHTML = '<div class="empty-state" id="logEmpty">日志已清空</div>';
}

function escapeHtml(s) {
  return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

async function loadLogs() {
  try {
    const res = await fetch('/api/logs');
    const history = await res.json();
    history.forEach(appendEntry);
    rerenderLogs();
  } catch (e) {}
}

// ============ 右侧页签：日志 / 任务 ============

function switchRight(tab) {
  rightTab = tab;
  const isBoard = tab === 'board';
  document.getElementById('rtabLogs').classList.toggle('rtab-active', tab === 'logs');
  document.getElementById('rtabTasks').classList.toggle('rtab-active', tab === 'tasks');
  document.getElementById('logArea').style.display = tab === 'logs' ? '' : 'none';
  document.getElementById('taskArea').style.display = tab === 'tasks' ? '' : 'none';
  document.getElementById('boardArea').style.display = isBoard ? '' : 'none';
  // 「看板」页签只在查看时存在：离开即收起，符合「单击左栏工具才展示详情」的交互
  document.getElementById('rtabBoard').style.display = isBoard ? '' : 'none';
  document.getElementById('rtabBoard').classList.toggle('rtab-active', isBoard);
  if (isBoard) {
    boardOpen = true;
    ensureAutoTimer();
    loadBoard();
    return;
  }
  boardOpen = false;
  boardDetailId = null;
  if (boardAutoTimer) { clearInterval(boardAutoTimer); boardAutoTimer = null; }  // 只停表；偏好本身持久化保留
  if (tab === 'tasks') {
    fetchTasks();
    if (!taskEs || taskEs.readyState === 2) openTaskStream();
  } else if (taskEs) {
    taskEs.close(); taskEs = null;
  }
}

async function fetchTasks() {
  try {
    const res = await fetch('/api/tasks');
    tasksData = await res.json();
    tasksData.forEach(r => { if (!(r.id in taskOutputs)) taskOutputs[r.id] = { lines: [], done: r.status !== 'running' }; });
    renderTasks();
  } catch (e) { addLogDirect('ERROR', '刷新任务失败: ' + e.message); }
}

function openTaskStream() {
  taskEs = new EventSource('/api/tasks/stream');
  taskEs.onmessage = (e) => {
    try {
      const d = JSON.parse(e.data);
      if (d.type === 'start') {
        if (!(d.id in taskOutputs)) {
          tasksData.unshift({ id: d.id, tool: d.tool, args: d.args, status: 'running', startedAt: '' });
          taskOutputs[d.id] = { lines: [], done: false };
          if (rightTab === 'tasks') renderTasks();
        }
      } else if (d.type === 'line') {
        const buf = taskOutputs[d.id] || (taskOutputs[d.id] = { lines: [], done: false });
        buf.lines.push((d.stream === 'stderr' ? '' : '') + d.text);
        if (buf.lines.length > 300) buf.lines.shift();
        const pre = document.getElementById('tout-' + d.id);
        if (pre) { pre.textContent = buf.lines.join('\n'); pre.scrollTop = pre.scrollHeight; }
      } else if (d.type === 'exit') {
        const rec = tasksData.find(x => x.id === d.id);
        if (rec) rec.status = d.ok ? 'ok' : (d.note === '已终止' ? 'killed' : 'failed');
        const buf = taskOutputs[d.id]; if (buf) buf.done = true;
        if (rightTab === 'tasks') renderTasks();
      }
    } catch {}
  };
  taskEs.onerror = () => { showDisconnected(); };
  taskEs.onopen = markConnected;   // 任务流单独复活时也要能清横幅（否则只有日志流重开会清）
}

function renderTasks() {
  const el = document.getElementById('taskArea');
  if (tasksData.length === 0) { el.innerHTML = '<div class="empty-state">暂无任务</div>'; return; }
  const BADGE = { running: ['tbadge-running', '运行中'], ok: ['tbadge-ok', '已完成'], failed: ['tbadge-failed', '失败'], killed: ['tbadge-killed', '已终止'] };
  el.innerHTML = tasksData.slice(0, 30).map(r => {
    const [bcls, btxt] = BADGE[r.status] || ['tbadge-running', r.status];
    const lines = (taskOutputs[r.id] || {}).lines || [];
    return `
      <div class="task-card">
        <div class="thead">⚡ ${r.tool} <span class="tbadge ${bcls}">${btxt}</span> <span style="color:var(--text-muted);font-weight:400;font-size:11px">${r.startedAt}</span></div>
        <div class="targs">${escapeHtml(r.args || '')}</div>
        ${lines.length ? `<pre class="tout" id="tout-${r.id}">${escapeHtml(lines.join('\n'))}</pre>` : '<div class="targs" style="color:var(--text-muted)">（暂无进展输出）</div>'}
      </div>`;
  }).join('');
}

// ============ 连接状态（需求一：断开横幅） ============

// 优雅关闭服务器（等价于终端 Ctrl+C）：响应返回后日志流会断开，
// bye 告别屏接管本页（bye 屏下看门狗与自动重连都停手；重启服务器后刷新本页重新进入）
async function shutdownServer() {
  if (!confirm('确定要优雅关闭服务器吗？（等价于终端 Ctrl+C，成功后本页显示告别屏）')) return;
  document.getElementById('btnShutdown').disabled = true;
  document.getElementById('connStatus').textContent = '正在退出…';
  try {
    await fetch('/api/shutdown', { method: 'POST' });
  } catch (e) { /* 关闭进行中连接中断，属预期 */ }
  // 主动释放本页的日志 SSE：服务器排水不再等长连接，可立即完成优雅退出
  if (es) { es.close(); es = null; }
  // 浏览器禁止关闭用户自开的标签页——window.close() 尽力而为（仅脚本打开的标签有效），
  // 否则以全页告别屏接管（复刻 dsh-graceful-exit），死服务器不会显示成破损的重连界面
  try { window.close(); } catch (e) {}
  showBye();
}

function showBye() {
  connected = false;
  byeShown = true;   // 告别屏是有意退出的终态：看门狗别探活、连接别自动「救活」
  document.getElementById('bye').style.display = 'flex';   // 全页接管
}

// 断开统一处理（幂等）：黄横幅「自动重连中」+ 日志提示 + 徽章翻「已断开」+ 左栏转快照态。
// onerror 每次断线/重连失败都会触发本函数——守卫保证只生效一次
function showDisconnected() {
  if (disconnectNotified) return;
  disconnectNotified = true;
  connected = false;
  if (currentTab === 'tools') renderTools();
  addLogDirect('INFO', '🔌 连接中断，自动重连中…');
  document.getElementById('leftPanel').classList.add('stale');
  const banner = document.getElementById('banner');
  banner.className = 'reconnecting';
  document.getElementById('bannerMsg').textContent = '⚠️ 连接已中断 — 正在自动重连（服务器运行中无需刷新页面）';
  banner.style.display = 'block';
  document.getElementById('statusBox').classList.add('disconnected');
  document.getElementById('connStatus').textContent = '已断开';
}

// 看门狗状态（探活进行中 / 连续失败计数 / 上次重建时刻——刚重建的流要给 15 秒自证）
let probeInFlight = false, probeFails = 0, lastRebuildAt = 0;

// 恢复统一处理（onopen，幂等）：清横幅 + 徽章翻回「已连接」；断开过才补刷新与日志
function markConnected() {
  const wasDown = disconnectNotified;
  disconnectNotified = false;
  connected = true;
  probeFails = 0;
  document.getElementById('banner').style.display = 'none';
  document.getElementById('statusBox').classList.remove('disconnected');
  document.getElementById('leftPanel').classList.remove('stale');
  document.getElementById('btnShutdown').disabled = false;
  document.getElementById('connStatus').textContent = '已连接 · 端口 58081';
  if (wasDown) {
    addLogDirect('INFO', '✅ 连接已恢复');
    refreshTools();               // 断开期间工具徽章曾翻「已断开」
    if (boardOpen) loadBoard();   // 看板开着时立即补一帧（自动刷新在断开期间停摆）
  }
}

// 不刷新页面、整体重建事件流（横幅「立即重连」按钮 / 看门狗共用）
function forceReconnect() {
  connectLogStream();
  if (taskEs) { taskEs.close(); taskEs = null; openTaskStream(); }   // 任务页签开着时同步重建
}

// 断连看门狗：横幅挂起期间每 5 秒探一次服务器是否还活着。
// 恢复不再只押浏览器 EventSource 自动重连——Firefox 后台标签会无限推迟重连请求，
// 「页面挂半天后横幅常驻、一刷新就好」正是这么来的。fetch 成功 = 服务器活着 →
// 整体重建事件流（onopen 清横幅）；失败 = 服务器真退出了 → 横幅升级红色「服务器无响应」，
// 继续探测，服务器回来后照旧自动恢复，全程无需刷新页面。
async function probeAndRecover() {
  if (connected || byeShown || probeInFlight) return;
  if (Date.now() - lastRebuildAt < 15000) return;   // 刚重建过，给新流时间自证
  probeInFlight = true;
  try {
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), 4000);
    const res = await fetch('/api/tasks', { cache: 'no-store', signal: ctl.signal });
    clearTimeout(timer);
    if (!res.ok) throw new Error('HTTP ' + res.status);
    lastRebuildAt = Date.now();
    forceReconnect();
  } catch (e) {
    if (++probeFails >= 2) {
      const banner = document.getElementById('banner');
      banner.className = 'down';
      document.getElementById('bannerMsg').textContent = '⛔ 服务器无响应（进程可能已退出）— 请到终端重启服务器；恢复后本页会自动重连';
    }
  } finally {
    probeInFlight = false;
  }
}
setInterval(probeAndRecover, 5000);
// 回到前台立即探一次：后台标签的 setInterval 被节流到分钟级，切回来时不等下一个整点
document.addEventListener('visibilitychange', () => { if (!document.hidden) probeAndRecover(); });

function connectLogStream() {
  if (es) es.close();   // 重建时先关旧流，防止新旧两条并存
  es = new EventSource('/api/logs/stream');
  es.addEventListener('log', (e) => {
    try { appendEntry(JSON.parse(e.data)); } catch {}
  });
  es.onopen = markConnected;
  es.onerror = () => {
    // 连接断开 → 横幅 + 徽章「已断开」；之后的恢复交给看门狗 probeAndRecover
    showDisconnected();
  };
}

// ============ 上下文看板（左栏「上下文看板」工具卡进入） ============

let boardOpen = false;
let boardAutoTimer = null;
let boardAutoOn = true;         // 自动刷新偏好（持久化存储；缺省=开，09-28 拍板默认勾选）
let boardDetailId = null;       // 非空 = 二级下钻中
let boardData = [];             // 最近一次会话列表（折叠切换重渲染时免重新拉取）
let boardSectionsOpen = {};     // 看板分栏展开状态（key→bool，undefined=展开；跨重渲染保留）
const BOARD_SWITCH_RATIO = 0.9; // 压缩预警线 = 压缩触发线 × 0.9（防引擎自动压缩重写历史的最后通牒）；换会话建议线 = 换会话线 40万

function fmtTok(n) {
  if (n === null || n === undefined) return '—';
  return (n / 10000).toFixed(1) + '万';
}
function fmtTime(ms) {
  if (!ms) return '—';
  const d = new Date(ms);
  const p = (x) => String(x).padStart(2, '0');
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
function fmtDur(ms) {
  if (ms === null || ms === undefined) return '—';
  if (ms < 1000) return ms + 'ms';
  const s = ms / 1000;
  return s < 60 ? s.toFixed(1) + 's' : Math.floor(s / 60) + 'm' + Math.round(s % 60) + 's';
}
function fmtPts(n) { return (n || 0).toFixed(2); }
function planName(p) {
  if (!p) return '—';
  if (p.includes('individual')) return '个人池';
  if (p.includes('team')) return '团队池';
  if (p.includes('start-plan')) return 'start池';
  return p;
}
function zoneColor(ctx) {
  if (ctx === null || ctx === undefined) return 'var(--text-muted)';
  if (ctx > 400000) return 'var(--purple)';
  if (ctx > 200000) return 'var(--yellow)';
  return 'var(--green)';
}

function openBoard() { switchRight('board'); }

async function loadBoard() {
  if (boardDetailId) { await openSession(boardDetailId, true); return; }
  const area = document.getElementById('boardArea');
  try {
    const res = await fetch('/api/ctx/sessions');
    if (!res.ok) throw new Error(await res.text());
    boardData = await res.json();
    renderBoardList(boardData);
  } catch (e) {
    area.innerHTML = '<div class="empty-state">看板加载失败: ' + escapeHtml(e.message) + '</div>';
  }
}

function sessionCard(s) {
  const ctx = s.context_tokens;
  const trig = s.trigger_tokens;
  const win = s.context_window || 1000000;
  const swLine = trig ? Math.round(trig * BOARD_SWITCH_RATIO) : null;
  const p20 = (200000 / win * 100).toFixed(2);
  const p40 = (400000 / win * 100).toFixed(2);
  const swPct = swLine ? (swLine / win * 100).toFixed(2) : 0;
  const trPct = trig ? (trig / win * 100).toFixed(2) : 0;
  const ctxTxt = (ctx === null || ctx === undefined)
    ? '<span style="color:var(--text-muted)">无请求数据</span>'
    : `<b style="color:${zoneColor(ctx)}">${fmtTok(ctx)}</b> / 触发线 ${fmtTok(trig)}（${trig ? (ctx / trig * 100).toFixed(1) : '—'}%）`;
  const fillW = ctx ? Math.min(ctx / win * 100, 100) : 0;
  // time_compacting 是瞬态列（压缩中置位、完成即 NULL），「曾压缩」看 compaction_count（part 表 boundaryId 口径）
  const compactChip = s.time_compacting_ms
    ? '<span class="bchip warn">压缩中…</span>'
    : s.compaction_count > 0
      ? `<span class="bchip warn">已压缩 ${s.compaction_count} 次</span>`
      : '<span class="bchip">未压缩</span>';
  return `
  <div class="bcard" onclick="openSession('${s.session_id}')">
    <div class="bhead">
      <span class="btitle">${escapeHtml(s.title || s.session_id)}</span>
      <span class="bchip model">${escapeHtml(s.model || '—')}</span>
      <span class="bchip plan">${planName(s.provider_id)}</span>
      ${compactChip}
      ${s.deleted ? '<span class="bchip gone">已删除</span>' : s.archived ? '<span class="bchip warn">已归档</span>' : ''}
      ${!s.deleted && !s.archived && s.subagent ? '<span class="bchip">子代理</span>' : ''}
      ${!s.deleted && !s.archived && s.task_type === 'fork' ? '<span class="bchip">fork</span>' : ''}
      ${!s.deleted && !s.archived && s.task_type === 'selection_side_chat' ? '<span class="bchip">侧聊</span>' : ''}
      ${!s.deleted && !s.archived && !s.in_client_list ? '<span class="bchip warn" title="引擎库里有这个会话，但壳侧会话索引（客户端列表的数据源）没有它的条目——侧聊等会话客户端列表从不显示，仅引擎侧存在">列表外</span>' : ''}
    </div>
    <div class="ctxrow">
      ${ctxTxt}
      <span>积分估算 <b>${fmtPts(s.points_estimate)}</b></span>
      <span>缓存命中 <b>${s.cache_hit_rate === null || s.cache_hit_rate === undefined ? '—' : (s.cache_hit_rate * 100).toFixed(1) + '%'}</b></span>
      <span>${s.request_count} 次请求</span>
    </div>
    ${(ctx !== null && trig) ? `
    <div class="ctxbar" style="background:linear-gradient(to right, rgba(63,185,80,.10) 0 ${p20}%, rgba(210,153,34,.12) ${p20}% ${p40}%, rgba(163,113,247,.10) ${p40}% ${swPct}%, rgba(88,166,255,.14) ${swPct}% ${trPct}%, rgba(248,81,73,.10) ${trPct}% 100%), var(--surface)">
      <div class="fill" style="width:${fillW}%;background:${zoneColor(ctx)}"></div>
      <div class="tick y" style="left:${p20}%" title="涣散线 20万（绿黄交界）"></div>
      <div class="tick p" style="left:${p40}%" title="换会话线 40万（黄紫交界）"></div>
      <div class="tick switchline" style="left:${swPct}%" title="压缩预警线 ${fmtTok(swLine)}"></div>
      <div class="tick trigger" style="left:${trPct}%" title="压缩触发线 ${fmtTok(trig)}"></div>
    </div>
    <div class="ctxlegend">
      <span class="tip" data-tip="模型上下文窗口总容量。四根线与填充色的百分比都以它为分母。">窗口 ${fmtTok(win)}</span>
      <span class="tip" style="color:var(--yellow)" data-tip="质量带边界：进入 20–40 万涣散带（黄区）的起点。此后模型注意力开始下滑、长程检索与多步推理变弱。外推经验值，非官方标准。">▎涣散线 20万</span>
      <span class="tip" style="color:var(--purple)" data-tip="换会话建议点：40 万起为高危区（紫区）。按换会话纪律，应在此线附近主动开新会话，避免在涣散状态下继续堆积上下文。">▎换会话线 40万</span>
      <span class="tip" style="color:var(--accent)" data-tip="压缩触发线 × 0.9 的最后通牒：越过压缩触发线后引擎将自动压缩并重写上下文、历史保真度下降，此线用于在被动压缩前主动决断。">▎压缩预警线 ${fmtTok(swLine)}</span>
      <span class="tip" style="color:var(--red)" data-tip="引擎动手点：上下文到达此值即触发自动压缩，压缩前的细节被重写，增长曲线此后不再连续。">▎压缩触发线 ${fmtTok(trig)}</span>
    </div>` : ''}
    <div class="bsub" style="margin-top:6px">最近活动 ${fmtTime(s.time_updated_ms)} · 累计输入 ${fmtTok(s.total_input_tokens)} / 输出 ${fmtTok(s.total_output_tokens)} · sess ${escapeHtml(s.session_id.slice(5, 13))}</div>
  </div>`;
}

// 分栏折叠状态持久化（localStorage，同浏览器跨会话记忆；存储不可用时静默降级为页面内记忆）
const BOARD_SECTION_KEY = 'kzm_board_sections';
function loadBoardSections() {
  try { boardSectionsOpen = JSON.parse(localStorage.getItem(BOARD_SECTION_KEY)) || {}; } catch (e) { boardSectionsOpen = {}; }
}
function saveBoardSections() {
  try { localStorage.setItem(BOARD_SECTION_KEY, JSON.stringify(boardSectionsOpen)); } catch (e) {}
}

// 自动刷新偏好持久化（'1'/'0'；缺省=开，09-28 拍板默认勾选）。定时器跟随「看板打开 && 偏好开」：
// 离开看板只停表，不清偏好——下次进来按记忆恢复
const BOARD_AUTO_KEY = 'kzm_board_autorefresh';
function loadBoardAuto() {
  try {
    const v = localStorage.getItem(BOARD_AUTO_KEY);
    boardAutoOn = v === null ? true : v === '1';
  } catch (e) { boardAutoOn = true; }
}
function saveBoardAuto() {
  try { localStorage.setItem(BOARD_AUTO_KEY, boardAutoOn ? '1' : '0'); } catch (e) {}
}
function ensureAutoTimer() {
  if (boardAutoOn && boardOpen && !boardAutoTimer) {
    boardAutoTimer = setInterval(() => { if (boardOpen && connected) loadBoard(); }, 30000);
  } else if ((!boardAutoOn || !boardOpen) && boardAutoTimer) {
    clearInterval(boardAutoTimer); boardAutoTimer = null;
  }
}

// 分栏可折叠标题（复用工具分组的 ▼/▶ 交互；undefined=展开，跨重渲染保留）
function toggleSection(key) {
  boardSectionsOpen[key] = (boardSectionsOpen[key] === false);
  saveBoardSections();
  renderBoardList(boardData);   // 翻完状态必须重渲染，否则 DOM 不动（09-28 验收抓到的漏渲染 bug）
}

function sectionHtml(key, label, cards, defaultOpen = true) {
  if (!cards.length) return '';
  const open = boardSectionsOpen[key] ?? defaultOpen;
  return `
    <div class="group-header" onclick="toggleSection('${key}')">
      <span class="arrow">${open ? '▼' : '▶'}</span> ${label}
      <span class="group-count">(${cards.length})</span>
    </div>
    ${open ? cards.map(sessionCard).join('') : ''}
  `;
}

function renderBoardList(sessions) {
  // 五栏分流：活跃主会话 / 列表外 / 子代理 / 已归档 / 已删除（壳侧 archived+deleted 软标记，各栏可折叠）
  const goneDel = sessions.filter(s => s.deleted);
  const goneArch = sessions.filter(s => s.archived && !s.deleted);
  const live = sessions.filter(s => !s.archived && !s.deleted);
  const mainAll = live.filter(s => !s.subagent);
  // 列表外 = 引擎里有、壳侧索引没条目（划词侧聊等）——客户端列表永不显示，也永远无法从客户端归档/删除，
  // 只会随使用缓慢堆积，单独成栏默认收起，不挤占主会话栏
  const outside = mainAll.filter(s => !s.in_client_list);
  const main = mainAll.filter(s => s.in_client_list);
  const subs = live.filter(s => s.subagent);
  const goneTxt = `${outside.length ? ` + ${outside.length} 列表外` : ''}${goneArch.length ? ` + ${goneArch.length} 已归档` : ''}${goneDel.length ? ` + ${goneDel.length} 已删除` : ''}`;
  document.getElementById('boardArea').innerHTML = `
    <div class="btoolbar">
      <span style="font-weight:600">会话上下文总览（本机 ZCode · ${main.length} 主 + ${subs.length} 子代理${goneTxt}）</span>
      <label style="font-size:12px;color:var(--text-muted);cursor:pointer"><input type="checkbox" id="boardAuto" ${boardAutoOn ? 'checked' : ''} onchange="toggleBoardAuto()"> 自动刷新 30s</label>
      <button class="btn" onclick="loadBoard()">🔄 刷新</button>
      <span style="font-size:11px;color:var(--text-muted)">积分 = 官方系数 × 时段乘数的本地估算，非权威账单；配额余量在服务端（v2 接入）</span>
    </div>
    ${sectionHtml('main', '活跃主会话', main) || '<div class="empty-state">无会话</div>'}
    ${sectionHtml('outside', '列表外会话（引擎在、客户端列表不显示：划词侧聊等）', outside, false)}
    ${sectionHtml('subs', '子代理会话', subs)}
    ${sectionHtml('archived', '已归档（客户端列表已移除，本地副本与流水仍在）', goneArch)}
    ${sectionHtml('deleted', '已删除（客户端列表已移除，本地副本与流水仍在）', goneDel)}
  `;
}

function toggleBoardAuto() {
  boardAutoOn = document.getElementById('boardAuto').checked;
  saveBoardAuto();
  ensureAutoTimer();
  if (boardAutoOn) loadBoard();
}

async function openSession(id, keepScroll) {
  boardDetailId = id;
  const area = document.getElementById('boardArea');
  const scroll = keepScroll ? area.scrollTop : 0;
  if (!keepScroll) area.innerHTML = '<div class="empty-state">加载会话详情…</div>';
  try {
    const res = await fetch('/api/ctx/sessions/' + id);
    if (!res.ok) throw new Error(await res.text());
    renderDetail(await res.json());
    if (keepScroll) area.scrollTop = scroll;
  } catch (e) {
    area.innerHTML = '<div class="empty-state">详情加载失败: ' + escapeHtml(e.message) + '</div><div style="text-align:center"><button class="btn" onclick="boardBack()">← 返回总览</button></div>';
  }
}

function boardBack() { boardDetailId = null; loadBoard(); }

function renderDetail(d) {
  const area = document.getElementById('boardArea');
  const win = d.context_window || 1000000;
  const trig = d.trigger_tokens;
  const swLine = trig ? Math.round(trig * BOARD_SWITCH_RATIO) : null;
  const compactN = (d.compactions || []).length;
  const compactChip = d.time_compacting_ms
    ? `<span class="bchip warn">压缩中…（自 ${fmtTime(d.time_compacting_ms)}）</span>`
    : compactN > 0
      ? `<span class="bchip warn">已压缩 ${compactN} 次</span>`
      : '<span class="bchip">未压缩</span>';

  // 上下文增长曲线（SVG，无外部依赖）：主会话折线 + 子代理散点 + 三参考线 + 压缩竖线
  let curveSvg = '<div class="bsub">无请求数据，无法画上下文曲线</div>';
  if ((d.curve || []).length) {
    const W = 900, H = 230, PL = 74, PR = 16, PT = 14, PB = 24;
    const pts = d.curve;
    const t0 = pts[0].t_ms, t1 = Math.max(pts[pts.length - 1].t_ms, t0 + 1);
    const yTop = Math.max(trig || 0, ...pts.map(p => p.input_tokens), 1);
    const x = t => PL + (t - t0) / (t1 - t0) * (W - PL - PR);
    const y = v => PT + (1 - v / yTop) * (H - PT - PB);
    const mainLine = pts.filter(p => p.agent === 'zcode-agent')
      .map(p => `${x(p.t_ms).toFixed(1)},${y(p.input_tokens).toFixed(1)}`).join(' ');
    const subDots = pts.filter(p => p.agent !== 'zcode-agent')
      .map(p => `<circle cx="${x(p.t_ms).toFixed(1)}" cy="${y(p.input_tokens).toFixed(1)}" r="2.5" fill="#8b949e"><title>子代理请求 ${fmtTok(p.input_tokens)} @ ${fmtTime(p.t_ms)}</title></circle>`).join('');
    const hline = (v, color, label) => v ? `<line x1="${PL}" x2="${W - PR}" y1="${y(v).toFixed(1)}" y2="${y(v).toFixed(1)}" stroke="${color}" stroke-dasharray="4 4" stroke-width="1"/><text x="${PL + 4}" y="${(y(v) - 3).toFixed(1)}" fill="${color}" font-size="10">${label}</text>` : '';
    const vlines = (d.compactions || []).filter(c => c.t_ms >= t0 && c.t_ms <= t1)
      .map(c => `<line x1="${x(c.t_ms).toFixed(1)}" x2="${x(c.t_ms).toFixed(1)}" y1="${PT}" y2="${H - PB}" stroke="#f85149" stroke-width="1.5"/><text x="${(x(c.t_ms) + 3).toFixed(1)}" y="${PT + 10}" fill="#f85149" font-size="10">压缩</text>`).join('');
    curveSvg = `
    <div class="curve-box">
      <svg viewBox="0 0 ${W} ${H}" style="width:100%;display:block">
        ${hline(trig, '#f85149', '压缩触发线 ' + fmtTok(trig))}
        ${hline(400000, '#a371f7', '换会话线 40万')}
        ${hline(200000, '#d29922', '涣散线 20万')}
        ${hline(swLine, '#58a6ff', '压缩预警线 ' + fmtTok(swLine))}
        ${vlines}
        ${mainLine ? `<polyline points="${mainLine}" fill="none" stroke="#3fb950" stroke-width="2"/>` : ''}
        ${subDots}
        <text x="${PL}" y="${H - 6}" fill="#8b949e" font-size="10">${fmtTime(t0)}</text>
        <text x="${W - PR}" y="${H - 6}" fill="#8b949e" font-size="10" text-anchor="end">${fmtTime(t1)}</text>
      </svg>
    </div>`;
  }

  const turns = (d.turns || []).map(t => {
    const st = t.cancelled_by_user ? '<span class="status-chip st-cancelled">已取消</span>'
      : t.context_exceeded ? '<span class="status-chip st-error">超上下文</span>'
      : t.status === 'completed' ? '<span class="status-chip st-completed">完成</span>'
      : `<span class="status-chip ${t.status ? 'st-error' : ''}">${escapeHtml(t.status || '—')}${t.error_type ? '·' + escapeHtml(t.error_type) : ''}</span>`;
    return `<tr>
      <td>${fmtTime(t.started_at_ms)}</td>
      <td>${fmtDur(t.duration_ms)}</td>
      <td>${t.ttft_ms === null || t.ttft_ms === undefined ? '—' : t.ttft_ms + 'ms'}</td>
      <td>${t.model_request_count}</td>
      <td>${t.tool_call_count}${t.tool_error_count ? `<span style="color:var(--red)">（${t.tool_error_count} 错）</span>` : ''}</td>
      <td>${fmtTok(t.input_tokens)}</td>
      <td>${fmtTok(t.output_tokens)}</td>
      <td>${fmtPts(t.points_estimate)}</td>
      <td>${st}</td>
    </tr>`;
  }).join('');

  area.innerHTML = `
    <div class="btoolbar">
      <button class="btn" onclick="boardBack()">← 返回总览</button>
      <span style="font-weight:600">${escapeHtml(d.session_id)}</span>
      <span class="bchip model">${escapeHtml(d.model || '—')}</span>
      ${compactChip}
      <span style="font-size:11px;color:var(--text-muted)">压缩事件（part 表权威口径，唯一 boundaryId 计数）：${compactN} 次</span>
      <button class="btn" onclick="openSession('${d.session_id}')">🔄 刷新</button>
    </div>
    <div style="font-weight:600;margin-bottom:2px">上下文增长曲线（每请求的完整上下文，含缓存命中；绿线 = 主会话，灰点 = 子代理；骤降≠压缩——旧大工具结果会被引擎逐出上下文，红竖线才是真实压缩边界）</div>
    ${curveSvg}
    <div style="font-weight:600;margin:10px 0 2px">轮次明细（${(d.turns || []).length} 轮 · 输入为计费口径 = 回合内各请求累计）</div>
    <table class="btable">
      <thead><tr><th>开始</th><th>耗时</th><th>首token</th><th>请求数</th><th>工具</th><th>输入(计费)</th><th>输出</th><th>积分</th><th>状态</th></tr></thead>
      <tbody>${turns || '<tr><td colspan="9" style="color:var(--text-muted)">无轮次记录</td></tr>'}</tbody>
    </table>
    ${d.rollout_file ? `<div class="bsub" style="margin-top:8px">流水：${escapeHtml(d.rollout_file)}</div>` : ''}
  `;
}

refreshTools();
loadLogs();
connectLogStream();
loadBoardSections();
loadBoardAuto();
// 深链：/?board=1 直接进入看板（书签直达 / 自动化自检用）
if (new URLSearchParams(location.search).has('board')) openBoard();
</script>
</body>
</html>"##
}
