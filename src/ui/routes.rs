use crate::app::state::AppState;

/// Render the dashboard view.
pub async fn render_dashboard(state: &AppState) -> String {
    let config = state.config();
    let uptime = state.start_time.elapsed().as_secs();

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TokenScavenger Dashboard</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: system-ui, sans-serif; }}</style>
</head>
<body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-6">TokenScavenger Dashboard</h1>
<div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
<div class="bg-slate-800 rounded p-4"><div class="text-gray-400 text-xs">Uptime</div><div class="text-xl font-mono">{uptime}s</div></div>
<div class="bg-slate-800 rounded p-4"><div class="text-gray-400 text-xs">Providers</div><div class="text-xl font-mono">{providers}</div></div>
<div class="bg-slate-800 rounded p-4"><div class="text-gray-400 text-xs">Bind</div><div class="text-sm font-mono">{bind}</div></div>
<div class="bg-slate-800 rounded p-4"><div class="text-gray-400 text-xs">Free First</div><div class="text-xl font-mono">{free_first}</div></div>
</div>
<div class="bg-slate-800 rounded p-4">
<h2 class="text-lg font-semibold mb-2 text-emerald-400">System Status</h2>
<p>TokenScavenger v0.1.0 — LLM Proxy Router</p>
<p class="text-gray-400 text-sm mt-2">Rust edition 2024 | Axum + Tokio + SQLite</p>
</div>
</body></html>"#,
        uptime = uptime,
        providers = config.providers.len(),
        bind = config.server.bind,
        free_first = config.routing.free_first,
    )
}

/// Render the providers view.
pub async fn render_providers(state: &AppState) -> String {
    let config = state.config();
    let mut rows = String::new();
    for p in &config.providers {
        let health = state.health_states.get(&p.id);
        let health_str = health.as_ref().map(|h| format!("{:?}", h.value())).unwrap_or("unknown".into());
        rows.push_str(&format!(
            r#"<tr class="border-t border-slate-700"><td class="p-2">{id}</td><td class="p-2">{health}</td><td class="p-2">{enabled}</td></tr>"#,
            id = p.id,
            health = health_str,
            enabled = if p.enabled { "Enabled" } else { "Disabled" },
        ));
    }

    format!(r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Providers | TokenScavenger</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: system-ui, sans-serif; }}</style>
</head><body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-4">Providers</h1>
<table class="w-full text-left">
<thead><tr class="text-gray-400 text-sm"><th class="p-2">ID</th><th class="p-2">Health</th><th class="p-2">Status</th></tr></thead>
<tbody>{rows}</tbody></table></body></html>"#,
        rows = rows,
    )
}

/// Render the models view.
pub async fn render_models(state: &AppState) -> String {
    let models = crate::discovery::merge::get_all_models(state).await;
    let models_html = match models.get("models").and_then(|m| m.as_array()) {
        Some(arr) => arr.iter().map(|m| {
            let p = m.get("provider_id").and_then(|v| v.as_str()).unwrap_or("?");
            let u = m.get("upstream_model_id").and_then(|v| v.as_str()).unwrap_or("?");
            format!(r#"<tr class="border-t border-slate-700"><td class="p-2 font-mono text-xs">{}</td><td class="p-2">{}</td></tr>"#, u, p)
        }).collect::<Vec<_>>().join("\n"),
        None => "<tr><td class='p-2 text-gray-400'>No models discovered</td></tr>".into(),
    };

    format!(r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Models | TokenScavenger</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: system-ui, sans-serif; }}</style>
</head><body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-4">Models</h1>
<table class="w-full text-left">
<thead><tr class="text-gray-400 text-sm"><th class="p-2">Model ID</th><th class="p-2">Provider</th></tr></thead>
<tbody>{models}</tbody></table></body></html>"#,
        models = models_html,
    )
}

/// Render the routing/aliases view.
pub async fn render_routing(state: &AppState) -> String {
    let config = state.config();
    let order = config.routing.provider_order.join(" → ");
    format!(r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Routing | TokenScavenger</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: system-ui, sans-serif; }}</style>
</head><body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-4">Routing</h1>
<div class="bg-slate-800 rounded p-4">
<p class="text-gray-400 text-sm mb-2">Provider Fallback Order</p>
<p class="font-mono text-sm">{order}</p>
<p class="mt-4 text-gray-400 text-sm">Free First: {free_first} | Paid Fallback: {paid_fallback}</p>
</div></body></html>"#,
        order = order,
        free_first = config.routing.free_first,
        paid_fallback = config.routing.allow_paid_fallback,
    )
}

/// Render the usage analytics view.
pub async fn render_usage(state: &AppState) -> String {
    let series = crate::usage::aggregation::get_usage_series(state).await;
    let rows = match series.get("series").and_then(|s| s.as_array()) {
        Some(arr) => arr.iter().map(|entry| {
            let p = entry.get("provider_id").and_then(|v| v.as_str()).unwrap_or("?");
            let inp = entry.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let out = entry.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let cost = entry.get("estimated_cost_usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
            format!(r#"<tr class="border-t border-slate-700"><td class="p-2">{p}</td><td class="p-2">{inp}</td><td class="p-2">{out}</td><td class="p-2">${cost:.4}</td></tr>"#)
        }).collect::<Vec<_>>().join("\n"),
        None => "<tr><td class='p-2 text-gray-400' colspan='4'>No usage data</td></tr>".into(),
    };

    format!(r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Usage | TokenScavenger</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: system-ui, sans-serif; }}</style>
</head><body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-4">Usage (Last 24h)</h1>
<table class="w-full text-left">
<thead><tr class="text-gray-400 text-sm"><th class="p-2">Provider</th><th class="p-2">Input Tokens</th><th class="p-2">Output Tokens</th><th class="p-2">Est. Cost</th></tr></thead>
<tbody>{rows}</tbody></table></body></html>"#,
        rows = rows,
    )
}

/// Render the health view.
pub async fn render_health(state: &AppState) -> String {
    let mut rows = String::new();
    for entry in state.health_states.iter() {
        let pid = entry.key();
        let hs = entry.value();
        rows.push_str(&format!(
            r#"<tr class="border-t border-slate-700"><td class="p-2">{}</td><td class="p-2">{:?}</td><td class="p-2">{}</td></tr>"#,
            pid,
            hs.value(),
            hs.recent_successes + hs.recent_failures,
        ));
    }

    format!(r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Health | TokenScavenger</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: system-ui, sans-serif; }}</style>
</head><body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-4">Provider Health</h1>
<table class="w-full text-left">
<thead><tr class="text-gray-400 text-sm"><th class="p-2">Provider</th><th class="p-2">State</th><th class="p-2">Total Requests</th></tr></thead>
<tbody>{rows}</tbody></table></body></html>"#,
        rows = rows,
    )
}

/// Render the logs view.
pub async fn render_logs(_state: &AppState) -> String {
    r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Logs | TokenScavenger</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: monospace; }}</style>
</head><body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-4">Log Stream</h1>
<div id="logs" class="bg-slate-900 p-4 rounded text-xs h-96 overflow-y-auto">
<p class="text-gray-500">Connecting to log stream...</p>
</div>
<script>
const es = new EventSource('/admin/logs/stream');
es.onmessage = (e) => { const el = document.getElementById('logs'); el.innerHTML += '<div>' + e.data + '</div>'; el.scrollTop = el.scrollHeight; };
</script>
</body></html>"#.into()
}

/// Render the configuration view.
pub async fn render_config(_state: &AppState) -> String {
    r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Configuration | TokenScavenger</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: system-ui, sans-serif; }}</style>
</head><body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-4">Configuration</h1>
<p class="text-gray-400">Configuration editing via the admin API. Use <code class="text-cyan-400">GET /admin/config</code> to view current config.</p>
<form action="/admin/config" method="GET" class="mt-4">
<button type="submit" class="bg-cyan-600 px-4 py-2 rounded text-sm">View Current Config</button>
</form>
</body></html>"#.into()
}

/// Render the audit history view.
pub async fn render_audit(_state: &AppState) -> String {
    r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Audit History | TokenScavenger</title>
<script src="https://cdn.tailwindcss.com"></script>
<style>body {{ background: #020617; color: #f5f5f5; font-family: system-ui, sans-serif; }}</style>
</head><body class="p-6">
<nav class="flex gap-4 mb-8 text-sm text-cyan-400">
<a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a>
<a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a>
<a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a>
</nav>
<h1 class="text-2xl font-bold text-orange-500 mb-4">Audit History</h1>
<p class="text-gray-400">Configuration change history is recorded as actions are performed.</p>
</body></html>"#.into()
}
