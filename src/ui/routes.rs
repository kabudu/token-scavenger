use crate::app::state::AppState;

pub async fn render_login(_state: &AppState) -> String {
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>Login // TokenScavenger</title>
<link rel="icon" type="image/png" href="/favicon.ico" />
<style>{}</style>
</head>
<body class="admin-shell">
<main class="min-h-screen w-full flex items-center justify-center p-6">
    <div class="glass-card w-full max-w-md p-8">
        <div class="flex items-center gap-3 mb-6">
            <img src="/ui/logo.png" alt="TokenScavenger Logo" class="w-10 h-10" />
            <div>
                <h1 class="text-xl font-bold">Token<span class="text-[#D35400]">Scavenger</span></h1>
                <p class="text-xs text-slate-500">Admin login</p>
            </div>
        </div>
        <form id="login-form" class="space-y-4">
            <label class="block">
                <span class="text-xs font-bold text-slate-500 uppercase">Master API Key</span>
                <input id="api-key" type="password" autocomplete="current-password" class="mt-2 w-full" autofocus required>
            </label>
            <button class="btn w-full" type="submit">Unlock Admin UI</button>
            <p id="login-error" class="hidden text-sm text-red-400"></p>
        </form>
    </div>
</main>
<script>
document.getElementById('login-form').addEventListener('submit', async (event) => {{
    event.preventDefault();
    const error = document.getElementById('login-error');
    error.classList.add('hidden');
    const apiKey = document.getElementById('api-key').value;
    const response = await fetch('/admin/session', {{
        method: 'POST',
        headers: {{'Content-Type': 'application/json'}},
        body: JSON.stringify({{api_key: apiKey}})
    }});
    if (response.ok) {{
        window.location.href = '/ui';
    }} else {{
        error.innerText = 'Invalid API key';
        error.classList.remove('hidden');
    }}
}});
</script>
</body>
</html>"##,
        include_str!("styles.css")
    )
}

pub fn render_shell(
    title: &str,
    active_nav: &str,
    content: &str,
    scripts: &str,
    state: &AppState,
) -> String {
    let uptime = state.start_time.elapsed().as_secs();
    let hrs = uptime / 3600;
    let mins = (uptime % 3600) / 60;
    let secs = uptime % 60;
    let uptime_str = format!("{:02}:{:02}:{:02}", hrs, mins, secs);

    let active_class =
        "sidebar-link active flex items-center gap-3 px-4 py-3 text-sm font-medium rounded-lg";
    let inactive_class = "sidebar-link flex items-center gap-3 px-4 py-3 text-sm font-medium rounded-lg text-slate-400";

    let nav_item = |id: &str, url: &str, icon: &str, label: &str| -> String {
        let class = if id == active_nav {
            active_class
        } else {
            inactive_class
        };
        format!(
            r#"<a href="{}" class="{}"><i class="{} w-5"></i> {}</a>"#,
            url, class, icon, label
        )
    };

    let nav = format!(
        r#"<nav class="flex-1 px-3 space-y-1">
        {}
        {}
        {}
        {}
        {}
        {}
        {}
        <div class="pt-4 pb-2 px-4 text-[10px] font-bold text-slate-600 uppercase tracking-widest">System</div>
        {}
        {}
        {}
        {}
        {}
        </nav>"#,
        nav_item("dashboard", "/ui", "fas fa-th-large", "Dashboard"),
        nav_item("providers", "/ui/providers", "fas fa-server", "Providers"),
        nav_item("models", "/ui/models", "fas fa-brain", "Models"),
        nav_item("routing", "/ui/routing", "fas fa-route", "Routing"),
        nav_item("usage", "/ui/usage", "fas fa-chart-line", "Usage"),
        nav_item("projects", "/ui/projects", "fas fa-key", "Projects"),
        nav_item("chat", "/ui/chat", "fas fa-comment-dots", "Chat Tester"),
        nav_item(
            "observability",
            "/ui/observability",
            "fas fa-wave-square",
            "Observability"
        ),
        nav_item("health", "/ui/health", "fas fa-heartbeat", "Health"),
        nav_item("logs", "/ui/logs", "fas fa-terminal", "Logs"),
        nav_item("config", "/ui/config", "fas fa-cog", "Config"),
        nav_item("audit", "/ui/audit", "fas fa-history", "Audit")
    );

    let head = format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>{} // TokenScavenger</title>
<link rel="icon" type="image/png" href="/favicon.ico" />
<script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css" />
<style>
{}
</style>
</head>"##,
        title,
        include_str!("styles.css")
    );

    format!(
        r##"{}
<body class="admin-shell">
<div id="sidebar-overlay" class="sidebar-overlay" onclick="closeSidebar()" aria-hidden="true"></div>
<aside id="admin-sidebar" class="admin-sidebar w-64 border-r border-white/5 flex flex-col shrink-0 bg-[#020617]" aria-label="Admin navigation">
    <div class="p-6 flex items-center justify-between gap-3">
    <div class="flex items-center gap-3 min-w-0">
        <img src="/ui/logo.png" alt="TokenScavenger Logo" class="w-8 h-8" />
        <span class="text-lg font-bold tracking-tight truncate">Token<span class="text-[#D35400]">Scavenger</span></span>
    </div>
    <button class="sidebar-close-btn" type="button" onclick="closeSidebar()" aria-label="Close navigation">
        <i class="fas fa-times" aria-hidden="true"></i>
    </button>
    </div>
    {}
    <div class="p-4 border-t border-white/5 bg-black/20">
    <div class="flex items-center justify-between mb-2">
        <span class="text-[10px] font-bold text-slate-500 uppercase">Version</span>
        <span class="text-[10px] font-mono text-emerald-500">v{}-STABLE</span>
    </div>
    <div class="flex items-center justify-between">
        <span class="text-[10px] font-bold text-slate-500 uppercase">Uptime</span>
        <span id="uptime-val" class="text-[10px] font-mono text-white">{}</span>
    </div>
    </div>
</aside>
<main class="admin-main flex-1 overflow-y-auto relative">
    <header class="mobile-topbar">
    <div class="flex items-center gap-3 min-w-0">
        <button id="sidebar-toggle" class="sidebar-toggle-btn" type="button" onclick="openSidebar()" aria-label="Open navigation" aria-controls="admin-sidebar" aria-expanded="false">
            <i class="fas fa-bars" aria-hidden="true"></i>
        </button>
        <img src="/ui/logo.png" alt="TokenScavenger Logo" class="w-7 h-7" />
        <span class="mobile-brand text-base font-bold tracking-tight truncate">Token<span class="text-[#D35400]">Scavenger</span></span>
    </div>
    <div class="mobile-title text-sm font-bold truncate">{}</div>
    </header>
    <header class="admin-page-header sticky top-0 z-10 bg-[#020617]/80 backdrop-blur-md border-b border-white/5 px-8 py-4 flex items-center justify-between">
    <div class="min-w-0">
        <h1 class="text-xl font-bold">{}</h1>
    </div>
    <div class="flex items-center gap-3">
        <div id="update-widget" class="hidden items-center gap-2 rounded-full border px-3 py-1.5" style="border-color: rgba(211,84,0,0.25); background: rgba(211,84,0,0.10);">
        <span class="h-2 w-2 rounded-full" style="background: #D35400; box-shadow: 0 0 10px rgba(211,84,0,0.55);"></span>
        <span class="text-[10px] font-bold uppercase tracking-wider" style="color: #D35400;">Update</span>
        <span id="update-widget-version" class="font-mono text-[10px] font-bold text-orange-100/90"></span>
        <span id="update-widget-asset" class="hidden max-w-[10rem] truncate text-[10px] text-orange-100/45 xl:inline"></span>
        <button id="update-apply-btn" class="ml-1 border-l pl-2 text-[10px] font-bold uppercase tracking-wider text-orange-100/80 transition hover:text-white" style="border-color: rgba(211,84,0,0.25);">Apply</button>
        </div>
        <div class="flex items-center gap-2 px-3 py-1.5 rounded-full border border-emerald-500/20 bg-emerald-500/10" id="health-badge">
        <span class="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" id="health-badge-dot"></span>
        <span class="text-[10px] font-bold text-emerald-500 uppercase tracking-wider" id="health-badge-text">Operational</span>
        </div>
    </div>
    </header>
    <div class="p-8 space-y-6">
    {}
    </div>
</main>
<div id="global-modal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm hidden opacity-0 transition-opacity duration-300">
    <div class="glass-card max-w-md w-full p-6 transform scale-95 transition-transform duration-300" id="global-modal-content">
        <h3 id="global-modal-title" class="text-lg font-bold mb-2"></h3>
        <p id="global-modal-message" class="text-sm text-slate-300 mb-6"></p>
        <div class="flex justify-end gap-3" id="global-modal-actions">
            <button class="btn" style="background:#334155;" onclick="hideModal()">Close</button>
        </div>
    </div>
</div>
<script>
function showModal(title, message, isError) {{
    const titleEl = document.getElementById('global-modal-title');
    const msgEl = document.getElementById('global-modal-message');
    const actionsEl = document.getElementById('global-modal-actions');
    
    titleEl.innerText = title;
    titleEl.className = `text-lg font-bold mb-2 ${{isError ? 'text-red-400' : 'text-emerald-400'}}`;
    msgEl.innerText = (typeof message === 'object') ? JSON.stringify(message, null, 2) : message;
    
    if (typeof message === 'object') {{ 
        msgEl.classList.add('font-mono', 'whitespace-pre-wrap', 'text-[10px]'); 
    }} else {{
        msgEl.classList.remove('font-mono', 'whitespace-pre-wrap', 'text-[10px]');
    }}

    actionsEl.innerHTML = `<button class="btn" style="background:#334155;" onclick="hideModal()">Close</button>`;

    const modal = document.getElementById('global-modal');
    modal.classList.remove('hidden');
    void modal.offsetWidth;
    modal.classList.remove('opacity-0');
    document.getElementById('global-modal-content').classList.remove('scale-95');
}}
function showConfirm(title, message, onConfirm) {{
    showModal(title, message, false);
    const actionsEl = document.getElementById('global-modal-actions');
    actionsEl.innerHTML = `
        <button class="btn" style="background:#334155;" onclick="hideModal()">Cancel</button>
        <button class="btn btn-primary" id="modal-confirm-btn">Confirm</button>
    `;
    document.getElementById('modal-confirm-btn').onclick = () => {{
        onConfirm();
        hideModal();
    }};
}}
function hideModal() {{
    const modal = document.getElementById('global-modal');
    modal.classList.add('opacity-0');
    document.getElementById('global-modal-content').classList.add('scale-95');
    setTimeout(() => {{ modal.classList.add('hidden'); }}, 300);
}}
function setSidebarOpen(isOpen) {{
    const sidebar = document.getElementById('admin-sidebar');
    const overlay = document.getElementById('sidebar-overlay');
    const toggle = document.getElementById('sidebar-toggle');
    if (!sidebar || !overlay) return;
    sidebar.classList.toggle('open', isOpen);
    overlay.classList.toggle('open', isOpen);
    document.body.classList.toggle('sidebar-open', isOpen);
    if (toggle) toggle.setAttribute('aria-expanded', String(isOpen));
}}
function openSidebar() {{ setSidebarOpen(true); }}
function closeSidebar() {{ setSidebarOpen(false); }}
document.addEventListener('keydown', (event) => {{
    if (event.key === 'Escape') closeSidebar();
}});
document.querySelectorAll('#admin-sidebar a').forEach((link) => {{
    link.addEventListener('click', () => {{
        if (window.matchMedia('(max-width: 900px)').matches) closeSidebar();
    }});
}});
window.matchMedia('(min-width: 901px)').addEventListener('change', (event) => {{
    if (event.matches) closeSidebar();
}});
let startTime = Date.now() - {};
setInterval(() => {{
    const diff = Math.floor((Date.now() - startTime) / 1000);
    const hrs = String(Math.floor(diff / 3600)).padStart(2, "0");
    const mins = String(Math.floor((diff % 3600) / 60)).padStart(2, "0");
    const secs = String(diff % 60).padStart(2, "0");
    document.getElementById("uptime-val").innerText = `${{hrs}}:${{mins}}:${{secs}}`;
}}, 1000);

const healthStates = {};
let worstState = "Healthy";
for (const [pid, h] of Object.entries(healthStates)) {{
    if (h.state === "Unhealthy") worstState = "Unhealthy";
    else if (h.state === "Degraded" && worstState !== "Unhealthy") worstState = "Degraded";
}}
const bText = document.getElementById('health-badge-text');
const bDot = document.getElementById('health-badge-dot');
const bCont = document.getElementById('health-badge');
if (worstState === "Healthy") {{
    bText.innerText = "Operational"; bText.className = "text-[10px] font-bold text-emerald-500 uppercase tracking-wider";
    bDot.className = "w-2 h-2 rounded-full bg-emerald-500 animate-pulse";
    bCont.className = "flex items-center gap-2 px-3 py-1.5 rounded-full bg-emerald-500/10 border border-emerald-500/20";
}} else if (worstState === "Degraded") {{
    bText.innerText = "Degraded"; bText.className = "text-[10px] font-bold text-yellow-500 uppercase tracking-wider";
    bDot.className = "w-2 h-2 rounded-full bg-yellow-500 animate-pulse";
    bCont.className = "flex items-center gap-2 px-3 py-1.5 rounded-full bg-yellow-500/10 border border-yellow-500/20";
}} else {{
    bText.innerText = "Unhealthy"; bText.className = "text-[10px] font-bold text-red-500 uppercase tracking-wider";
    bDot.className = "w-2 h-2 rounded-full bg-red-500 animate-pulse";
    bCont.className = "flex items-center gap-2 px-3 py-1.5 rounded-full bg-red-500/10 border border-red-500/20";
}}

async function checkForUpdate() {{
    try {{
        const response = await fetch('/admin/update/check');
        if (!response.ok) return;
        const update = await response.json();
        if (!update.enabled || !update.update_available) return;
        const widget = document.getElementById('update-widget');
        const version = document.getElementById('update-widget-version');
        const asset = document.getElementById('update-widget-asset');
        const button = document.getElementById('update-apply-btn');
        if (!widget || !version || !asset || !button) return;
        version.innerText = `v${{update.latest_version}}`;
        asset.innerText = update.asset_name || 'current platform';
        widget.title = `TokenScavenger v${{update.latest_version}} is available for ${{update.asset_name || 'this platform'}}.`;
        button.onclick = () => showConfirm('Apply update?', 'TokenScavenger will install the verified release asset, restart with the same arguments, and reload the admin UI.', async () => {{
            const apply = await fetch('/admin/update/apply', {{ method: 'POST' }});
            const body = await apply.json().catch(() => ({{}}));
            if (!apply.ok) {{
                showModal('Update failed', body, true);
                return;
            }}
            showModal('Restart scheduled', 'The new version is being installed. This page will reload shortly.', false);
            setTimeout(() => window.location.reload(), 5000);
        }});
        widget.classList.remove('hidden');
        widget.classList.add('flex');
    }} catch (_) {{
        // Update checks are best-effort and should never disturb the admin UI.
    }}
}}
checkForUpdate();
</script>
{}
</body>
</html>"##,
        head,
        nav,
        env!("CARGO_PKG_VERSION"),
        uptime_str,
        title,
        title,
        content,
        uptime * 1000,
        {
            let health_map: std::collections::HashMap<_, _> = state
                .health_states
                .iter()
                .map(|e| {
                    (
                        e.key().clone(),
                        serde_json::json!({ "state": format!("{:?}", e.value().state) }),
                    )
                })
                .collect();
            serde_json::to_string(&health_map).unwrap_or("{}".into())
        },
        scripts
    )
}

/// Format a number with commas
fn format_number(num: i64) -> String {
    let mut s = num.to_string();
    let mut result = String::new();
    while s.len() > 3 {
        let len = s.len();
        result = format!(",{}", &s[len - 3..]) + &result;
        s = s[..len - 3].to_string();
    }
    s + &result
}

/// Format a number as currency (USD)
fn format_currency(amount: f64) -> String {
    if amount == 0.0 {
        "$0.00".to_string()
    } else if amount < 0.01 {
        format!("${:.4}", amount)
    } else {
        format!("${:.2}", amount)
    }
}

/// Render the dashboard view.
pub async fn render_dashboard(state: &AppState) -> String {
    let config = state.config();

    // Initial data for SSR
    let usage = crate::usage::aggregation::get_usage_series(state, "24h").await;
    let metrics = crate::usage::aggregation::get_period_summary(state, "24h").await;

    let mut estimated_cost = 0.0_f64;
    let mut total_tokens = 0_i64;
    if let Some(series) = usage.get("series").and_then(|v| v.as_array()) {
        for entry in series {
            estimated_cost += entry
                .get("estimated_cost_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            total_tokens += entry
                .get("input_tokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                + entry
                    .get("output_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
        }
    }

    let request_count = metrics
        .get("request_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let avg_latency = metrics
        .get("avg_latency")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let provider_latencies: std::collections::HashMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "SELECT selected_provider_id, CAST(AVG(latency_ms) AS INTEGER) FROM request_log WHERE status = 'success' GROUP BY selected_provider_id"
    )
    .fetch_all(&state.db).await.unwrap_or_default().into_iter().collect();

    let hourly_traffic = crate::usage::aggregation::get_hourly_traffic(state, "24h").await;
    let provider_dist = crate::usage::aggregation::get_provider_distribution(state, "24h").await;

    let mut provider_rows = String::new();
    for p in &config.providers {
        if !p.enabled {
            continue;
        }
        let health = state.health_states.get(&p.id);
        let status = health
            .as_ref()
            .map(|h| format!("{:?}", h.value().state))
            .unwrap_or("Unknown".into());
        let status_html = match status.as_str() {
            "Healthy" => {
                r#"<span class="px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-500 text-[10px]">Optimal</span>"#
            }
            "Degraded" => {
                r#"<span class="px-2 py-0.5 rounded bg-yellow-500/10 text-yellow-500 text-[10px]">Degraded</span>"#
            }
            "Unhealthy" => {
                r#"<span class="px-2 py-0.5 rounded bg-red-500/10 text-red-500 text-[10px]">Unhealthy</span>"#
            }
            _ => {
                r#"<span class="px-2 py-0.5 rounded bg-white/5 text-slate-500 text-[10px]">Standby</span>"#
            }
        };
        let lat = provider_latencies
            .get(&p.id)
            .map(|l| format!("{}ms", l))
            .unwrap_or("--".to_string());
        provider_rows.push_str(&format!(
            r#"<tr><td class="px-6 py-4 font-bold">{}</td><td class="px-6 py-4">{}</td><td class="px-6 py-4 font-mono">{}</td></tr>"#,
            p.id, status_html, lat
        ));
    }

    let content = format!(
        r##"
        <div class="flex items-center justify-between mb-2">
            <div class="text-xs font-bold text-slate-500 uppercase tracking-widest">Performance Metrics</div>
            <div class="period-selector" id="main-period-selector">
                <div class="period-btn active" onclick="changePeriod('24h')">24H</div>
                <div class="period-btn" onclick="changePeriod('7d')">7D</div>
                <div class="period-btn" onclick="changePeriod('30d')">30D</div>
                <div class="period-btn" onclick="changePeriod('1y')">1Y</div>
            </div>
        </div>
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-5 gap-4">
          <div class="glass-card p-5 metric-glow-orange">
            <div class="flex justify-between items-start mb-2"><span class="text-xs font-bold text-slate-500 uppercase">Active Providers</span><i class="fas fa-server text-[#D35400]"></i></div>
            <div id="stat-providers" class="text-3xl font-bold">{}</div>
          </div>
          <div class="glass-card p-5">
            <div class="flex justify-between items-start mb-2"><span class="text-xs font-bold text-slate-500 uppercase">Total Requests</span><i class="fas fa-exchange-alt text-slate-500"></i></div>
            <div id="stat-requests" class="text-3xl font-bold">{}</div>
          </div>
          <div class="glass-card p-5 metric-glow-cyan">
            <div class="flex justify-between items-start mb-2"><span class="text-xs font-bold text-slate-500 uppercase">Total Tokens</span><i class="fas fa-coins text-cyan-400"></i></div>
            <div id="stat-tokens" class="text-3xl font-bold">{}</div>
          </div>
          <div class="glass-card p-5">
            <div class="flex justify-between items-start mb-2"><span class="text-xs font-bold text-slate-500 uppercase">Avg Latency</span><i class="fas fa-bolt text-yellow-400"></i></div>
            <div class="text-3xl font-bold"><span id="stat-latency">{}</span><span class="text-sm font-normal text-slate-500 ml-1">ms</span></div>
          </div>
          <div class="glass-card p-5 metric-glow-emerald">
            <div class="flex justify-between items-start mb-2"><span class="text-xs font-bold text-slate-500 uppercase">Estimated Spend</span><i class="fas fa-piggy-bank text-emerald-400"></i></div>
            <div class="text-3xl font-bold"><span id="stat-spend">{}</span></div>
            <div class="text-[10px] text-slate-500 font-mono mt-1 flex items-center gap-1" id="stat-spend-confidence-wrapper">
                <span class="cursor-pointer text-slate-500 hover:text-cyan-400 transition-colors" id="pricing-info-icon" onclick="togglePricingInfo()" title="Pricing source details">&#9432;</span>
                <span id="pricing-info-tooltip" class="hidden absolute z-50 bg-slate-800 border border-slate-600 rounded-lg p-3 text-xs text-slate-300 max-w-xs shadow-xl leading-relaxed" style="margin-top: 6rem;"></span>
            </div>
          </div>
        </div>
        <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div class="lg:col-span-2 glass-card p-6">
            <div class="flex items-center justify-between mb-6">
                <h3 class="font-bold text-slate-300">Token Scavenging Efficiency</h3>
                <div class="text-[10px] text-slate-500 font-mono" id="traffic-period-label">PAST 24 HOURS</div>
            </div>
            <div class="h-64"><canvas id="trafficChart"></canvas></div>
          </div>
          <div class="lg:col-span-1 glass-card p-6">
            <h3 class="font-bold text-slate-300 mb-6">Provider Distribution</h3>
            <div class="h-64 flex flex-col justify-between"><canvas id="providerChart"></canvas></div>
          </div>
        </div>
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 flex items-center justify-between bg-white/[0.02]">
              <h3 class="font-bold text-sm">Priority Routing Chains</h3>
            </div>
            <div class="p-0 overflow-x-auto">
              <table class="w-full text-left text-xs"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th class="px-6 py-3">Provider</th><th class="px-6 py-3">Status</th><th class="px-6 py-3">Latency</th></tr></thead><tbody class="divide-y divide-white/5">{}</tbody></table>
            </div>
          </div>
          <div class="glass-card overflow-hidden flex flex-col h-full">
            <div class="px-6 py-4 border-b border-white/5 flex items-center justify-between bg-white/[0.02]"><h3 class="font-bold text-sm">System Stream</h3><div class="flex gap-2"><span class="w-2 h-2 rounded-full bg-emerald-500"></span><span class="w-2 h-2 rounded-full bg-slate-700"></span></div></div>
            <div id="logs" class="p-4 bg-[#010409] font-mono text-[10px] leading-relaxed overflow-y-auto h-64 text-slate-400"></div>
          </div>
        </div>
    "##,
        config.providers.iter().filter(|p| p.enabled).count(),
        format_number(request_count),
        format_number(total_tokens),
        avg_latency,
        format_currency(estimated_cost),
        provider_rows
    );

    let scripts = format!(
        r##"<script>
      Chart.defaults.color = "#64748b"; Chart.defaults.font.family = "Inter";
      let trafficChart, providerChart;
      
      const initCharts = (trafficData, providerData) => {{
          const ctxTraffic = document.getElementById("trafficChart").getContext("2d");
          const gradientFox = ctxTraffic.createLinearGradient(0, 0, 0, 400);
          gradientFox.addColorStop(0, "rgba(211, 84, 0, 0.3)"); gradientFox.addColorStop(1, "rgba(211, 84, 0, 0)");
          
          trafficChart = new Chart(ctxTraffic, {{
            type: "line",
            data: {{
              labels: trafficData.labels || [],
              datasets: [
                {{ label: "Scavenged Tokens (Free)", data: trafficData.free_tokens || [], borderColor: "#D35400", backgroundColor: gradientFox, fill: true, tension: 0.4, borderWidth: 2, pointRadius: 3 }},
                {{ label: "Paid Overflow", data: trafficData.paid_tokens || [], borderColor: "#00F5FF", borderDash: [5, 5], fill: false, tension: 0.4, borderWidth: 1.5, pointRadius: 3 }}
              ]
            }},
            options: {{ responsive: true, maintainAspectRatio: false, plugins: {{ legend: {{ display: true, position: "top", align: "end", labels: {{ boxWidth: 10, usePointStyle: true }} }} }}, scales: {{ y: {{ beginAtZero: true, grid: {{ color: "rgba(255, 255, 255, 0.03)" }} }}, x: {{ grid: {{ display: false }} }} }} }}
          }});
          
          providerChart = new Chart(document.getElementById("providerChart").getContext("2d"), {{
            type: "doughnut",
            data: {{
              labels: providerData.labels || [],
              datasets: [{{ data: providerData.data || [], backgroundColor: ["#D35400", "#E67E22", "#00FF9F", "#00F5FF", "#1e293b"], borderWidth: 0, hoverOffset: 10 }}]
            }},
            options: {{ responsive: true, maintainAspectRatio: false, cutout: "70%", plugins: {{ legend: {{ position: "bottom", labels: {{ boxWidth: 8, padding: 20 }} }} }} }}
          }});
      }};

      let currentAnalyticsPeriod = "24h";
      let analyticsRefreshInFlight = false;

      function updatePeriodControls(period) {{
          document.querySelectorAll('#main-period-selector .period-btn').forEach(btn => {{
              btn.classList.toggle('active', btn.innerText.toLowerCase() === period.toLowerCase());
          }});
          document.getElementById('traffic-period-label').innerText = 'PAST ' + (period === '24h' ? '24 HOURS' : period === '7d' ? '7 DAYS' : period === '30d' ? 'MONTH' : 'YEAR');
      }}

      async function refreshDashboardAnalytics(period = currentAnalyticsPeriod) {{
          if (analyticsRefreshInFlight || document.hidden) return;
          analyticsRefreshInFlight = true;
          try {{
              const [traffic, dist, summary, metrics] = await Promise.all([
                  fetch(`/admin/analytics/traffic?period=${{period}}`).then(r => r.json()),
                  fetch(`/admin/analytics/distribution?period=${{period}}`).then(r => r.json()),
                  fetch(`/admin/analytics/summary?period=${{period}}`).then(r => r.json()),
                  fetch(`/admin/analytics/metrics?period=${{period}}`).then(r => r.json())
              ]);

              trafficChart.data.labels = traffic.labels || [];
              trafficChart.data.datasets[0].data = traffic.free_tokens || [];
              trafficChart.data.datasets[1].data = traffic.paid_tokens || [];
              trafficChart.update();

              providerChart.data.labels = dist.labels || [];
              providerChart.data.datasets[0].data = dist.data || [];
              providerChart.update();

              let totalTokens = 0, totalCost = 0;
              const confidences = new Set();
              (summary.series || []).forEach(s => {{
                  totalTokens += (s.input_tokens || 0) + (s.output_tokens || 0);
                  totalCost += s.estimated_cost_usd || 0;
                  String(s.cost_confidence || '').split(',').filter(Boolean).forEach(c => confidences.add(c));
              }});

              document.getElementById('stat-requests').innerText = (metrics.request_count || 0).toLocaleString();
              document.getElementById('stat-tokens').innerText = totalTokens.toLocaleString();
              document.getElementById('stat-latency').innerText = metrics.avg_latency || 0;
              document.getElementById('stat-spend').innerText = new Intl.NumberFormat('en-US', {{ style: 'currency', currency: 'USD', minimumFractionDigits: 2, maximumFractionDigits: 2 }}).format(totalCost);
              updatePricingConfidence(confidences);
          }} catch (error) {{
              console.warn("Dashboard analytics refresh failed", error);
          }} finally {{
              analyticsRefreshInFlight = false;
          }}
      }}

      const PRICING_CONFIDENCE_LABELS = {{
          'free_tier': 'Free provider \u2014 no cost',
          'provider_published': 'Official provider pricing',
          'scraped': 'Scraped from provider website',
          'builtin': 'Built-in estimate',
          'fallback_estimate': 'Estimated from public sources \u2014 may differ from actual rates',
          'operator_override': 'Operator override',
          'unknown_price': 'Unknown pricing \u2014 cost not estimated',
          'unknown': 'Unknown pricing source',
      }};

      function humanizeConfidence(raw) {{
          if (!raw) return 'Unknown';
          // Handle compound codes like backfilled_current_rate:provider_published
          if (raw.startsWith('backfilled_current_rate:')) {{
              const inner = raw.slice('backfilled_current_rate:'.length);
              const label = PRICING_CONFIDENCE_LABELS[inner] || inner;
              return 'Backfilled using current rate \u2014 ' + label;
          }}
          return PRICING_CONFIDENCE_LABELS[raw] || raw;
      }}

      function updatePricingConfidence(confidences) {{
          const icon = document.getElementById('pricing-info-icon');
          const tooltip = document.getElementById('pricing-info-tooltip');
          if (!confidences.size) {{
              icon.style.display = 'none';
              return;
          }}
          icon.style.display = '';
          const parts = Array.from(confidences).filter(Boolean);
          const lines = parts.map(c => '<div>' + humanizeConfidence(c) + '</div>').join('');
          tooltip.innerHTML = '<div class=\'font-bold text-cyan-400 mb-1\'>Pricing Sources</div>' + lines;
      }}

      function togglePricingInfo() {{
          const tooltip = document.getElementById('pricing-info-tooltip');
          const isHidden = tooltip.classList.contains('hidden');
          // Close tooltip if it was open
          if (!isHidden) {{ tooltip.classList.add('hidden'); return; }}
          // Position and show tooltip
          const icon = document.getElementById('pricing-info-icon');
          const rect = icon.getBoundingClientRect();
          tooltip.style.position = 'fixed';
          tooltip.style.left = rect.right + 8 + 'px';
          tooltip.style.top = rect.top + 'px';
          tooltip.style.marginTop = '0';
          tooltip.classList.remove('hidden');
          // Dismiss on outside click
          setTimeout(() => {{
              document.addEventListener('click', function dismiss(e) {{
                  if (!tooltip.contains(e.target) && e.target !== icon) {{
                      tooltip.classList.add('hidden');
                      document.removeEventListener('click', dismiss);
                  }}
              }});
          }}, 0);
      }}

      async function changePeriod(period) {{
          currentAnalyticsPeriod = period;
          updatePeriodControls(period);
          await refreshDashboardAnalytics(period);
      }}

      initCharts({}, {});
      setInterval(() => refreshDashboardAnalytics(), 30000);
      
      const es=new EventSource('/admin/logs/stream');
      es.onmessage=(e)=>{{ 
          const el=document.getElementById('logs'); 
          const line=document.createElement('div'); 
          line.className="mb-1"; 
          line.textContent=e.data; 
          el.appendChild(line); 
          el.scrollTop=el.scrollHeight; 
          if (el.childNodes.length > 100) el.removeChild(el.firstChild);
      }};
    </script>"##,
        serde_json::to_string(&hourly_traffic).unwrap_or("{}".into()),
        serde_json::to_string(&provider_dist).unwrap_or("{}".into())
    );

    render_shell("Management Console", "dashboard", &content, &scripts, state)
}

/// Render the providers view.
pub async fn render_providers(state: &AppState) -> String {
    let config = state.config();
    let mut rows = String::new();
    let provider_options = crate::providers::registry::SUPPORTED_PROVIDERS
        .iter()
        .map(|provider| {
            format!(
                r#"<option value="{id}" data-free-only="{free_only}" data-default-base-url="{base_url}">{name}</option>"#,
                id = provider.id,
                free_only = provider.free_only_default,
                base_url = provider.default_base_url,
                name = provider.display_name
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let provider_defaults = serde_json::to_string(crate::providers::registry::SUPPORTED_PROVIDERS)
        .unwrap_or_else(|_| "[]".to_string());
    for p in &config.providers {
        let health = state.health_states.get(&p.id);
        let health_str = health
            .as_ref()
            .map(|h| format!("{:?}", h.value().state))
            .unwrap_or("unknown".into());
        let next_enabled = if p.enabled { "false" } else { "true" };
        let button_label = if p.enabled { "Disable" } else { "Enable" };
        let button_class = if p.enabled { "btn-danger" } else { "btn" };
        rows.push_str(&format!(
            r#"<tr><td class="font-mono text-sm text-cyan-400">{}</td><td class="text-sm">{}</td><td><span class="px-2 py-0.5 rounded {} text-[10px]">{}</span></td><td><div class="flex gap-2"><button class="{}" onclick="toggleProvider('{}',{})">{}</button><button class="btn" style="background:#334155;" onclick="testProvider('{}')">Test</button></div></td></tr>"#,
            p.id, health_str, if p.enabled { "bg-emerald-500/10 text-emerald-500" } else { "bg-white/5 text-slate-500" }, if p.enabled { "Enabled" } else { "Disabled" }, button_class, p.id, next_enabled, button_label, p.id
        ));
    }
    let content = format!(
        r#"
        <div class="glass-card provider-add-card p-6 mb-6">
            <div class="flex items-center justify-between gap-4 mb-5">
                <div>
                    <h3 class="font-bold">Add Provider</h3>
                    <p class="text-xs text-slate-500 mt-1">Choose from the full baked-in provider catalog.</p>
                </div>
                <span class="text-xs text-slate-500">Paid fallback obeys routing policy</span>
            </div>
            <div class="provider-add-grid">
                <label class="provider-field">
                    <span>Provider</span>
                    <select id="new-provider-id" class="provider-input" onchange="syncProviderDefaults()">
                        {}
                    </select>
                </label>
                <label class="provider-field">
                    <span>API Key</span>
                    <input id="new-provider-key" class="provider-input" type="password" autocomplete="off" placeholder="sk-...">
                </label>
                <label class="provider-field">
                    <span>Base URL</span>
                    <input id="new-provider-base-url" class="provider-input" type="url" placeholder="Default">
                </label>
                <label class="toggle-row">
                    <input id="new-provider-free-only" class="toggle-input" type="checkbox">
                    <span class="toggle-track" aria-hidden="true"><span class="toggle-thumb"></span></span>
                    <span class="toggle-copy">
                        <span>Free only</span>
                        <small>Block paid endpoint use</small>
                    </span>
                </label>
                <label class="provider-field">
                    <span>Embeddings</span>
                    <select id="new-provider-embedding-support" class="provider-input">
                        <option value="auto">Auto probe</option>
                        <option value="enabled">Force on</option>
                        <option value="disabled">Force off</option>
                    </select>
                </label>
                <button class="btn provider-add-btn" onclick="addProvider()">Add Provider</button>
            </div>
        </div>
        <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 flex items-center justify-between bg-white/[0.02]">
                <h3 class="font-bold">Configured Providers</h3>
                <button id="refresh-discovery-btn" class="btn flex items-center justify-center min-w-[140px]" onclick="refreshDiscovery()">
                    <span class="btn-text">Refresh Discovery</span>
                </button>
            </div>
            <div class="p-0 overflow-x-auto">
                <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>ID</th><th>Health</th><th>Status</th><th>Actions</th></tr></thead><tbody class="divide-y divide-white/5">{}</tbody></table>
            </div>
        </div>"#,
        provider_options, rows
    );
    let scripts = r#"<script>
    const providerDefaults = __PROVIDER_DEFAULTS__;
    function syncProviderDefaults() {
        const id = document.getElementById('new-provider-id').value;
        const freeOnly = document.getElementById('new-provider-free-only');
        const baseUrl = document.getElementById('new-provider-base-url');
        const selected = providerDefaults.find(provider => provider.id === id) || {};
        freeOnly.checked = selected.free_only_default !== false;
        baseUrl.placeholder = selected.default_base_url || 'Default';
    }
    async function addProvider() {
        const id = document.getElementById('new-provider-id').value;
        const apiKey = document.getElementById('new-provider-key').value.trim();
        const baseUrl = document.getElementById('new-provider-base-url').value.trim();
        const freeOnly = document.getElementById('new-provider-free-only').checked;
        const embeddingSupport = document.getElementById('new-provider-embedding-support').value;
        if (!apiKey) { showModal('Error', 'API key is required', true); return; }
        const provider = { id, enabled: true, api_key: apiKey, free_only: freeOnly, embedding_support: embeddingSupport };
        if (baseUrl) provider.base_url = baseUrl;
        const r = await fetch('/admin/config', {method:'PUT', headers:{'Content-Type':'application/json'}, body:JSON.stringify({providers:[provider]})});
        if (r.ok) location.reload(); else showModal('Error', 'Provider add failed', true);
    }
    async function toggleProvider(id, enabled) { const r = await fetch('/admin/config', {method:'PUT', headers:{'Content-Type':'application/json'}, body:JSON.stringify({providers:[{id, enabled}]})}); if (r.ok) location.reload(); else showModal('Error', 'Provider update failed', true); }
    async function testProvider(id) { const r = await fetch('/admin/providers/'+encodeURIComponent(id)+'/test', {method:'POST'}); const data = await r.json(); showModal('Test Result', data, data.status === 'error'); }
    async function refreshDiscovery() { 
        const btn = document.getElementById('refresh-discovery-btn');
        if (!btn) return;
        btn.classList.add('btn-loading');
        try {
            const r = await fetch('/admin/providers/discovery/refresh', {method:'POST'}); 
            if (r.ok) location.reload(); 
            else showModal('Error', 'Discovery refresh failed', true); 
        } catch (e) {
            showModal('Error', 'Network error', true);
        } finally {
            btn.classList.remove('btn-loading');
        }
    }
    syncProviderDefaults();
    </script>"#
        .replace("__PROVIDER_DEFAULTS__", &provider_defaults);
    render_shell("Providers", "providers", &content, &scripts, state)
}

/// Render the models view.
pub async fn render_models(state: &AppState) -> String {
    let models = crate::discovery::merge::get_all_models(state).await;
    let models_json = serde_json::to_string(&models).unwrap_or("{}".into());
    let initial_rows = render_initial_model_rows(&models);
    let content = format!(
        r#"
        <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02] flex items-center justify-between gap-4">
                <h3 class="font-bold">Model Catalog</h3>
                <div class="flex gap-2 text-sm flex-1 max-w-md ml-auto">
                    <input type="text" id="modelSearch" placeholder="Search models..." class="flex-1" onkeyup="renderTable()">
                    <select id="providerFilter" onchange="renderTable()">
                        <option value="">All Providers</option>
                    </select>
                </div>
            </div>
            <div class="p-0 overflow-x-auto min-h-[300px]">
                <table class="w-full text-left">
                    <thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]">
                        <tr><th>Model ID</th><th>Provider</th><th>Intelligence</th><th>Freshness</th><th>Status</th><th>Priority</th><th>Actions</th></tr>
                    </thead>
                    <tbody id="modelsTableBody" class="divide-y divide-white/5">{}</tbody>
                </table>
            </div>
            <div class="px-6 py-3 border-t border-white/5 bg-white/[0.01] flex items-center justify-between">
                <span id="pageInfo" class="text-xs text-slate-500"></span>
                <div class="flex gap-2">
                    <button class="btn" style="background:#334155;" onclick="prevPage()">Previous</button>
                    <button class="btn" style="background:#334155;" onclick="nextPage()">Next</button>
                </div>
            </div>
        </div>"#,
        initial_rows
    );
    let scripts = format!(
        r#"<script>
    let modelsData = {};
    let modelsArr = modelsData.models || [];
    let currentPage = 1;
    const pageSize = 10;
    
    function syncProviderFilter() {{
        const select = document.getElementById('providerFilter');
        const current = select.value;
        const providers = [...new Set(modelsArr.map(m => m.provider_id).filter(Boolean))].sort();
        select.innerHTML = '<option value="">All Providers</option>';
        providers.forEach(p => {{
            select.innerHTML += `<option value="${{p}}">${{p}}</option>`;
        }});
        if (providers.includes(current)) select.value = current;
    }}

    function setModelStatus(message) {{
        document.getElementById('modelsTableBody').innerHTML = `<tr><td colspan="7" class="px-6 py-4 text-center text-slate-500">${{message}}</td></tr>`;
        document.getElementById('pageInfo').innerText = '';
    }}

    async function loadModels() {{
        try {{
            const response = await fetch('/admin/models', {{headers: {{'Accept': 'application/json'}}}});
            if (!response.ok) throw new Error(`HTTP ${{response.status}}`);
            modelsData = await response.json();
            modelsArr = modelsData.models || [];
            currentPage = 1;
            syncProviderFilter();
            renderTable();
        }} catch (error) {{
            console.error('Failed to load model catalog', error);
            if (modelsArr.length > 0) {{
                syncProviderFilter();
                renderTable();
            }} else {{
                setModelStatus('Failed to load models');
            }}
        }}
    }}

    function renderTable() {{
        const search = document.getElementById('modelSearch').value.toLowerCase();
        const prov = document.getElementById('providerFilter').value;
        const filtered = modelsArr.filter(m => {{
            const matchS = (m.upstream_model_id || '').toLowerCase().includes(search);
            const matchP = prov ? m.provider_id === prov : true;
            return matchS && matchP;
        }});
        const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize));
        if (currentPage > totalPages) currentPage = totalPages;
        const start = (currentPage - 1) * pageSize;
        const slice = filtered.slice(start, start + pageSize);

        let html = '';
        if (slice.length === 0) {{
            html = `<tr><td colspan="7" class="px-6 py-4 text-center text-slate-500">No models found</td></tr>`;
        }} else {{
            slice.forEach(m => {{
                const u = m.upstream_model_id || '?';
                const p = m.provider_id || '?';
                const intel = m.intelligence || {{}};
                const tags = (intel.task_tags || []).slice(0, 3).map(t => `<span class="text-[10px] px-1.5 py-0.5 rounded bg-cyan-500/10 text-cyan-300 border border-cyan-500/10">${{t}}</span>`).join('');
                const modalities = (intel.modalities || []).map(t => `<span class="text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-300 border border-emerald-500/10">${{t}}</span>`).join('');
                const context = intel.context_window ? `${{Number(intel.context_window).toLocaleString()}} ctx` : 'unknown ctx';
                const freshness = m.freshness || intel.freshness || 'Unknown';
                const freshnessScore = Math.round((m.freshness_score || intel.freshness_score || 0) * 100);
                const enabled = m.enabled !== false;
                const next_enabled = !enabled;
                const button_label = enabled ? "Disable" : "Enable";
                const button_class = enabled ? "btn-danger" : "btn";
                const prio = m.priority || 100;
                const status_html = enabled 
                    ? `<span class="px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-500 text-[10px]">Enabled</span>`
                    : `<span class="px-2 py-0.5 rounded bg-white/5 text-slate-500 text-[10px]">Disabled</span>`;
                html += `<tr><td class="font-mono text-sm text-cyan-400">${{u}}<div class="text-[10px] text-slate-500 mt-1">${{intel.family || 'general'}} · ${{context}}</div></td><td class="text-sm">${{p}}</td><td><div class="flex flex-wrap gap-1 max-w-xs">${{tags}}${{modalities}}</div></td><td class="text-xs"><span class="text-slate-300">${{freshness}}</span><div class="text-[10px] text-slate-500">${{freshnessScore}}%</div></td><td>${{status_html}}</td><td><input type="number" value="${{prio}}" class="w-16 bg-black/20 border border-white/10 rounded px-2 py-0.5 text-xs text-center" onchange="updateModelPriority('${{p.replace(/'/g, "\\'")}}','${{u.replace(/'/g, "\\'")}}', this.value)"></td><td><button class="${{button_class}}" onclick="toggleModel('${{p.replace(/'/g, "\\'")}}','${{u.replace(/'/g, "\\'")}}',${{next_enabled}})">${{button_label}}</button></td></tr>`;
            }});
        }}
        document.getElementById('modelsTableBody').innerHTML = html;
        document.getElementById('pageInfo').innerText = `Page ${{currentPage}} of ${{totalPages}} (${{filtered.length}} total)`;
    }}

    function prevPage() {{ if (currentPage > 1) {{ currentPage--; renderTable(); }} }}
    function nextPage() {{ const search = document.getElementById('modelSearch').value.toLowerCase(); const prov = document.getElementById('providerFilter').value; const totalPages = Math.ceil(modelsArr.filter(m => (m.upstream_model_id || '').toLowerCase().includes(search) && (prov ? m.provider_id === prov : true)).length / pageSize); if (currentPage < totalPages) {{ currentPage++; renderTable(); }} }}

    async function toggleModel(provider_id, model_id, enabled) {{ const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{models:[{{provider_id, model_id, enabled}}]}})}}); if (r.ok) loadModels(); else showModal('Error', 'Model update failed', true); }}
    async function updateModelPriority(provider_id, model_id, priority) {{ const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{models:[{{provider_id, model_id, priority: parseInt(priority)}}]}})}}); if (r.ok) loadModels(); else showModal('Error', 'Priority update failed', true); }}
    
    syncProviderFilter();
    renderTable();
    loadModels();
    </script>"#,
        models_json
    );
    render_shell("Models", "models", &content, &scripts, state)
}

fn render_initial_model_rows(models: &serde_json::Value) -> String {
    let Some(models) = models.get("models").and_then(|value| value.as_array()) else {
        return r#"<tr><td colspan="7" class="px-6 py-4 text-center text-slate-500">No models found</td></tr>"#.to_string();
    };
    if models.is_empty() {
        return r#"<tr><td colspan="7" class="px-6 py-4 text-center text-slate-500">No models found</td></tr>"#.to_string();
    }

    models
        .iter()
        .take(10)
        .map(|model| {
            let upstream = escape_html(
                model
                    .get("upstream_model_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("?"),
            );
            let provider = escape_html(
                model
                    .get("provider_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("?"),
            );
            let enabled = model
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let priority = model
                .get("priority")
                .and_then(|value| value.as_i64())
                .unwrap_or(100);
            let intelligence = model.get("intelligence").unwrap_or(&serde_json::Value::Null);
            let family = escape_html(
                intelligence
                    .get("family")
                    .and_then(|value| value.as_str())
                    .unwrap_or("general"),
            );
            let context = intelligence
                .get("context_window")
                .and_then(|value| value.as_u64())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let freshness = escape_html(
                model
                    .get("freshness")
                    .or_else(|| intelligence.get("freshness"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("Unknown"),
            );
            let status = if enabled {
                r#"<span class="px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-500 text-[10px]">Enabled</span>"#
            } else {
                r#"<span class="px-2 py-0.5 rounded bg-white/5 text-slate-500 text-[10px]">Disabled</span>"#
            };
            format!(
                r#"<tr><td class="font-mono text-sm text-cyan-400">{}<div class="text-[10px] text-slate-500 mt-1">{} · {} ctx</div></td><td class="text-sm">{}</td><td class="text-xs text-slate-500">Loading...</td><td class="text-xs">{}</td><td>{}</td><td class="text-sm">{}</td><td class="text-xs text-slate-500">Loading...</td></tr>"#,
                upstream, family, context, provider, freshness, status, priority
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Render the routing view.
pub async fn render_routing(state: &AppState) -> String {
    let config = state.config();
    let order = config.routing.provider_order.join(" → ");
    let default_model = config
        .routing
        .provider_order
        .first()
        .map(|_| "test-model")
        .unwrap_or("default");
    let content = format!(
        r#"
        <div class="grid gap-6">
            <div class="glass-card p-6">
                <h3 class="font-bold mb-4 text-emerald-400">Routing Configuration</h3>
                <div class="text-sm text-slate-400 mb-1 uppercase tracking-wider font-bold">Provider Fallback Order</div>
                <div class="font-mono bg-black/30 p-3 rounded text-cyan-400 mb-4">{}</div>
                <div class="flex gap-4"><span class="bg-white/5 px-3 py-1 rounded text-sm">Free First: <span class="font-bold">{}</span></span><span class="bg-white/5 px-3 py-1 rounded text-sm">Paid Fallback: <span class="font-bold">{}</span></span></div>
            </div>
            <div class="glass-card overflow-hidden">
                <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">Route Plan Explorer</h3></div>
                <div class="p-6">
                    <div class="flex gap-4 mb-4">
                        <input id="plan-model" value="{}" aria-label="Model" class="flex-1">
                        <select id="plan-endpoint" aria-label="Endpoint"><option value="chat">chat</option><option value="embeddings">embeddings</option></select>
                        <button class="btn" onclick="explainPlan()">Explain</button>
                    </div>
                    <div id="route-plan" class="bg-black/50 p-4 rounded text-sm text-slate-400 min-h-[100px] overflow-hidden">
                        <div class="text-center text-slate-500 py-4">Click explain to view the routing plan.</div>
                    </div>
                </div>
            </div>
        </div>"#,
        order, config.routing.free_first, config.routing.allow_paid_fallback, default_model
    );
    let scripts = r#"<script>
    async function explainPlan() { 
        const model = encodeURIComponent(document.getElementById('plan-model').value); 
        const endpoint = encodeURIComponent(document.getElementById('plan-endpoint').value); 
        const r = await fetch('/admin/route-plan?model='+model+'&endpoint='+endpoint); 
        const data = await r.json();
        
        let html = `<div class="mb-4 flex items-center gap-2">
            <span class="px-2 py-1 bg-white/5 rounded">Requested: <strong class="text-white">${data.requested_model}</strong></span>
            <i class="fas fa-arrow-right text-slate-600"></i>
            <span class="px-2 py-1 bg-white/5 rounded">Resolved: <strong class="text-emerald-400">${data.resolved_model}</strong></span>
        </div>
        <div class="space-y-2">`;
        
        if (data.attempts && data.attempts.length > 0) {
            data.attempts.forEach((a, i) => {
                const statusColor = a.eligible ? "text-emerald-500" : "text-slate-500";
                html += `<div class="flex items-center gap-3 p-3 bg-white/[0.02] border border-white/5 rounded">
                    <div class="w-6 h-6 rounded-full bg-black/40 flex items-center justify-center font-bold text-[10px]">${i+1}</div>
                    <div class="flex-1">
                        <div class="font-bold text-white">${a.provider} <span class="text-cyan-400 font-mono text-xs ml-2">${a.upstream_model}</span></div>
                        <div class="text-xs text-slate-500">Tier: ${a.tier}</div>
                    </div>
                    <div class="text-right">
                        <div class="font-bold text-[10px] uppercase tracking-wide ${statusColor}">${a.eligible ? 'Eligible' : 'Filtered'}</div>
                        ${a.reason ? `<div class="text-xs text-red-400">${a.reason}</div>` : ''}
                    </div>
                </div>`;
            });
        } else {
            html += `<div class="text-red-400 p-4 bg-red-400/10 rounded">No eligible routes found. Request would fail.</div>`;
        }
        html += `</div>`;
        document.getElementById('route-plan').innerHTML = html;
    }
    </script>"#;
    render_shell("Routing", "routing", &content, scripts, state)
}

/// Render the usage view.
pub async fn render_usage(state: &AppState) -> String {
    let series = crate::usage::aggregation::get_usage_series(state, "24h").await;
    let pricing = crate::usage::pricing_catalog::get_pricing_state(&state.db).await;
    let rows = match series.get("series").and_then(|s| s.as_array()) {
        Some(arr) => arr.iter().map(|entry| {
            let p = entry.get("provider_id").and_then(|v| v.as_str()).unwrap_or("?");
            let inp = entry.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let out = entry.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let cost = entry.get("estimated_cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
            format!(r#"<tr><td class="font-bold">{}</td><td class="font-mono">{}</td><td class="font-mono">{}</td><td class="font-mono text-emerald-400">${:.4}</td></tr>"#, p, inp, out, cost)
        }).collect::<Vec<_>>().join("\n"),
        None => "<tr><td colspan=\"4\" class=\"px-6 py-4 text-center text-slate-500\">No usage data</td></tr>".into(),
    };
    let pricing_rows = match pricing.get("rates").and_then(|r| r.as_array()) {
        Some(arr) if !arr.is_empty() => arr.iter().map(|rate| {
            let provider = rate.get("provider_id").and_then(|v| v.as_str()).unwrap_or("?");
            let model = rate.get("model_id").and_then(|v| v.as_str()).unwrap_or("?");
            let input = rate.get("input_per_1m").and_then(|v| v.as_f64()).map(|v| format!("${:.4}", v)).unwrap_or_else(|| "-".into());
            let cached = rate.get("cached_input_per_1m").and_then(|v| v.as_f64()).map(|v| format!("${:.4}", v)).unwrap_or_else(|| "-".into());
            let output = rate.get("output_per_1m").and_then(|v| v.as_f64()).map(|v| format!("${:.4}", v)).unwrap_or_else(|| "-".into());
            let confidence = rate.get("confidence").and_then(|v| v.as_str()).unwrap_or("unknown");
            let source = rate.get("source_kind").and_then(|v| v.as_str()).unwrap_or("unknown");
            format!(r#"<tr class="pricing-row" data-provider="{}" data-model="{}"><td class="font-bold">{}</td><td class="font-mono text-xs text-cyan-400">{}</td><td class="font-mono">{}</td><td class="font-mono">{}</td><td class="font-mono">{}</td><td><span class="px-2 py-0.5 rounded bg-white/5 text-[10px] uppercase">{}</span></td><td class="text-xs text-slate-400">{}</td></tr>"#, provider, model, provider, model, input, cached, output, confidence, source)
        }).collect::<Vec<_>>().join("\n"),
        _ => "<tr><td colspan=\"7\" class=\"px-6 py-4 text-center text-slate-500\">No pricing data</td></tr>".into(),
    };
    let source_rows = match pricing.get("sources").and_then(|s| s.as_array()) {
        Some(arr) if !arr.is_empty() => arr.iter().map(|source| {
            let provider = source.get("provider_id").and_then(|v| v.as_str()).unwrap_or("?");
            let kind = source.get("source_kind").and_then(|v| v.as_str()).unwrap_or("?");
            let status = source.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
            let last_success = source.get("last_success_at").and_then(|v| v.as_str()).unwrap_or("-");
            let error = source.get("last_error_summary").and_then(|v| v.as_str()).unwrap_or("");
            let color = if status == "ok" { "text-emerald-400" } else { "text-red-400" };
            format!(r#"<tr><td class="font-bold">{}</td><td class="font-mono text-xs">{}</td><td class="{} text-xs font-bold uppercase">{}</td><td class="font-mono text-xs">{}</td><td class="text-xs text-slate-500">{}</td></tr>"#, provider, kind, color, status, last_success, error)
        }).collect::<Vec<_>>().join("\n"),
        _ => "<tr><td colspan=\"5\" class=\"px-6 py-4 text-center text-slate-500\">No pricing sources checked yet</td></tr>".into(),
    };
    let content = format!(
        r#"
        <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">Usage Totals (Last 24h)</h3></div>
            <div class="p-0 overflow-x-auto">
                <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>Provider</th><th>Input Tokens</th><th>Output Tokens</th><th>Est. Cost</th></tr></thead><tbody class="divide-y divide-white/5">{}</tbody></table>
            </div>
        </div>
        <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02] flex items-center justify-between gap-4 flex-wrap">
                <h3 class="font-bold">Pricing Catalog</h3>
                <div class="flex items-center gap-3">
                    <input type="text" id="pricing-search" placeholder="Filter provider or model..." class="bg-white/5 border border-white/10 rounded px-3 py-1 text-sm w-48 focus:outline-none focus:border-cyan-400/50" oninput="filterPricing()">
                    <button class="btn" onclick="refreshPricing()">Refresh Pricing</button>
                </div>
            </div>
            <div class="p-0 overflow-x-auto">
                <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>Provider</th><th>Model</th><th>Input / 1M</th><th>Cached / 1M</th><th>Output / 1M</th><th>Confidence</th><th>Source</th></tr></thead><tbody id="pricing-tbody" class="divide-y divide-white/5">{}</tbody></table>
                <div id="pricing-empty" class="hidden px-6 py-4 text-center text-slate-500">No pricing rates match this filter</div>
            </div>
            <div id="pricing-pagination" class="px-6 py-3 border-t border-white/5 bg-white/[0.01] flex items-center justify-between text-sm text-slate-400">
                <span id="pricing-count"></span>
                <div class="flex items-center gap-2">
                    <button id="pricing-prev" class="btn text-xs" onclick="prevPricingPage()" disabled>&laquo; Prev</button>
                    <span id="pricing-page-indicator"></span>
                    <button id="pricing-next" class="btn text-xs" onclick="nextPricingPage()">&raquo; Next</button>
                </div>
            </div>
        </div>
        <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">Pricing Source Freshness</h3></div>
            <div class="p-0 overflow-x-auto">
                <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>Provider</th><th>Source</th><th>Status</th><th>Last Success</th><th>Last Error</th></tr></thead><tbody class="divide-y divide-white/5">{}</tbody></table>
            </div>
        </div>"#,
        rows, pricing_rows, source_rows
    );
    let scripts = r#"<script>
    async function refreshPricing() {
        const btn = event.target;
        const orig = btn.innerText;
        btn.disabled = true;
        btn.innerText = 'Refreshing...';
        try {
            const r = await fetch('/admin/pricing/refresh', { method: 'POST' });
            if (r.ok) location.reload();
            else showModal('Error', 'Pricing refresh failed', true);
        } finally {
            btn.disabled = false;
            btn.innerText = orig;
        }
    }

    let pricingPage = 0;
    const PRICING_PAGE_SIZE = 20;

    function allPricingRows() {
        const tbody = document.getElementById('pricing-tbody');
        if (!tbody) return [];
        return [...tbody.querySelectorAll('.pricing-row')];
    }

    function visiblePricingRows(rows) {
        const search = document.getElementById('pricing-search');
        const q = search ? search.value.trim().toLowerCase() : '';
        return rows.filter(row => {
            if (!q) return true;
            const prov = (row.dataset.provider || '').toLowerCase();
            const model = (row.dataset.model || '').toLowerCase();
            const text = row.innerText.toLowerCase();
            return prov.includes(q) || model.includes(q) || text.includes(q);
        });
    }

    function renderPricingPage() {
        try {
            const rows = allPricingRows();
            const filtered = visiblePricingRows(rows);
            const totalPages = Math.max(1, Math.ceil(filtered.length / PRICING_PAGE_SIZE));
            if (pricingPage >= totalPages) pricingPage = totalPages - 1;
            rows.forEach(r => { r.style.display = 'none'; });
            const start = pricingPage * PRICING_PAGE_SIZE;
            const page = filtered.slice(start, start + PRICING_PAGE_SIZE);
            page.forEach(r => { r.style.display = ''; });
            const countEl = document.getElementById('pricing-count');
            const indicatorEl = document.getElementById('pricing-page-indicator');
            const prevEl = document.getElementById('pricing-prev');
            const nextEl = document.getElementById('pricing-next');
            const paginationEl = document.getElementById('pricing-pagination');
            const emptyEl = document.getElementById('pricing-empty');
            if (countEl) countEl.innerText = filtered.length + ' rate' + (filtered.length !== 1 ? 's' : '');
            if (indicatorEl) indicatorEl.innerText = 'Page ' + (pricingPage + 1) + ' / ' + totalPages;
            if (prevEl) prevEl.disabled = pricingPage === 0;
            if (nextEl) nextEl.disabled = pricingPage >= totalPages - 1;
            if (paginationEl) paginationEl.style.display = filtered.length <= PRICING_PAGE_SIZE ? 'none' : '';
            if (emptyEl) emptyEl.classList.toggle('hidden', filtered.length !== 0);
        } catch(e) {
            console.error('renderPricingPage error:', e);
        }
    }

    function filterPricing() {
        pricingPage = 0;
        renderPricingPage();
    }
    function prevPricingPage() { if (pricingPage > 0) { pricingPage--; renderPricingPage(); } }
    function nextPricingPage() {
        const totalPages = Math.max(1, Math.ceil(visiblePricingRows(allPricingRows()).length / PRICING_PAGE_SIZE));
        if (pricingPage < totalPages - 1) {
            pricingPage++;
            renderPricingPage();
        }
    }

    renderPricingPage();
    </script>"#;
    render_shell("Usage", "usage", &content, scripts, state)
}

pub async fn render_projects(state: &AppState) -> String {
    let projects = crate::projects::list_projects(state)
        .await
        .unwrap_or_else(|_| serde_json::json!({ "projects": [] }));
    let rows = projects
        .get("projects")
        .and_then(|value| value.as_array())
        .map(|projects| {
            projects
                .iter()
                .map(|project| {
                    let id = escape_html(project.get("project_id").and_then(|v| v.as_str()).unwrap_or("?"));
                    let name = escape_html(project.get("display_name").and_then(|v| v.as_str()).unwrap_or("?"));
                    let enabled = project.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                    let privacy = escape_html(project.get("privacy_profile").and_then(|v| v.as_str()).unwrap_or("default"));
                    let allow_paid = project.get("allow_paid_fallback").and_then(|v| v.as_bool()).unwrap_or(false);
                    let keys = project.get("keys").and_then(|v| v.as_array()).map(|keys| keys.len()).unwrap_or(0);
                    let cost_day = project.get("max_cost_per_day_usd").and_then(|v| v.as_f64()).map(|v| format!("${v:.2}")).unwrap_or_else(|| "-".into());
                    let status = if enabled { "Enabled" } else { "Disabled" };
                    let status_class = if enabled { "text-emerald-400" } else { "text-red-400" };
                    format!(r#"<tr>
                        <td><div class="font-bold">{}</div><div class="font-mono text-[10px] text-slate-500">{}</div></td>
                        <td><span class="{} text-xs font-bold uppercase">{}</span></td>
                        <td class="font-mono text-xs">{}</td>
                        <td class="font-mono text-xs">{}</td>
                        <td class="font-mono text-xs">{}</td>
                        <td class="font-mono text-xs">{}</td>
                        <td class="text-right">
                            <button class="btn text-xs" onclick="issueKey('{}')">Issue key</button>
                            <a class="btn text-xs" href="/admin/projects/{}/export.csv">CSV</a>
                            <a class="btn text-xs" href="/admin/projects/{}/diagnostics/bundle">Diagnostics</a>
                        </td>
                    </tr>"#, name, id, status_class, status, privacy, if allow_paid { "paid ok" } else { "free/local" }, keys, cost_day, id, id, id)
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|rows| !rows.is_empty())
        .unwrap_or_else(|| "<tr><td colspan=\"7\" class=\"px-6 py-4 text-center text-slate-500\">No projects configured</td></tr>".into());

    let content = format!(
        r#"
        <div class="grid grid-cols-1 xl:grid-cols-[1fr_24rem] gap-6">
            <div class="glass-card overflow-hidden">
                <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02] flex items-center justify-between gap-4 flex-wrap">
                    <h3 class="font-bold">Projects</h3>
                    <button class="btn" onclick="createProject()">Create Project</button>
                </div>
                <div class="p-0 overflow-x-auto">
                    <table class="w-full text-left">
                        <thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]">
                            <tr><th>Project</th><th>Status</th><th>Privacy</th><th>Paid</th><th>Keys</th><th>Daily Cost Cap</th><th class="text-right">Actions</th></tr>
                        </thead>
                        <tbody class="divide-y divide-white/5">{}</tbody>
                    </table>
                </div>
            </div>
            <div class="glass-card p-5 space-y-4">
                <h3 class="font-bold">New Project</h3>
                <label class="block"><span class="text-xs font-bold text-slate-500 uppercase">Name</span><input id="project-name" class="mt-2 w-full" placeholder="staging"></label>
                <label class="block project-model-group-field">
                    <span class="text-xs font-bold text-slate-500 uppercase">Allowed Model Groups</span>
                    <div id="project-model-group-select" class="project-combobox mt-2" onclick="focusProjectModelSearch()">
                        <div id="project-model-chips" class="project-combobox-chips"></div>
                        <input id="project-model-search" class="project-combobox-input" type="text" placeholder="Search model groups" autocomplete="off" role="combobox" aria-controls="project-model-options" aria-expanded="false" onfocus="filterProjectModelGroups()" oninput="filterProjectModelGroups()" onkeydown="handleProjectModelKeydown(event)">
                    </div>
                    <div id="project-model-options" class="project-combobox-options hidden" role="listbox"></div>
                </label>
                <label class="block"><span class="text-xs font-bold text-slate-500 uppercase">Daily Cost Cap USD</span><input id="project-cost-day" type="number" min="0" step="0.01" class="mt-2 w-full" placeholder="2.00"></label>
                <label class="block"><span class="text-xs font-bold text-slate-500 uppercase">Privacy Profile</span>
                    <select id="project-privacy" class="mt-2 w-full">
                        <option value="default">default</option>
                        <option value="free_only">free_only</option>
                        <option value="local_only">local_only</option>
                    </select>
                </label>
                <label class="toggle-row">
                    <input id="project-paid" class="toggle-input" type="checkbox">
                    <span class="toggle-track" aria-hidden="true"><span class="toggle-thumb"></span></span>
                    <span class="toggle-copy">
                        <span>Allow paid fallback</span>
                        <small>Project paid providers</small>
                    </span>
                </label>
            </div>
        </div>
        "#,
        rows
    );
    let scripts = r#"<script>
    let projectModelGroups = [];
    let selectedProjectModelGroups = [];
    let highlightedProjectModelIndex = -1;

    function escapeProjectHtml(value) {
        return String(value).replace(/[&<>"']/g, ch => ({
            '&': '&amp;',
            '<': '&lt;',
            '>': '&gt;',
            '"': '&quot;',
            "'": '&#39;'
        })[ch]);
    }
    function focusProjectModelSearch() {
        document.getElementById('project-model-search').focus();
    }
    function projectModelCandidates() {
        const query = document.getElementById('project-model-search').value.trim().toLowerCase();
        return projectModelGroups
            .filter(group => !selectedProjectModelGroups.includes(group))
            .filter(group => !query || group.toLowerCase().includes(query))
            .slice(0, 12);
    }
    function renderProjectModelChips() {
        const chips = document.getElementById('project-model-chips');
        chips.innerHTML = selectedProjectModelGroups.map(group => `
            <span class="project-combobox-chip">
                <span>${escapeProjectHtml(group)}</span>
                <button type="button" onclick="removeProjectModelGroup('${encodeURIComponent(group)}')" aria-label="Remove ${escapeProjectHtml(group)}">&times;</button>
            </span>
        `).join('');
    }
    function renderProjectModelOptions(candidates) {
        const options = document.getElementById('project-model-options');
        const search = document.getElementById('project-model-search');
        if (candidates.length === 0) {
            options.classList.add('hidden');
            search.setAttribute('aria-expanded', 'false');
            highlightedProjectModelIndex = -1;
            return;
        }
        if (highlightedProjectModelIndex >= candidates.length) highlightedProjectModelIndex = candidates.length - 1;
        options.innerHTML = candidates.map((group, index) => `
            <button type="button" class="project-combobox-option ${index === highlightedProjectModelIndex ? 'active' : ''}" role="option" onclick="selectProjectModelGroup('${encodeURIComponent(group)}')">
                <span>${escapeProjectHtml(group)}</span>
            </button>
        `).join('');
        options.classList.remove('hidden');
        search.setAttribute('aria-expanded', 'true');
    }
    function filterProjectModelGroups() {
        highlightedProjectModelIndex = -1;
        renderProjectModelOptions(projectModelCandidates());
    }
    function selectProjectModelGroup(encodedGroup) {
        const group = decodeURIComponent(encodedGroup);
        if (!selectedProjectModelGroups.includes(group)) {
            selectedProjectModelGroups.push(group);
            selectedProjectModelGroups.sort();
        }
        document.getElementById('project-model-search').value = '';
        renderProjectModelChips();
        renderProjectModelOptions(projectModelCandidates());
        focusProjectModelSearch();
    }
    function removeProjectModelGroup(encodedGroup) {
        const group = decodeURIComponent(encodedGroup);
        selectedProjectModelGroups = selectedProjectModelGroups.filter(item => item !== group);
        renderProjectModelChips();
        renderProjectModelOptions(projectModelCandidates());
    }
    function handleProjectModelKeydown(event) {
        const candidates = projectModelCandidates();
        if (event.key === 'ArrowDown') {
            event.preventDefault();
            highlightedProjectModelIndex = Math.min(candidates.length - 1, highlightedProjectModelIndex + 1);
            renderProjectModelOptions(candidates);
        } else if (event.key === 'ArrowUp') {
            event.preventDefault();
            highlightedProjectModelIndex = Math.max(0, highlightedProjectModelIndex - 1);
            renderProjectModelOptions(candidates);
        } else if (event.key === 'Enter') {
            if (highlightedProjectModelIndex >= 0 && candidates[highlightedProjectModelIndex]) {
                event.preventDefault();
                selectProjectModelGroup(encodeURIComponent(candidates[highlightedProjectModelIndex]));
            }
        } else if (event.key === 'Backspace' && event.target.value === '' && selectedProjectModelGroups.length > 0) {
            selectedProjectModelGroups.pop();
            renderProjectModelChips();
        } else if (event.key === 'Escape') {
            document.getElementById('project-model-options').classList.add('hidden');
            event.target.setAttribute('aria-expanded', 'false');
        }
    }
    async function loadProjectModelGroupOptions() {
        try {
            const resp = await fetch('/admin/model-groups');
            if (!resp.ok) return;
            const groups = await resp.json();
            projectModelGroups = groups
                .filter(group => group.enabled !== false)
                .map(group => group.name)
                .filter(Boolean)
                .sort();
        } catch (e) {
            projectModelGroups = [];
        }
        renderProjectModelChips();
    }
    document.addEventListener('click', (event) => {
        const field = document.querySelector('.project-model-group-field');
        if (field && !field.contains(event.target)) {
            document.getElementById('project-model-options').classList.add('hidden');
            document.getElementById('project-model-search').setAttribute('aria-expanded', 'false');
        }
    });
    async function createProject() {
        const name = document.getElementById('project-name').value.trim();
        if (!name) { showModal('Project name required', 'Enter a project name.', true); return; }
        const cap = document.getElementById('project-cost-day').value;
        const body = {
            display_name: name,
            allowed_model_groups: selectedProjectModelGroups,
            privacy_profile: document.getElementById('project-privacy').value,
            allow_paid_fallback: document.getElementById('project-paid').checked
        };
        if (cap) body.max_cost_per_day_usd = Number(cap);
        const r = await fetch('/admin/projects', {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(body)});
        const data = await r.json().catch(() => ({}));
        if (!r.ok) { showModal('Project create failed', data, true); return; }
        location.reload();
    }
    async function issueKey(projectId) {
        const label = prompt('Key label');
        if (!label) return;
        const r = await fetch(`/admin/projects/${projectId}/keys`, {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({label})});
        const data = await r.json().catch(() => ({}));
        if (!r.ok) { showModal('Key issue failed', data, true); return; }
        showModal('Project API key', data.api_key + '\n\nStore it now. It will not be shown again.', false);
    }
    loadProjectModelGroupOptions();
    </script>"#;
    render_shell("Projects", "projects", &content, scripts, state)
}

/// Render the health view.
pub async fn render_health(state: &AppState) -> String {
    let mut rows = String::new();
    for entry in state.health_states.iter() {
        let pid = entry.key();
        let hs = entry.value();
        rows.push_str(&format!(r#"<tr><td class="font-bold">{}</td><td class="text-cyan-400 text-sm">{:?}</td><td class="font-mono">{}</td></tr>"#, pid, hs.state, hs.recent_successes + hs.recent_failures));
    }
    let content = format!(
        r#"
        <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">Provider Health States</h3></div>
            <div class="p-0 overflow-x-auto">
                <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>Provider</th><th>State</th><th>Total Requests</th></tr></thead><tbody class="divide-y divide-white/5">{}</tbody></table>
            </div>
        </div>"#,
        rows
    );
    render_shell("Health", "health", &content, "", state)
}

/// Render the observability and incident workflow view.
pub async fn render_observability(state: &AppState) -> String {
    let summary = crate::observability::get_observability_summary(state, "24h").await;
    let traces = crate::observability::get_request_traces(state, 20).await;
    let incidents = crate::observability::get_incidents(state, 20).await;

    let trace_rows = traces
        .get("traces")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .map(|trace| {
                    let request_id = escape_html(
                        trace
                            .get("request_id")
                            .and_then(|value| value.as_str())
                            .unwrap_or("?"),
                    );
                    let status = escape_html(
                        trace
                            .get("status")
                            .and_then(|value| value.as_str())
                            .unwrap_or("unknown"),
                    );
                    let model = escape_html(
                        trace
                            .get("requested_model")
                            .and_then(|value| value.as_str())
                            .unwrap_or("?"),
                    );
                    let provider = escape_html(
                        trace
                            .get("selected_provider_id")
                            .and_then(|value| value.as_str())
                            .unwrap_or("-"),
                    );
                    let latency = trace
                        .get("latency_ms")
                        .and_then(|value| value.as_i64())
                        .unwrap_or(0);
                    let status_class = if status == "success" {
                        "text-emerald-400"
                    } else if trace.get("http_status").and_then(|value| value.as_i64()) == Some(429)
                    {
                        "text-yellow-400"
                    } else {
                        "text-red-400"
                    };
                    format!(
                        r#"<tr><td class="font-mono text-xs text-cyan-400">{}</td><td class="font-mono text-xs">{}</td><td>{}</td><td>{}</td><td class="{} font-bold text-xs uppercase">{}</td><td class="font-mono">{}ms</td><td><button class="btn text-xs" data-request-id="{}" onclick="loadTraceFromButton(this)">Open</button></td></tr>"#,
                        request_id, model, provider, trace["endpoint_kind"], status_class, status, latency, request_id
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|rows| !rows.is_empty())
        .unwrap_or_else(|| {
            r#"<tr><td colspan="7" class="px-6 py-4 text-center text-slate-500">No request traces yet</td></tr>"#
                .to_string()
        });

    let incident_rows = incidents
        .get("incidents")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .map(|incident| {
                    let severity = incident
                        .get("severity")
                        .and_then(|value| value.as_str())
                        .unwrap_or("info");
                    let severity_class = match severity {
                        "critical" => "text-red-400 bg-red-500/10",
                        "warning" => "text-yellow-400 bg-yellow-500/10",
                        _ => "text-cyan-300 bg-cyan-500/10",
                    };
                    let title = escape_html(
                        incident
                            .get("title")
                            .and_then(|value| value.as_str())
                            .unwrap_or("incident"),
                    );
                    let kind = escape_html(
                        incident
                            .get("kind")
                            .and_then(|value| value.as_str())
                            .unwrap_or("event"),
                    );
                    let recorded_at = escape_html(
                        incident
                            .get("recorded_at")
                            .and_then(|value| value.as_str())
                            .unwrap_or("-"),
                    );
                    format!(
                        r#"<tr><td class="font-mono text-xs">{}</td><td><span class="px-2 py-0.5 rounded text-[10px] uppercase {}">{}</span></td><td class="text-xs text-slate-400">{}</td><td class="font-bold">{}</td></tr>"#,
                        recorded_at, severity_class, severity, kind, title
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|rows| !rows.is_empty())
        .unwrap_or_else(|| {
            r#"<tr><td colspan="4" class="px-6 py-4 text-center text-slate-500">No incidents recorded</td></tr>"#
                .to_string()
        });

    let request_count = summary
        .get("request_count")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);
    let success_rate = summary
        .get("success_rate")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
        * 100.0;
    let rate_limit_rate = summary
        .get("rate_limit_rate")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
        * 100.0;
    let fallback_count = summary
        .get("fallback_count")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);
    let total_tokens = summary
        .get("total_tokens")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);
    let estimated_cost = summary
        .get("estimated_cost_usd")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);

    let content = format!(
        r#"
        <div class="flex items-center justify-between gap-4 mb-2">
            <div class="text-xs font-bold text-slate-500 uppercase tracking-widest">Incident Console</div>
            <div class="flex gap-2">
                <select id="observability-period" class="bg-white/5 border border-white/10 rounded px-3 py-2 text-sm" onchange="refreshObservability()">
                    <option value="24h">24H</option>
                    <option value="7d">7D</option>
                    <option value="30d">30D</option>
                </select>
                <button class="btn" onclick="downloadDiagnosticBundle()">Export Bundle</button>
            </div>
        </div>
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-6 gap-4">
            <div class="glass-card p-5"><div class="text-xs font-bold text-slate-500 uppercase">Requests</div><div id="obs-requests" class="text-3xl font-bold mt-2">{}</div></div>
            <div class="glass-card p-5"><div class="text-xs font-bold text-slate-500 uppercase">Success</div><div id="obs-success" class="text-3xl font-bold mt-2 text-emerald-400">{:.1}%</div></div>
            <div class="glass-card p-5"><div class="text-xs font-bold text-slate-500 uppercase">429 Rate</div><div id="obs-rate-limit" class="text-3xl font-bold mt-2 text-yellow-400">{:.1}%</div></div>
            <div class="glass-card p-5"><div class="text-xs font-bold text-slate-500 uppercase">Fallbacks</div><div id="obs-fallbacks" class="text-3xl font-bold mt-2">{}</div></div>
            <div class="glass-card p-5"><div class="text-xs font-bold text-slate-500 uppercase">Tokens</div><div id="obs-tokens" class="text-3xl font-bold mt-2">{}</div></div>
            <div class="glass-card p-5"><div class="text-xs font-bold text-slate-500 uppercase">Cost</div><div id="obs-cost" class="text-3xl font-bold mt-2">{}</div></div>
        </div>
        <div class="grid grid-cols-1 xl:grid-cols-2 gap-6">
            <div class="glass-card overflow-hidden">
                <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">Request Traces</h3></div>
                <div class="overflow-x-auto">
                    <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>Request</th><th>Model</th><th>Provider</th><th>Endpoint</th><th>Status</th><th>Latency</th><th>Trace</th></tr></thead><tbody id="trace-rows" class="divide-y divide-white/5">{}</tbody></table>
                </div>
            </div>
            <div class="glass-card overflow-hidden">
                <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">Incident Feed</h3></div>
                <div class="overflow-x-auto">
                    <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>Time</th><th>Severity</th><th>Kind</th><th>Event</th></tr></thead><tbody id="incident-rows" class="divide-y divide-white/5">{}</tbody></table>
                </div>
            </div>
        </div>
        <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02] flex items-center justify-between"><h3 class="font-bold">Trace Timeline</h3><span id="trace-title" class="font-mono text-xs text-slate-500">Select a request</span></div>
            <pre id="trace-detail" class="p-4 bg-[#010409] font-mono text-[11px] leading-relaxed overflow-x-auto text-slate-300 min-h-[220px]"></pre>
        </div>"#,
        format_number(request_count),
        success_rate,
        rate_limit_rate,
        format_number(fallback_count),
        format_number(total_tokens),
        format_currency(estimated_cost),
        trace_rows,
        incident_rows
    );

    let scripts = r#"<script>
    function htmlEscape(value) {
        return String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
    }
    function money(value) {
        return new Intl.NumberFormat('en-US', {style:'currency', currency:'USD', minimumFractionDigits:2, maximumFractionDigits:4}).format(Number(value || 0));
    }
    function traceRow(trace) {
        const status = trace.status || 'unknown';
        const cls = status === 'success' ? 'text-emerald-400' : (trace.http_status === 429 ? 'text-yellow-400' : 'text-red-400');
        const id = htmlEscape(trace.request_id);
        return `<tr><td class="font-mono text-xs text-cyan-400">${id}</td><td class="font-mono text-xs">${htmlEscape(trace.requested_model)}</td><td>${htmlEscape(trace.selected_provider_id || '-')}</td><td>${htmlEscape(trace.endpoint_kind)}</td><td class="${cls} font-bold text-xs uppercase">${htmlEscape(status)}</td><td class="font-mono">${trace.latency_ms || 0}ms</td><td><button class="btn text-xs" data-request-id="${id}" onclick="loadTraceFromButton(this)">Open</button></td></tr>`;
    }
    function incidentRow(incident) {
        const severity = incident.severity || 'info';
        const cls = severity === 'critical' ? 'text-red-400 bg-red-500/10' : (severity === 'warning' ? 'text-yellow-400 bg-yellow-500/10' : 'text-cyan-300 bg-cyan-500/10');
        return `<tr><td class="font-mono text-xs">${htmlEscape(incident.recorded_at || '-')}</td><td><span class="px-2 py-0.5 rounded text-[10px] uppercase ${cls}">${htmlEscape(severity)}</span></td><td class="text-xs text-slate-400">${htmlEscape(incident.kind)}</td><td class="font-bold">${htmlEscape(incident.title)}</td></tr>`;
    }
    async function refreshObservability() {
        const period = document.getElementById('observability-period').value;
        const [summary, traces, incidents] = await Promise.all([
            fetch(`/admin/observability/summary?period=${period}`).then(r => r.json()),
            fetch('/admin/request-traces?limit=20').then(r => r.json()),
            fetch('/admin/incidents?limit=20').then(r => r.json())
        ]);
        document.getElementById('obs-requests').innerText = Number(summary.request_count || 0).toLocaleString();
        document.getElementById('obs-success').innerText = ((summary.success_rate || 0) * 100).toFixed(1) + '%';
        document.getElementById('obs-rate-limit').innerText = ((summary.rate_limit_rate || 0) * 100).toFixed(1) + '%';
        document.getElementById('obs-fallbacks').innerText = Number(summary.fallback_count || 0).toLocaleString();
        document.getElementById('obs-tokens').innerText = Number(summary.total_tokens || 0).toLocaleString();
        document.getElementById('obs-cost').innerText = money(summary.estimated_cost_usd);
        document.getElementById('trace-rows').innerHTML = (traces.traces || []).map(traceRow).join('') || '<tr><td colspan="7" class="px-6 py-4 text-center text-slate-500">No request traces yet</td></tr>';
        document.getElementById('incident-rows').innerHTML = (incidents.incidents || []).map(incidentRow).join('') || '<tr><td colspan="4" class="px-6 py-4 text-center text-slate-500">No incidents recorded</td></tr>';
    }
    async function loadTrace(requestId) {
        const response = await fetch('/admin/request-traces/' + encodeURIComponent(requestId));
        const detail = await response.json();
        document.getElementById('trace-title').innerText = requestId;
        document.getElementById('trace-detail').innerText = JSON.stringify(detail, null, 2);
    }
    function loadTraceFromButton(button) {
        loadTrace(button.dataset.requestId || '');
    }
    async function downloadDiagnosticBundle() {
        const bundle = await fetch('/admin/diagnostics/bundle').then(r => r.json());
        const blob = new Blob([JSON.stringify(bundle, null, 2)], {type:'application/json'});
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `tokenscavenger-diagnostics-${new Date().toISOString().replace(/[:.]/g, '-')}.json`;
        a.click();
        URL.revokeObjectURL(url);
    }
    setInterval(refreshObservability, 30000);
    </script>"#;
    render_shell("Observability", "observability", &content, scripts, state)
}

/// Render the logs view.
pub async fn render_logs(state: &AppState) -> String {
    let content = r#"
        <div class="glass-card overflow-hidden flex flex-col h-[calc(100vh-12rem)]">
            <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">System Log Stream</h3></div>
            <div id="logs" aria-live="polite" class="flex-1 p-4 bg-[#010409] font-mono text-[10px] leading-relaxed overflow-y-auto text-slate-400">
            </div>
        </div>"#;
    let scripts = r#"<script>
      const es=new EventSource('/admin/logs/stream');
      es.onmessage=(e)=>{ const el=document.getElementById('logs'); const line=document.createElement('div'); line.className="mb-1"; line.textContent=e.data; el.appendChild(line); el.scrollTop=el.scrollHeight; };
    </script>"#;
    render_shell("Logs", "logs", content, scripts, state)
}

/// Render the configuration view.
pub async fn render_config(state: &AppState) -> String {
    let snapshots = sqlx::query_as::<_, (i64, String, String)>("SELECT id, created_at, source FROM config_snapshots ORDER BY id DESC LIMIT 20")
        .fetch_all(&state.db).await.unwrap_or_default().into_iter()
        .map(|(id, created_at, source)| format!(r#"<tr><td class="font-mono text-cyan-400">{}</td><td class="text-sm">{}</td><td class="text-sm">{}</td><td><button class="btn" style="background:#334155;" onclick="rollback({})">Rollback</button></td></tr>"#, id, created_at, source, id))
        .collect::<Vec<_>>().join("\n");

    let models = crate::discovery::merge::get_all_models(state).await;
    let models_json = serde_json::to_string(&models).unwrap_or("{}".into());
    let content = format!(
        r#"
        <div class="grid gap-6">
            <div class="glass-card overflow-hidden flex flex-col">
                <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02] flex items-center justify-between">
                    <h3 class="font-bold">Raw JSON Configuration</h3>
                    <button class="btn" onclick="saveConfig()">Deploy Configuration</button>
                </div>
                <div class="p-4 bg-black/50">
                    <textarea id="raw-config" class="w-full h-[400px] font-mono text-[10px] p-4 bg-transparent border-0 text-cyan-400 focus:ring-0" spellcheck="false">Loading...</textarea>
                </div>
            </div>
            <div class="glass-card p-6">
                <h3 class="font-bold mb-4 text-emerald-400">Model Group Editor</h3>
                <div class="flex flex-col gap-4">
                    <div class="flex gap-4">
                        <input id="model-group-name" placeholder="group name (e.g. 'fast-chat')" aria-label="Model group name" class="w-1/3">
                        <div class="flex-1 relative">
                            <div id="target-tags" class="flex flex-wrap gap-2 p-2 min-h-[42px] bg-black/20 border border-white/10 rounded items-center">
                                <select id="target-mode" aria-label="Target selection mode" class="bg-black/30 border border-white/10 rounded px-2 py-1 text-xs text-slate-300 focus:ring-1 focus:ring-cyan-500">
                                    <option value="any">Any provider</option>
                                    <option value="provider">Specific provider</option>
                                </select>
                                <input id="target-search" placeholder="Search and add models..." class="flex-1 bg-transparent border-0 p-0 focus:ring-0 text-sm min-w-[150px]">
                            </div>
                            <div id="target-dropdown" class="absolute left-0 right-0 top-full mt-1 dropdown-opaque border border-white/10 rounded-md shadow-xl z-50 max-h-60 overflow-y-auto hidden">
                                <!-- Dropdown items -->
                            </div>
                        </div>
                        <button id="save-model-group-btn" class="btn h-[42px] flex items-center justify-center min-w-[150px]" onclick="saveModelGroup()">
                            <span class="btn-text">Save Group</span>
                        </button>
                        <button id="cancel-edit-btn" class="btn h-[42px] hidden" style="background:#334155;" onclick="cancelEdit()">Cancel</button>
                    </div>
                    <p class="text-[10px] text-slate-500">Model groups map one public model name to ordered targets. Use Any provider for portable model IDs, or Specific provider to pin a target to one upstream.</p>
                </div>
            </div>

            <!-- Model Group Management List -->
            <div class="glass-card overflow-hidden">
                <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02] flex items-center justify-between">
                    <h3 class="font-bold text-sm">Configured Model Groups</h3>
                </div>
                <div class="p-0 overflow-x-auto">
                    <table class="w-full text-left">
                        <thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]">
                            <tr><th>Group Name</th><th>Target Models</th><th class="text-right px-6">Actions</th></tr>
                        </thead>
                        <tbody id="model-group-list-body" class="divide-y divide-white/5">
                            <tr><td colspan="3" class="text-center text-slate-500 py-8">Loading model groups...</td></tr>
                        </tbody>
                    </table>
                </div>
            </div>
            <div class="glass-card overflow-hidden">
                <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">Configuration Rollback</h3></div>
                <div class="p-0 overflow-x-auto">
                    <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>ID</th><th>Created</th><th>Source</th><th>Action</th></tr></thead><tbody class="divide-y divide-white/5">{}</tbody></table>
                </div>
            </div>
        </div>"#,
        snapshots
    );

    let scripts = format!(
        r#"<script>
    const modelsPayload = {};
    const allModels = modelsPayload.models || [];
    let selectedModels = [];

    const searchInput = document.getElementById('target-search');
    const targetMode = document.getElementById('target-mode');
    const dropdown = document.getElementById('target-dropdown');
    const tagsContainer = document.getElementById('target-tags');

    function normalizeTarget(target) {{
        if (typeof target === 'string') return {{ provider_id: null, model_id: target }};
        return {{
            provider_id: target.provider_id || target.provider || null,
            model_id: target.model_id || target.model || target.upstream_model_id || ''
        }};
    }}

    function targetKey(target) {{
        const t = normalizeTarget(target);
        return `${{t.provider_id || '*'}}::${{t.model_id}}`;
    }}

    function targetLabel(target) {{
        const t = normalizeTarget(target);
        return t.provider_id ? `${{t.provider_id}} / ${{t.model_id}}` : `Any / ${{t.model_id}}`;
    }}

    function targetPayload(target) {{
        const t = normalizeTarget(target);
        return t.provider_id ? {{ provider: t.provider_id, model: t.model_id }} : t.model_id;
    }}

    function encodeTarget(target) {{
        return encodeURIComponent(JSON.stringify(targetPayload(target)));
    }}

    function decodeTarget(encoded) {{
        return normalizeTarget(JSON.parse(decodeURIComponent(encoded)));
    }}

    function renderTags() {{
        const existingTags = tagsContainer.querySelectorAll('.model-tag');
        existingTags.forEach(t => t.remove());
        selectedModels.forEach(m => {{
            const target = normalizeTarget(m);
            const tag = document.createElement('span');
            tag.className = target.provider_id
                ? 'model-tag px-2 py-1 bg-emerald-500/10 text-emerald-300 text-xs rounded border border-emerald-500/20 flex items-center gap-2'
                : 'model-tag px-2 py-1 bg-cyan-500/10 text-cyan-400 text-xs rounded border border-cyan-500/20 flex items-center gap-2';
            tag.innerHTML = `<span class="font-mono">${{targetLabel(target)}}</span> <i class="fas fa-times cursor-pointer hover:text-white" onclick="removeModel('${{targetKey(target)}}')"></i>`;
            tagsContainer.insertBefore(tag, targetMode);
        }});
    }}

    function removeModel(key) {{
        selectedModels = selectedModels.filter(x => targetKey(x) !== key);
        renderTags();
    }}

    function addModel(encodedTarget) {{
        const target = decodeTarget(encodedTarget);
        if (!target.model_id) return;
        if (!selectedModels.some(m => targetKey(m) === targetKey(target))) {{
            selectedModels.push(target);
            renderTags();
        }}
        searchInput.value = '';
        dropdown.classList.add('hidden');
    }}

    function cancelEdit() {{
        document.getElementById('model-group-name').value = '';
        document.getElementById('model-group-name').disabled = false;
        selectedModels = [];
        targetMode.value = 'any';
        renderTags();
        document.getElementById('save-model-group-btn').querySelector('.btn-text').innerText = 'Save Group';
        document.getElementById('cancel-edit-btn').classList.add('hidden');
    }}

    function editModelGroup(name, encodedTargets) {{
        document.getElementById('model-group-name').value = name;
        document.getElementById('model-group-name').disabled = true;
        selectedModels = JSON.parse(decodeURIComponent(encodedTargets)).map(normalizeTarget).filter(t => t.model_id);
        renderTags();
        document.getElementById('save-model-group-btn').querySelector('.btn-text').innerText = 'Update Group';
        document.getElementById('cancel-edit-btn').classList.remove('hidden');
        document.getElementById('model-group-name').scrollIntoView({{ behavior: 'smooth' }});
    }}

    searchInput.onfocus = () => showDropdown(searchInput.value);
    searchInput.oninput = (e) => showDropdown(e.target.value);
    targetMode.onchange = () => showDropdown(searchInput.value);
    
    document.addEventListener('click', (e) => {{
        if (!tagsContainer.contains(e.target) && !dropdown.contains(e.target)) {{
            dropdown.classList.add('hidden');
        }}
    }});

    function showDropdown(query) {{
        const q = query.toLowerCase();
        const mode = targetMode.value;
        let candidates = allModels.filter(m =>
            (m.upstream_model_id.toLowerCase().includes(q) || m.provider_id.toLowerCase().includes(q))
        );
        if (mode === 'any') {{
            const seen = new Set();
            candidates = candidates.filter(m => {{
                if (seen.has(m.upstream_model_id)) return false;
                seen.add(m.upstream_model_id);
                return !selectedModels.some(target => targetKey(target) === targetKey({{ model_id: m.upstream_model_id }}));
            }});
        }} else {{
            candidates = candidates.filter(m =>
                !selectedModels.some(target => targetKey(target) === targetKey({{ provider_id: m.provider_id, model_id: m.upstream_model_id }}))
            );
        }}
        const filtered = candidates.slice(0, 50);

        if (filtered.length === 0) {{
            dropdown.classList.add('hidden');
            return;
        }}

        dropdown.innerHTML = filtered.map(m => {{
            const target = mode === 'any'
                ? {{ model_id: m.upstream_model_id }}
                : {{ provider_id: m.provider_id, model_id: m.upstream_model_id }};
            return `
            <div class="px-4 py-2 hover:bg-white/5 cursor-pointer flex justify-between items-center gap-4 group" onclick="addModel('${{encodeTarget(target)}}')">
                <span class="text-sm text-slate-200 font-mono truncate">${{m.upstream_model_id}}</span>
                <span class="text-[10px] text-slate-500 group-hover:text-cyan-400 shrink-0">${{mode === 'any' ? 'Any provider' : m.provider_id}}</span>
            </div>
        `}}).join('');
        dropdown.classList.remove('hidden');
    }}

    async function loadModelGroups() {{
        const body = document.getElementById('model-group-list-body');
        try {{
            const resp = await fetch('/admin/model-groups');
            if (!resp.ok) throw new Error('Failed to load');
            const modelGroups = await resp.json();
            
            if (modelGroups.length === 0) {{
                body.innerHTML = '<tr><td colspan="3" class="text-center text-slate-500 py-8">No model groups configured.</td></tr>';
                return;
            }}

            body.innerHTML = modelGroups.map(group => {{
                const targets = Array.isArray(group.target) ? group.target : [group.target];
                const encodedTargets = encodeURIComponent(JSON.stringify(targets));
                return `
                <tr>
                    <td class="px-6 py-4 font-mono text-cyan-400">${{group.name}}</td>
                    <td class="px-6 py-4">
                        <div class="flex flex-wrap gap-1">
                            ${{targets.map(t => {{
                                const target = normalizeTarget(t);
                                const classes = target.provider_id ? 'text-emerald-300 border-emerald-500/20' : 'text-cyan-300 border-cyan-500/20';
                                return `<span class="text-[10px] bg-white/5 px-2 py-0.5 rounded border ${{classes}} font-mono">${{targetLabel(target)}}</span>`;
                            }}).join('')}}
                        </div>
                    </td>
                    <td class="px-6 py-4 text-right">
                        <div class="flex justify-end gap-2">
                            <button onclick="editModelGroup('${{group.name.replace(/'/g, "\\'")}}', '${{encodedTargets}}')" class="p-2 text-emerald-400 hover:bg-emerald-400/10 rounded transition-colors" title="Edit">
                                <i class="fas fa-edit"></i>
                            </button>
                            <button onclick="deleteModelGroup('${{group.name.replace(/'/g, "\\'")}}')" class="p-2 text-red-400 hover:bg-red-400/10 rounded transition-colors" title="Delete">
                                <i class="fas fa-trash-alt"></i>
                            </button>
                        </div>
                    </td>
                </tr>
            `}}).join('');
        }} catch (e) {{
            body.innerHTML = '<tr><td colspan="3" class="text-center text-red-400 py-8">Error loading model groups.</td></tr>';
        }}
    }}

    async function deleteModelGroup(name) {{
        showConfirm('Delete Model Group', `Are you sure you want to delete model group "${{name}}"? This action cannot be undone.`, async () => {{
            try {{
                const resp = await fetch(`/admin/model-groups/${{encodeURIComponent(name)}}`, {{ method: 'DELETE' }});
                if (resp.ok) {{
                    loadModelGroups();
                }} else {{
                    showModal('Error', 'Failed to delete model group', true);
                }}
            }} catch (e) {{
                console.error(e);
            }}
        }});
    }}

    async function saveModelGroup() {{ 
        const name=document.getElementById('model-group-name').value.trim(); 
        const btn = document.getElementById('save-model-group-btn');
        if (!name || selectedModels.length === 0) return showModal('Error', 'Model group name and at least one target model are required', true); 
        
        btn.classList.add('btn-loading');
        try {{
            const r = await fetch('/admin/config', {{
                method:'PUT', 
                headers:{{'Content-Type':'application/json'}}, 
                body:JSON.stringify({{
                    model_groups:[{{
                        name, 
                        target: selectedModels.length === 1 ? targetPayload(selectedModels[0]) : selectedModels.map(targetPayload), 
                        enabled:true
                    }}]
                }})
            }}); 
            
            if (r.ok) {{ 
                cancelEdit();
                loadModelGroups();
                showModal('Success', 'Model group saved successfully', false); 
            }} else {{
                showModal('Error', 'Failed to save model group', true); 
            }}
        }} catch (e) {{
            showModal('Error', 'Network error', true);
        }} finally {{
            btn.classList.remove('btn-loading');
        }}
    }}
    async function rollback(snapshot_id) {{ const r = await fetch('/admin/config/rollback', {{method:'POST', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{snapshot_id}})}}); if (r.ok) location.reload(); else showModal('Error', 'Rollback failed', true); }}
    async function saveConfig() {{ try {{ const cfg = JSON.parse(document.getElementById('raw-config').value); const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify(cfg)}}); if (r.ok) {{ showModal('Success', 'Configuration deployed successfully', false); setTimeout(()=>location.reload(), 1500); }} else {{ const err = await r.json(); showModal('Validation Error', err, true); }} }} catch(e) {{ showModal('Parse Error', 'Invalid JSON', true); }} }}
    
    // Load initial data
    loadModelGroups();
    fetch('/admin/config').then(r=>r.json()).then(d => {{
        document.getElementById('raw-config').value = JSON.stringify(d, null, 2);
    }}).catch(() => {{
        document.getElementById('raw-config').value = "Failed to load config.";
    }});
    </script>"#,
        models_json
    );
    render_config_html(&content, &scripts, state)
}

fn render_config_html(content: &str, scripts: &str, state: &AppState) -> String {
    render_shell("Config", "config", content, scripts, state)
}

pub async fn render_audit(state: &AppState) -> String {
    let content = r#"<div class="glass-card p-6"><h3 class="font-bold text-emerald-400 mb-2">Audit History</h3><p class="text-sm text-slate-400">Configuration change history is recorded as actions are performed. View the raw DB for details.</p></div>"#;
    render_shell("Audit", "audit", content, "", state)
}

/// Render the interactive Chat Tester page.
pub async fn render_chat(state: &AppState) -> String {
    let models = crate::discovery::merge::get_all_models(state).await;
    let models_json = serde_json::to_string(&models).unwrap_or("{}".into());

    let content = r#"
    <div class="flex gap-6 h-[calc(100vh-10rem)]">

      <!-- Sidebar: settings -->
      <div class="w-72 shrink-0 flex flex-col gap-4">
        <div class="glass-card p-5 flex flex-col gap-4">
          <h3 class="font-bold text-sm text-slate-300 uppercase tracking-wider">Model</h3>
          <div>
            <label class="text-[10px] font-bold text-slate-500 uppercase tracking-wider block mb-1">Provider</label>
            <select id="chatProvider" class="w-full" onchange="onProviderChange()"><option value="">Auto (Router decides)</option></select>
          </div>
          <div>
            <label class="text-[10px] font-bold text-slate-500 uppercase tracking-wider block mb-1">Model</label>
            <select id="chatModel" class="w-full"><option value="">Default</option></select>
          </div>
        </div>

        <div class="glass-card p-5 flex flex-col gap-4">
          <h3 class="font-bold text-sm text-slate-300 uppercase tracking-wider">Parameters</h3>
          <div>
            <label class="text-[10px] font-bold text-slate-500 uppercase tracking-wider block mb-1">Temperature <span id="tempVal" class="text-white">0.7</span></label>
            <input type="range" id="temperature" min="0" max="2" step="0.05" value="0.7" class="w-full accent-[#D35400]" oninput="document.getElementById('tempVal').innerText=this.value">
          </div>
          <div>
            <label class="text-[10px] font-bold text-slate-500 uppercase tracking-wider block mb-1">Max Tokens</label>
            <input type="number" id="maxTokens" value="1024" min="1" max="32768" class="w-full">
          </div>
          <div>
            <label class="text-[10px] font-bold text-slate-500 uppercase tracking-wider block mb-1">Stream</label>
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="checkbox" id="useStream" checked class="accent-[#D35400]">
              <span class="text-sm text-slate-400">Enable streaming</span>
            </label>
          </div>
        </div>

        <div class="glass-card p-5 flex flex-col gap-3">
          <h3 class="font-bold text-sm text-slate-300 uppercase tracking-wider">System Prompt</h3>
          <textarea id="systemPrompt" class="w-full h-28 text-xs resize-none" placeholder="You are a helpful assistant..."></textarea>
        </div>

        <div class="glass-card p-4">
          <div class="text-[10px] font-bold text-slate-500 uppercase tracking-wider mb-2">Last Response</div>
          <div id="tokenInfo" class="text-xs text-slate-400 space-y-1">
            <div class="flex justify-between"><span>Input tokens</span><span id="ti" class="text-white font-mono">—</span></div>
            <div class="flex justify-between"><span>Output tokens</span><span id="to" class="text-white font-mono">—</span></div>
            <div class="flex justify-between"><span>Provider used</span><span id="tp" class="text-cyan-400 font-mono text-[10px]">—</span></div>
            <div class="flex justify-between"><span>Model used</span><span id="tm" class="text-cyan-400 font-mono text-[10px]">—</span></div>
            <div class="flex justify-between"><span>Latency</span><span id="tl" class="text-white font-mono">—</span></div>
          </div>
        </div>
      </div>

      <!-- Main: conversation -->
      <div class="flex-1 flex flex-col glass-card overflow-hidden">
        <!-- Header toolbar -->
        <div class="px-6 py-3 border-b border-white/5 bg-white/[0.02] flex items-center justify-between shrink-0">
          <span class="text-sm font-bold text-slate-300">Conversation</span>
          <div class="flex gap-2">
            <button class="btn" style="background:#334155;" onclick="clearChat()"><i class="fas fa-trash-alt mr-1"></i>Clear</button>
          </div>
        </div>

        <!-- Messages -->
        <div id="chatMessages" class="flex-1 overflow-y-auto p-6 space-y-4">
          <div class="flex items-start gap-3" id="welcomeMsg">
            <div class="w-7 h-7 rounded-full bg-[#D35400]/20 flex items-center justify-center shrink-0 mt-0.5">
              <i class="fas fa-robot text-[#D35400] text-[10px]"></i>
            </div>
            <div class="glass-card p-4 max-w-prose">
              <p class="text-sm text-slate-300">Hello! Select a model and start chatting to test the TokenScavenger routing engine. Messages are sent directly through <code class="text-cyan-400">/v1/chat/completions</code>.</p>
            </div>
          </div>
        </div>

        <!-- Input bar -->
        <div class="px-6 py-4 border-t border-white/5 bg-white/[0.02] shrink-0">
          <div class="flex gap-3 items-end">
            <textarea id="chatInput" rows="2" class="flex-1 resize-none text-sm" placeholder="Type a message… (Ctrl+Enter to send)" onkeydown="handleKey(event)"></textarea>
            <button id="sendBtn" class="btn h-12 px-6" onclick="sendMessage()">
              <i class="fas fa-paper-plane mr-1"></i>Send
            </button>
          </div>
          <div id="statusBar" class="mt-2 text-[10px] text-slate-500 min-h-[1rem]"></div>
        </div>
      </div>
    </div>"#.to_string();

    let scripts = format!(
        r#"<script>
    const modelsPayload = {};
    const allModels = modelsPayload.models || [];

    // Populate provider dropdown
    const providerSel = document.getElementById('chatProvider');
    const modelSel = document.getElementById('chatModel');
    const providers = [...new Set(allModels.map(m => m.provider_id).filter(Boolean))];
    providers.forEach(p => {{
        const opt = document.createElement('option');
        opt.value = p; opt.text = p;
        providerSel.appendChild(opt);
    }});

    function onProviderChange() {{
        const prov = providerSel.value;
        modelSel.innerHTML = '<option value="">Default</option>';
        const filtered = modelsForCurrentSelection();
        filtered.forEach(m => {{
            const opt = document.createElement('option');
            opt.value = m.upstream_model_id || '';
            opt.text = m.upstream_model_id || '';
            modelSel.appendChild(opt);
        }});
    }}

    function modelsForCurrentSelection() {{
        const prov = providerSel.value;
        return allModels.filter(m =>
            m.enabled !== false &&
            m.supports_chat !== false &&
            (!prov || m.provider_id === prov)
        );
    }}

    function resolveSelectedModel() {{
        const explicit = modelSel.value;
        if (explicit) return explicit;
        const fallback = modelsForCurrentSelection()[0];
        return fallback?.upstream_model_id || fallback?.public_model_id || '';
    }}

    onProviderChange();

    let messages = []; // {{role, content}}
    let generating = false;

    function handleKey(e) {{
        if (e.ctrlKey && e.key === 'Enter') {{ e.preventDefault(); sendMessage(); }}
    }}

    function clearChat() {{
        messages = [];
        const el = document.getElementById('chatMessages');
        el.innerHTML = '';
        document.getElementById('welcomeMsg') && null;
        setStatus('');
    }}

    function setStatus(msg) {{
        document.getElementById('statusBar').innerText = msg;
    }}

    function appendBubble(role, content, id) {{
        const isUser = role === 'user';
        const container = document.getElementById('chatMessages');
        const wrap = document.createElement('div');
        wrap.className = `flex items-start gap-3 ${{isUser ? 'flex-row-reverse' : ''}}`;
        const avatar = document.createElement('div');
        avatar.className = `w-7 h-7 rounded-full flex items-center justify-center shrink-0 mt-0.5 ${{isUser ? 'bg-slate-700' : 'bg-[#D35400]/20'}}`;
        avatar.innerHTML = isUser ? '<i class="fas fa-user text-slate-300 text-[10px]"></i>' : '<i class="fas fa-robot text-[#D35400] text-[10px]"></i>';
        const bubble = document.createElement('div');
        bubble.className = `glass-card p-4 max-w-prose text-sm ${{isUser ? 'bg-slate-800/60 text-white' : 'text-slate-200'}}`;
        if (id) bubble.id = id;
        bubble.style.whiteSpace = 'pre-wrap';
        bubble.innerText = content;
        wrap.appendChild(avatar);
        wrap.appendChild(bubble);
        container.appendChild(wrap);
        container.scrollTop = container.scrollHeight;
        return bubble;
    }}

    async function sendMessage() {{
        if (generating) return;
        const input = document.getElementById('chatInput');
        const userText = input.value.trim();
        if (!userText) return;

        const model = resolveSelectedModel();
        if (!model) {{
            setStatus('No enabled chat models are available for the current provider filter.');
            return;
        }}
        const systemPrompt = document.getElementById('systemPrompt').value.trim();
        const temperature = parseFloat(document.getElementById('temperature').value);
        const maxTokens = parseInt(document.getElementById('maxTokens').value, 10);
        const useStream = document.getElementById('useStream').checked;

        // Add to history and display
        messages.push({{ role: 'user', content: userText }});
        appendBubble('user', userText);
        input.value = '';

        const reqMessages = [
            ...(systemPrompt ? [{{ role: 'system', content: systemPrompt }}] : []),
            ...messages
        ];

        generating = true;
        document.getElementById('sendBtn').disabled = true;
        document.getElementById('sendBtn').innerHTML = '<i class="fas fa-spinner fa-spin mr-1"></i>Sending';
        setStatus('Sending request...');

        const t0 = Date.now();
        try {{
            const body = {{
                model,
                messages: reqMessages,
                temperature,
                max_tokens: maxTokens,
                stream: useStream
            }};

            if (useStream) {{
                const asstBubble = appendBubble('assistant', '', 'streaming-bubble');
                let fullText = '';
                let usageData = null;

                const resp = await fetch('/v1/chat/completions', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(body)
                }});

                if (!resp.ok) {{
                    const err = await resp.json().catch(() => ({{ message: resp.statusText }}));
                    asstBubble.innerText = `Error ${{resp.status}}: ${{err.error?.message || err.message || 'Unknown error'}}`;
                    asstBubble.style.color = '#ef4444';
                    messages.push({{ role: 'assistant', content: asstBubble.innerText }});
                    return;
                }}

                const reader = resp.body.getReader();
                const decoder = new TextDecoder();
                setStatus('Streaming...');

                while (true) {{
                    const {{ done, value }} = await reader.read();
                    if (done) break;
                    const chunk = decoder.decode(value, {{ stream: true }});
                    for (const line of chunk.split('\n')) {{
                        const trimmed = line.trim();
                        if (!trimmed.startsWith('data:')) continue;
                        const data = trimmed.slice(5).trim();
                        if (data === '[DONE]') break;
                        try {{
                            const parsed = JSON.parse(data);
                            const delta = parsed.choices?.[0]?.delta?.content || '';
                            fullText += delta;
                            asstBubble.innerText = fullText;
                            document.getElementById('chatMessages').scrollTop = document.getElementById('chatMessages').scrollHeight;
                            if (parsed.usage) usageData = parsed.usage;
                            if (parsed.model) document.getElementById('tm').innerText = parsed.model;
                            if (parsed.x_provider) document.getElementById('tp').innerText = parsed.x_provider;
                        }} catch {{}}
                    }}
                }}

                asstBubble.removeAttribute('id');
                messages.push({{ role: 'assistant', content: fullText }});
                if (usageData) {{
                    document.getElementById('ti').innerText = usageData.prompt_tokens ?? '—';
                    document.getElementById('to').innerText = usageData.completion_tokens ?? '—';
                }}

            }} else {{
                const resp = await fetch('/v1/chat/completions', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify(body)
                }});
                const data = await resp.json();
                if (!resp.ok) {{
                    appendBubble('assistant', `Error ${{resp.status}}: ${{data.error?.message || 'Unknown error'}}`);
                    return;
                }}
                const text = data.choices?.[0]?.message?.content || '';
                appendBubble('assistant', text);
                messages.push({{ role: 'assistant', content: text }});
                if (data.usage) {{
                    document.getElementById('ti').innerText = data.usage.prompt_tokens ?? '—';
                    document.getElementById('to').innerText = data.usage.completion_tokens ?? '—';
                }}
                if (data.model) document.getElementById('tm').innerText = data.model;
            }}

            const latency = ((Date.now() - t0) / 1000).toFixed(2);
            document.getElementById('tl').innerText = latency + 's';
            setStatus(`Done in ${{latency}}s`);

        }} catch(err) {{
            appendBubble('assistant', 'Network error: ' + err.message);
            setStatus('Error: ' + err.message);
        }} finally {{
            generating = false;
            document.getElementById('sendBtn').disabled = false;
            document.getElementById('sendBtn').innerHTML = '<i class="fas fa-paper-plane mr-1"></i>Send';
        }}
    }}
    </script>"#,
        models_json
    );

    render_shell("Chat Tester", "chat", &content, &scripts, state)
}
