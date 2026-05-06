use crate::app::state::AppState;

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
        <div class="pt-4 pb-2 px-4 text-[10px] font-bold text-slate-600 uppercase tracking-widest">System</div>
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
        nav_item("chat", "/ui/chat", "fas fa-comment-dots", "Chat Tester"),
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
    <div class="flex items-center gap-4">
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
        if (!apiKey) { showModal('Error', 'API key is required', true); return; }
        const provider = { id, enabled: true, api_key: apiKey, free_only: freeOnly };
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
    let content = r#"
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
                        <tr><th>Model ID</th><th>Provider</th><th>Status</th><th>Priority</th><th>Actions</th></tr>
                    </thead>
                    <tbody id="modelsTableBody" class="divide-y divide-white/5"></tbody>
                </table>
            </div>
            <div class="px-6 py-3 border-t border-white/5 bg-white/[0.01] flex items-center justify-between">
                <span id="pageInfo" class="text-xs text-slate-500"></span>
                <div class="flex gap-2">
                    <button class="btn" style="background:#334155;" onclick="prevPage()">Previous</button>
                    <button class="btn" style="background:#334155;" onclick="nextPage()">Next</button>
                </div>
            </div>
        </div>"#.to_string();
    let scripts = format!(
        r#"<script>
    const modelsData = {};
    const modelsArr = modelsData.models || [];
    let currentPage = 1;
    const pageSize = 10;
    
    const providers = [...new Set(modelsArr.map(m => m.provider_id))];
    const select = document.getElementById('providerFilter');
    providers.forEach(p => {{
        if (p) select.innerHTML += `<option value="${{p}}">${{p}}</option>`;
    }});

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
            html = `<tr><td colspan="4" class="px-6 py-4 text-center text-slate-500">No models found</td></tr>`;
        }} else {{
            slice.forEach(m => {{
                const u = m.upstream_model_id || '?';
                const p = m.provider_id || '?';
                const enabled = m.enabled !== false;
                const next_enabled = !enabled;
                const button_label = enabled ? "Disable" : "Enable";
                const button_class = enabled ? "btn-danger" : "btn";
                const prio = m.priority || 100;
                const status_html = enabled 
                    ? `<span class="px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-500 text-[10px]">Enabled</span>`
                    : `<span class="px-2 py-0.5 rounded bg-white/5 text-slate-500 text-[10px]">Disabled</span>`;
                html += `<tr><td class="font-mono text-sm text-cyan-400">${{u}}</td><td class="text-sm">${{p}}</td><td>${{status_html}}</td><td><input type="number" value="${{prio}}" class="w-16 bg-black/20 border border-white/10 rounded px-2 py-0.5 text-xs text-center" onchange="updateModelPriority('${{p.replace(/'/g, "\\'")}}','${{u.replace(/'/g, "\\'")}}', this.value)"></td><td><button class="${{button_class}}" onclick="toggleModel('${{p.replace(/'/g, "\\'")}}','${{u.replace(/'/g, "\\'")}}',${{next_enabled}})">${{button_label}}</button></td></tr>`;
            }});
        }}
        document.getElementById('modelsTableBody').innerHTML = html;
        document.getElementById('pageInfo').innerText = `Page ${{currentPage}} of ${{totalPages}} (${{filtered.length}} total)`;
    }}

    function prevPage() {{ if (currentPage > 1) {{ currentPage--; renderTable(); }} }}
    function nextPage() {{ const search = document.getElementById('modelSearch').value.toLowerCase(); const prov = document.getElementById('providerFilter').value; const totalPages = Math.ceil(modelsArr.filter(m => (m.upstream_model_id || '').toLowerCase().includes(search) && (prov ? m.provider_id === prov : true)).length / pageSize); if (currentPage < totalPages) {{ currentPage++; renderTable(); }} }}

    async function toggleModel(provider_id, model_id, enabled) {{ const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{models:[{{provider_id, model_id, enabled}}]}})}}); if (r.ok) location.reload(); else showModal('Error', 'Model update failed', true); }}
    async function updateModelPriority(provider_id, model_id, priority) {{ const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{models:[{{provider_id, model_id, priority: parseInt(priority)}}]}})}}); if (r.ok) location.reload(); else showModal('Error', 'Priority update failed', true); }}
    
    renderTable();
    </script>"#,
        models_json
    );
    render_shell("Models", "models", &content, &scripts, state)
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
                    <p class="text-[10px] text-slate-500">Model groups map one public model name to multiple target models. TokenScavenger will try them in order if the first one fails or is unhealthy.</p>
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
    const dropdown = document.getElementById('target-dropdown');
    const tagsContainer = document.getElementById('target-tags');

    function renderTags() {{
        const existingTags = tagsContainer.querySelectorAll('.model-tag');
        existingTags.forEach(t => t.remove());
        selectedModels.forEach(m => {{
            const tag = document.createElement('span');
            tag.className = 'model-tag px-2 py-1 bg-cyan-500/10 text-cyan-400 text-xs rounded border border-cyan-500/20 flex items-center gap-2';
            tag.innerHTML = `${{m}} <i class="fas fa-times cursor-pointer hover:text-white" onclick="removeModel('${{m}}')"></i>`;
            tagsContainer.insertBefore(tag, searchInput);
        }});
    }}

    function removeModel(m) {{
        selectedModels = selectedModels.filter(x => x !== m);
        renderTags();
    }}

    function addModel(m) {{
        if (!selectedModels.includes(m)) {{
            selectedModels.push(m);
            renderTags();
        }}
        searchInput.value = '';
        dropdown.classList.add('hidden');
    }}

    function cancelEdit() {{
        document.getElementById('model-group-name').value = '';
        document.getElementById('model-group-name').disabled = false;
        selectedModels = [];
        renderTags();
        document.getElementById('save-model-group-btn').querySelector('.btn-text').innerText = 'Save Group';
        document.getElementById('cancel-edit-btn').classList.add('hidden');
    }}

    function editModelGroup(name, targets) {{
        document.getElementById('model-group-name').value = name;
        document.getElementById('model-group-name').disabled = true;
        selectedModels = [...targets];
        renderTags();
        document.getElementById('save-model-group-btn').querySelector('.btn-text').innerText = 'Update Group';
        document.getElementById('cancel-edit-btn').classList.remove('hidden');
        document.getElementById('model-group-name').scrollIntoView({{ behavior: 'smooth' }});
    }}

    searchInput.onfocus = () => showDropdown(searchInput.value);
    searchInput.oninput = (e) => showDropdown(e.target.value);
    
    document.addEventListener('click', (e) => {{
        if (!tagsContainer.contains(e.target) && !dropdown.contains(e.target)) {{
            dropdown.classList.add('hidden');
        }}
    }});

    function showDropdown(query) {{
        const q = query.toLowerCase();
        const filtered = allModels.filter(m => 
            !selectedModels.includes(m.upstream_model_id) && 
            (m.upstream_model_id.toLowerCase().includes(q) || m.provider_id.toLowerCase().includes(q))
        ).slice(0, 50);

        if (filtered.length === 0) {{
            dropdown.classList.add('hidden');
            return;
        }}

        dropdown.innerHTML = filtered.map(m => `
            <div class="px-4 py-2 hover:bg-white/5 cursor-pointer flex justify-between items-center group" onclick="addModel('${{m.upstream_model_id}}')">
                <span class="text-sm text-slate-200">${{m.upstream_model_id}}</span>
                <span class="text-[10px] text-slate-500 group-hover:text-cyan-400">${{m.provider_id}}</span>
            </div>
        `).join('');
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
                const targetsJson = JSON.stringify(targets).replace(/"/g, '&quot;');
                return `
                <tr>
                    <td class="px-6 py-4 font-mono text-cyan-400">${{group.name}}</td>
                    <td class="px-6 py-4">
                        <div class="flex flex-wrap gap-1">
                            ${{targets.map(t => `<span class="text-[10px] bg-white/5 px-2 py-0.5 rounded border border-white/10">${{t}}</span>`).join('')}}
                        </div>
                    </td>
                    <td class="px-6 py-4 text-right">
                        <div class="flex justify-end gap-2">
                            <button onclick="editModelGroup('${{group.name.replace(/'/g, "\\'")}}', ${{targetsJson}})" class="p-2 text-emerald-400 hover:bg-emerald-400/10 rounded transition-colors" title="Edit">
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
                        target: selectedModels.length === 1 ? selectedModels[0] : selectedModels, 
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
        const filtered = prov ? allModels.filter(m => m.provider_id === prov && m.enabled !== false) : allModels.filter(m => m.enabled !== false);
        filtered.forEach(m => {{
            const opt = document.createElement('option');
            opt.value = m.upstream_model_id || '';
            opt.text = m.upstream_model_id || '';
            modelSel.appendChild(opt);
        }});
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

        const model = modelSel.value || undefined;
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
                model: model || 'default',
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
