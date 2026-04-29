use crate::app::state::AppState;

/// Base HTML shell with embedded Tailwind (offline-compatible via inline styles).
const HTML_HEAD: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>:root{--bg:#020617;--fg:#f5f5f5;--cyan:#00f5ff;--orange:#d35400;--emerald:#00ff9f;--card:#1e293b;--border:#334155;--gray:#9ca3af;--danger:#ef4444}*{margin:0;padding:0;box-sizing:border-box}body{background:var(--bg);color:var(--fg);font-family:system-ui,sans-serif;padding:1.5rem}a{color:var(--cyan);text-decoration:none;font-size:.875rem}h1{color:var(--orange);font-size:1.5rem;font-weight:bold;margin-bottom:1rem}h2{color:var(--emerald);font-size:1.125rem;margin-bottom:.5rem}nav{display:flex;flex-wrap:wrap;gap:1rem;margin-bottom:2rem}table{width:100%;border-collapse:collapse}caption{text-align:left;color:var(--gray);margin-bottom:.5rem}th{color:var(--gray);font-size:.875rem;padding:.5rem;text-align:left}td{padding:.5rem;border-top:1px solid var(--border)}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:1rem;margin-bottom:2rem}.card{background:var(--card);padding:1rem;border-radius:.5rem}.label{color:var(--gray);font-size:.75rem}.value{font-size:1.25rem;font-family:monospace}.mono{font-family:monospace;font-size:.875rem}.btn{background:#0891b2;padding:.5rem 1rem;border-radius:.25rem;border:none;color:#fff;cursor:pointer;font-size:.875rem}.btn.secondary{background:#334155}.btn.danger{background:var(--danger)}.btn:focus,a:focus{outline:2px solid var(--cyan);outline-offset:2px}.logs{background:#0f172a;padding:1rem;border-radius:.5rem;height:24rem;overflow-y:auto;font-family:monospace;font-size:.75rem}.mt-4{margin-top:1rem}.actions{display:flex;gap:.5rem;flex-wrap:wrap}</style>"#;

const NAV: &str = r#"<nav><a href="/ui">Dashboard</a><a href="/ui/providers">Providers</a><a href="/ui/models">Models</a><a href="/ui/routing">Routing</a><a href="/ui/usage">Usage</a><a href="/ui/health">Health</a><a href="/ui/logs">Logs</a><a href="/ui/config">Config</a><a href="/ui/audit">Audit</a></nav>"#;

/// Render the dashboard view.
pub async fn render_dashboard(state: &AppState) -> String {
    let config = state.config();
    let uptime = state.start_time.elapsed().as_secs();
    format!(
        r#"{head}<title>TokenScavenger Dashboard</title></head><body>{nav}<h1>TokenScavenger Dashboard</h1><div class="grid"><div class="card"><div class="label">Uptime</div><div class="value">{uptime}s</div></div><div class="card"><div class="label">Providers</div><div class="value">{providers}</div></div><div class="card"><div class="label">Bind</div><div class="value mono">{bind}</div></div><div class="card"><div class="label">Free First</div><div class="value">{free_first}</div></div></div><div class="card"><h2>System Status</h2><p>TokenScavenger v0.1.0 — LLM Proxy Router</p><p class="label mt-4">Rust edition 2024 | Axum + Tokio + SQLite</p></div></body></html>"#,
        head = HTML_HEAD,
        nav = NAV,
        uptime = uptime,
        providers = config.providers.len(),
        bind = config.server.bind,
        free_first = config.routing.free_first
    )
}

/// Render the providers view.
pub async fn render_providers(state: &AppState) -> String {
    let config = state.config();
    let mut rows = String::new();
    for p in &config.providers {
        let health = state.health_states.get(&p.id);
        let health_str = health
            .as_ref()
            .map(|h| format!("{:?}", h.value()))
            .unwrap_or("unknown".into());
        let next_enabled = if p.enabled { "false" } else { "true" };
        let button_label = if p.enabled { "Disable" } else { "Enable" };
        let button_class = if p.enabled { "btn danger" } else { "btn" };
        rows.push_str(&format!(
            r#"<tr><td class="mono">{id}</td><td>{health}</td><td>{enabled}</td><td><div class="actions"><button class="{button_class}" onclick="toggleProvider('{id}',{next_enabled})">{button_label}</button><button class="btn secondary" onclick="testProvider('{id}')">Test</button></div></td></tr>"#,
            id = p.id,
            health = health_str,
            enabled = if p.enabled { "Enabled" } else { "Disabled" },
            button_class = button_class,
            button_label = button_label,
            next_enabled = next_enabled
        ));
    }
    format!(
        r#"{head}<title>Providers | TokenScavenger</title></head><body>{nav}<h1>Providers</h1><div class="actions mt-4"><button class="btn" onclick="refreshDiscovery()">Refresh Discovery</button></div><table class="mt-4"><caption>Configured provider status and controls</caption><thead><tr><th>ID</th><th>Health</th><th>Status</th><th>Actions</th></tr></thead><tbody>{rows}</tbody></table><script>
async function toggleProvider(id, enabled) {{
  const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{providers:[{{id, enabled}}]}})}});
  if (r.ok) location.reload(); else alert('Provider update failed');
}}
async function testProvider(id) {{
  const r = await fetch('/admin/providers/'+encodeURIComponent(id)+'/test', {{method:'POST'}});
  alert(JSON.stringify(await r.json(), null, 2));
}}
async function refreshDiscovery() {{
  const r = await fetch('/admin/providers/discovery/refresh', {{method:'POST'}});
  if (r.ok) location.reload(); else alert('Discovery refresh failed');
}}
</script></body></html>"#,
        head = HTML_HEAD,
        nav = NAV,
        rows = rows
    )
}

/// Render the models view.
pub async fn render_models(state: &AppState) -> String {
    let models = crate::discovery::merge::get_all_models(state).await;
    let models_html = match models.get("models").and_then(|m| m.as_array()) {
        Some(arr) => arr
            .iter()
            .map(|m| {
                let u = m
                    .get("upstream_model_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let p = m.get("provider_id").and_then(|v| v.as_str()).unwrap_or("?");
                let enabled = m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                let next_enabled = if enabled { "false" } else { "true" };
                let button_label = if enabled { "Disable" } else { "Enable" };
                let button_class = if enabled { "btn danger" } else { "btn" };
                format!(
                    r#"<tr><td class="mono">{}</td><td>{}</td><td>{}</td><td><button class="{}" onclick="toggleModel('{}','{}',{})">{}</button></td></tr>"#,
                    u,
                    p,
                    if enabled { "Enabled" } else { "Disabled" },
                    button_class,
                    p.replace('\'', "\\'"),
                    u.replace('\'', "\\'"),
                    next_enabled,
                    button_label
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        None => "<tr><td class=\"label\" colspan=\"4\">No models discovered</td></tr>".into(),
    };
    format!(
        r#"{head}<title>Models | TokenScavenger</title></head><body>{nav}<h1>Models</h1><table><caption>Discovered and curated model catalog</caption><thead><tr><th>Model ID</th><th>Provider</th><th>Status</th><th>Actions</th></tr></thead><tbody>{models}</tbody></table><script>
async function toggleModel(provider_id, model_id, enabled) {{
  const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{models:[{{provider_id, model_id, enabled}}]}})}});
  if (r.ok) location.reload(); else alert('Model update failed');
}}
</script></body></html>"#,
        head = HTML_HEAD,
        nav = NAV,
        models = models_html
    )
}

/// Render the routing view.
pub async fn render_routing(state: &AppState) -> String {
    let config = state.config();
    let order = config.routing.provider_order.join(" → ");
    format!(
        r#"{head}<title>Routing | TokenScavenger</title></head><body>{nav}<h1>Routing</h1><div class="card"><div class="label">Provider Fallback Order</div><div class="mono mt-4">{order}</div><div class="label mt-4">Free First: {free_first} | Paid Fallback: {paid_fallback}</div></div></body></html>"#,
        head = HTML_HEAD,
        nav = NAV,
        order = order,
        free_first = config.routing.free_first,
        paid_fallback = config.routing.allow_paid_fallback
    )
}

/// Render the usage view.
pub async fn render_usage(state: &AppState) -> String {
    let series = crate::usage::aggregation::get_usage_series(state).await;
    let rows = match series.get("series").and_then(|s| s.as_array()) {
        Some(arr) => arr
            .iter()
            .map(|entry| {
                let p = entry
                    .get("provider_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let inp = entry
                    .get("input_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let out = entry
                    .get("output_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let cost = entry
                    .get("estimated_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                format!(
                    r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>${:.4}</td></tr>"#,
                    p, inp, out, cost
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        None => "<tr><td class=\"label\" colspan=\"4\">No usage data</td></tr>".into(),
    };
    format!(
        r#"{head}<title>Usage | TokenScavenger</title></head><body>{nav}<h1>Usage (Last 24h)</h1><table><caption>Usage totals grouped by provider</caption><thead><tr><th>Provider</th><th>Input Tokens</th><th>Output Tokens</th><th>Est. Cost</th></tr></thead><tbody>{rows}</tbody></table></body></html>"#,
        head = HTML_HEAD,
        nav = NAV,
        rows = rows
    )
}

/// Render the health view.
pub async fn render_health(state: &AppState) -> String {
    let mut rows = String::new();
    for entry in state.health_states.iter() {
        let pid = entry.key();
        let hs = entry.value();
        rows.push_str(&format!(
            r#"<tr><td>{}</td><td>{:?}</td><td>{}</td></tr>"#,
            pid,
            hs.value(),
            hs.recent_successes + hs.recent_failures
        ));
    }
    format!(
        r#"{head}<title>Health | TokenScavenger</title></head><body>{nav}<h1>Provider Health</h1><table><caption>Current in-memory provider health state</caption><thead><tr><th>Provider</th><th>State</th><th>Total Requests</th></tr></thead><tbody>{rows}</tbody></table></body></html>"#,
        head = HTML_HEAD,
        nav = NAV,
        rows = rows
    )
}

/// Render the logs view.
pub async fn render_logs(_state: &AppState) -> String {
    format!(
        r#"{head}<title>Logs | TokenScavenger</title></head><body>{nav}<h1>Log Stream</h1><div id="logs" class="logs" aria-live="polite"><p class="label">Connecting to log stream...</p></div><script>const es=new EventSource('/admin/logs/stream');es.onmessage=(e)=>{{const el=document.getElementById('logs');const line=document.createElement('div');line.textContent=e.data;el.appendChild(line);el.scrollTop=el.scrollHeight;}};</script></body></html>"#,
        head = HTML_HEAD,
        nav = NAV
    )
}

/// Render the configuration view.
pub async fn render_config(_state: &AppState) -> String {
    format!(
        r#"{head}<title>Configuration | TokenScavenger</title></head><body>{nav}<h1>Configuration</h1><div class="card"><h2>Alias Editor</h2><div class="actions mt-4"><input id="alias" placeholder="alias" aria-label="Alias"><input id="target" placeholder="target model" aria-label="Target model"><button class="btn" onclick="saveAlias()">Save Alias</button><form action="/admin/config" method="GET"><button type="submit" class="btn secondary">View Redacted Config</button></form></div></div><script>
async function saveAlias() {{
  const alias=document.getElementById('alias').value.trim();
  const target=document.getElementById('target').value.trim();
  if (!alias || !target) return alert('Alias and target are required');
  const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{aliases:[{{alias, target, enabled:true}}]}})}});
  if (r.ok) alert('Alias saved'); else alert('Alias save failed');
}}
</script></body></html>"#,
        head = HTML_HEAD,
        nav = NAV
    )
}

/// Render the audit history view.
pub async fn render_audit(_state: &AppState) -> String {
    format!(
        r#"{head}<title>Audit History | TokenScavenger</title></head><body>{nav}<h1>Audit History</h1><p class="label">Configuration change history is recorded as actions are performed.</p></body></html>"#,
        head = HTML_HEAD,
        nav = NAV
    )
}
