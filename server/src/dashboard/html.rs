/// Dashboard HTML — 内嵌的完整 Web 管理界面
///
/// 功能：工具卡片（卸载/重载/启用，单击卡片筛选日志）、资源与提示词页签、
/// 服务器断连横幅（页面挂着的日志 SSE 断开时显示）。

pub fn dashboard_html() -> &'static str {
    r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>MCP Dashboard — 风见血月</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  :root {
    --bg: #0d1117; --surface: #161b22; --border: #30363d;
    --text: #e6edf3; --text-muted: #8b949e; --accent: #58a6ff;
    --green: #3fb950; --red: #f85149; --yellow: #d29922;
  }
  body { font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace; background: var(--bg); color: var(--text); height: 100vh; display: flex; flex-direction: column; }
  #banner { display: none; background: var(--red); color: #fff; text-align: center; padding: 8px 16px; font-size: 13px; font-weight: 600; }
  header { background: var(--surface); border-bottom: 1px solid var(--border); padding: 12px 20px; display: flex; align-items: center; justify-content: space-between; }
  header h1 { font-size: 16px; font-weight: 600; }
  header h1 span { color: var(--red); }
  .status { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-muted); }
  .status .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--green); animation: pulse 2s infinite; }
  .status.disconnected .dot { background: var(--red); animation: none; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
  .main { display: flex; flex: 1; overflow: hidden; }
  .panel-left { width: 380px; min-width: 300px; background: var(--surface); border-right: 1px solid var(--border); display: flex; flex-direction: column; }
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
  .log-area { flex: 1; overflow-y: auto; padding: 12px 16px; font-size: 13px; line-height: 1.7; }
  .log-entry { padding: 2px 0; display: flex; gap: 10px; align-items: baseline; }
  .log-time { color: var(--text-muted); flex-shrink: 0; font-size: 12px; }
  .log-level { flex-shrink: 0; font-size: 11px; padding: 0 5px; border-radius: 3px; font-weight: 600; min-width: 48px; text-align: center; }
  .log-level.INFO { color: var(--green); background: rgba(63, 185, 80, 0.1); }
  .log-level.WARN { color: var(--yellow); background: rgba(210, 153, 34, 0.1); }
  .log-level.ERROR { color: var(--red); background: rgba(248, 81, 73, 0.1); }
  .log-level.DEBUG { color: var(--text-muted); background: rgba(139, 148, 158, 0.1); }
  .log-msg { flex: 1; word-break: break-all; }
  .log-tool { flex-shrink: 0; font-size: 11px; padding: 0 6px; border-radius: 4px; background: rgba(88, 166, 255, 0.15); color: var(--accent); }
  .empty-state { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted); font-size: 13px; }
  ::-webkit-scrollbar { width: 6px; } ::-webkit-scrollbar-track { background: transparent; } ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
</style>
</head>
<body>
<div id="banner">⛔ 服务器已断开 — 请重新启动服务器后刷新页面</div>
<header>
  <h1>🔮 <span>血月</span> MCP Dashboard</h1>
  <div class="status" id="statusBox"><div class="dot"></div><span id="connStatus">Connected · Port 58081</span><button class="btn" id="btnShutdown" onclick="shutdownServer()" title="等价于在终端按下 Ctrl+C：优雅退出服务器（本标签页保持打开，显示断开横幅）">⏻ 关闭</button></div>
</header>
<div class="main">
  <div class="panel-left">
    <div class="tabs">
      <div class="tab active" data-tab="tools" onclick="switchTab('tools')">🧰 工具</div>
      <div class="tab" data-tab="prompts" onclick="switchTab('prompts')">💬 提示词</div>
      <div class="tab" data-tab="resources" onclick="switchTab('resources')">📦 资源</div>
    </div>
    <div class="panel-header">
      <h2 id="listTitle">工具列表 (<span id="toolCount">0</span>)</h2>
      <div style="display:flex; gap:6px;">
        <button class="btn" id="btnRescan" onclick="rescanTools()">🔍 扫描新插件</button>
        <button class="btn" id="btnReload" style="display:none" onclick="reloadCurrent()">⟳ 从磁盘重载</button>
        <button class="btn" onclick="refreshCurrent()">🔄 刷新</button>
      </div>
    </div>
    <div class="tool-list" id="listArea">
      <div class="empty-state">加载中…</div>
    </div>
  </div>
  <div class="panel-right">
    <div class="panel-header">
      <h2>📋 运行日志 <span id="filterChip" style="display:none; cursor:pointer; color:var(--accent);" onclick="clearFilter()" title="点击取消筛选">[筛选中，点击取消]</span></h2>
      <button class="btn" onclick="clearLogs()">🗑️ 清空</button>
    </div>
    <div class="log-area" id="logArea">
      <div class="empty-state" id="logEmpty">等待日志…</div>
    </div>
  </div>
</div>
<script>
let tools = [], prompts = [], resources = [];
let currentTab = 'tools';
let logs = [];
let currentFilter = null;   // null 或工具名（需求三：单击工具卡片筛选日志，再次点击/点筛选条取消）

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

function renderTools() {
  document.getElementById('listTitle').innerHTML = '工具列表 (<span id="toolCount">' + tools.length + '</span>)';
  const el = document.getElementById('listArea');
  if (tools.length === 0) { el.innerHTML = '<div class="empty-state">没有工具（检查 kzm-* 插件是否已构建）</div>'; return; }
  el.innerHTML = tools.map(t => `
    <div class="tool-card ${t.enabled ? '' : 'disabled'} ${currentFilter === t.name ? 'selected' : ''}" onclick="toggleFilter('${t.name}')">
      <div class="tool-name">
        ${t.name}
        <span class="badge ${t.enabled ? 'badge-on' : 'badge-off'}">${t.enabled ? '运行中' : '已卸载'}</span>
      </div>
      <div class="tool-desc">${t.description}</div>
      <div class="tool-actions" onclick="event.stopPropagation()">
        <button class="btn btn-reload" onclick="reloadTool('${t.name}')">⟳ 重载</button>
        ${t.enabled
          ? `<button class="btn btn-unload" onclick="unloadTool('${t.name}')">⏏ 卸载</button>`
          : `<button class="btn btn-load" onclick="loadTool('${t.name}')">↩ 启用</button>`
        }
      </div>
    </div>
  `).join('');
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
    document.getElementById('listArea').innerHTML = prompts.length === 0
      ? '<div class="empty-state">无提示词（mcp_data/prompts 下放 .json / .md 文件后点「从磁盘重载」）</div>'
      : prompts.map(p => `
        <div class="entry-card">
          <div class="name">💬 ${p.name}</div>
          <div class="desc">${p.description || ''}</div>
          ${p.arguments ? `<div class="meta">参数: ${p.arguments.map(a => a.name + (a.required ? '*' : '')).join(', ')}</div>` : ''}
        </div>`).join('');
  } catch (e) { addLogDirect('ERROR', '刷新提示词失败: ' + e.message); }
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
  div.className = 'log-entry';
  div.innerHTML = `<span class="log-time">${entry.timestamp}</span><span class="log-level ${entry.level}">${entry.level}</span>${entry.tool ? `<span class="log-tool">${entry.tool}</span>` : ''}<span class="log-msg">${escapeHtml(entry.message)}</span>`;
  area.appendChild(div);
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

// ============ 连接状态（需求一：断开横幅） ============

// 优雅关闭服务器（等价于终端 Ctrl+C）：响应返回后日志流会断开，
// onerror 自动显示「服务器已断开」横幅；重启服务器后 EventSource 自动重连恢复
async function shutdownServer() {
  if (!confirm('确定要优雅关闭服务器吗？（等价于终端 Ctrl+C，本页保持打开）')) return;
  document.getElementById('btnShutdown').disabled = true;
  document.getElementById('connStatus').textContent = 'Shutting down…';
  try {
    await fetch('/api/shutdown', { method: 'POST' });
    addLogDirect('WARN', '已发送关闭请求，服务器正在优雅退出…');
  } catch (e) {
    addLogDirect('WARN', '连接已中断（关闭进行中，属预期）');
  }
}

function connectLogStream() {
  const es = new EventSource('/api/logs/stream');
  es.addEventListener('log', (e) => {
    try { appendEntry(JSON.parse(e.data)); } catch {}
  });
  es.onopen = () => {
    document.getElementById('banner').style.display = 'none';
    document.getElementById('statusBox').classList.remove('disconnected');
    document.getElementById('connStatus').textContent = 'Connected · Port 58081';
  };
  es.onerror = () => {
    // 服务器关闭 → 日志流断开 → 显示横幅；浏览器会自动重连，服务器恢复后横幅自动消失
    document.getElementById('banner').style.display = 'block';
    document.getElementById('statusBox').classList.add('disconnected');
    document.getElementById('connStatus').textContent = 'Disconnected';
  };
}

refreshTools();
loadLogs();
connectLogStream();
</script>
</body>
</html>"##
}
