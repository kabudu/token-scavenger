import re

with open("src/ui/routes.rs", "r") as f:
    code = f.read()

# 1. Replace cdn.tailwindcss.com with compiled css
code = code.replace('<script src="https://cdn.tailwindcss.com"></script>', "")
code = code.replace(
    '<style>',
    '<style>\n{}'
)
code = code.replace(
    '</style>\n</head>"##, title);',
    '</style>\n</head>"##, title, include_str!("styles.css"));'
)

# 2. Add Modal system & Health badge to render_shell
old_health = r"""<div class="flex items-center gap-2 px-3 py-1.5 rounded-full bg-emerald-500/10 border border-emerald-500/20">
        <span class="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></span>
        <span class="text-[10px] font-bold text-emerald-500 uppercase tracking-wider">Operational</span>
        </div>"""
new_health = r"""<div class="flex items-center gap-2 px-3 py-1.5 rounded-full border" id="health-badge">
        <span class="w-2 h-2 rounded-full animate-pulse" id="health-badge-dot"></span>
        <span class="text-[10px] font-bold uppercase tracking-wider" id="health-badge-text">...</span>
        </div>"""
code = code.replace(old_health, new_health)

old_main_end = r"""</main>
<script>"""
new_main_end = r"""</main>
<div id="global-modal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-sm hidden opacity-0 transition-opacity duration-300">
    <div class="glass-card max-w-md w-full p-6 transform scale-95 transition-transform duration-300" id="global-modal-content">
        <h3 id="global-modal-title" class="text-lg font-bold mb-2"></h3>
        <p id="global-modal-message" class="text-sm text-slate-300 mb-6"></p>
        <div class="flex justify-end">
            <button class="btn" style="background:#334155;" onclick="hideModal()">Close</button>
        </div>
    </div>
</div>
<script>
function showModal(title, message, isError) {{
    document.getElementById('global-modal-title').innerText = title;
    document.getElementById('global-modal-title').className = `text-lg font-bold mb-2 ${{isError ? 'text-red-400' : 'text-emerald-400'}}`;
    document.getElementById('global-modal-message').innerText = (typeof message === 'object') ? JSON.stringify(message, null, 2) : message;
    if (typeof message === 'object') {{ document.getElementById('global-modal-message').classList.add('font-mono', 'whitespace-pre-wrap', 'text-[10px]'); }}
    const modal = document.getElementById('global-modal');
    modal.classList.remove('hidden');
    void modal.offsetWidth;
    modal.classList.remove('opacity-0');
    document.getElementById('global-modal-content').classList.remove('scale-95');
}}
function hideModal() {{
    const modal = document.getElementById('global-modal');
    modal.classList.add('opacity-0');
    document.getElementById('global-modal-content').classList.add('scale-95');
    setTimeout(() => {{ modal.classList.add('hidden'); }}, 300);
}}"""
code = code.replace(old_main_end, new_main_end)

old_js_end = r"""}, 1000);
</script>
{}
</body>
</html>"##, head, nav, uptime_str, title, content, uptime * 1000, scripts)"""
new_js_end = r"""}, 1000);

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
</html>"##, head, nav, uptime_str, title, content, uptime * 1000, {
    let health_map: std::collections::HashMap<_, _> = state.health_states.iter().map(|e| (e.key().clone(), serde_json::json!({ "state": format!("{:?}", e.value().state) }))).collect();
    serde_json::to_string(&health_map).unwrap_or("{}".into())
}, scripts)"""
code = code.replace(old_js_end, new_js_end)

# 3. Alerts -> Modals
code = code.replace("alert('Update failed')", "showModal('Error', 'Update failed', true)")
code = code.replace("alert(JSON.stringify(await r.json(), null, 2))", "const data = await r.json(); showModal('Test Result', data, data.status === 'error')")
code = code.replace("alert('Failed')", "showModal('Error', 'Operation failed', true)")
code = code.replace("alert('Required')", "showModal('Error', 'Required fields missing', true)")
code = code.replace("alert('Saved')", "showModal('Success', 'Saved successfully', false)")

# 4. Models update
old_models = r"""pub async fn render_models(state: &AppState) -> String {
    let models = crate::discovery::merge::get_all_models(state).await;
    let models_html = match models.get("models").and_then(|m| m.as_array()) {
        Some(arr) => arr
            .iter()
            .map(|m| {
                let u = m.get("upstream_model_id").and_then(|v| v.as_str()).unwrap_or("?");
                let p = m.get("provider_id").and_then(|v| v.as_str()).unwrap_or("?");
                let enabled = m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                let next_enabled = if enabled { "false" } else { "true" };
                let button_label = if enabled { "Disable" } else { "Enable" };
                let button_class = if enabled { "btn-danger" } else { "btn" };
                format!(r#"<tr><td class="font-mono text-sm text-cyan-400">{}</td><td class="text-sm">{}</td><td><span class="px-2 py-0.5 rounded {} text-[10px]">{}</span></td><td><button class="{}" onclick="toggleModel('{}','{}',{})">{}</button></td></tr>"#, u, p, if enabled { "bg-emerald-500/10 text-emerald-500" } else { "bg-white/5 text-slate-500" }, if enabled { "Enabled" } else { "Disabled" }, button_class, p.replace('\'', "\\'"), u.replace('\'', "\\'"), next_enabled, button_label)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        None => "<tr><td colspan=\"4\" class=\"px-6 py-4 text-center text-slate-500\">No models discovered</td></tr>".into(),
    };
    let content = format!(
        r#"
        <div class="glass-card overflow-hidden">
            <div class="px-6 py-4 border-b border-white/5 bg-white/[0.02]"><h3 class="font-bold">Model Catalog</h3></div>
            <div class="p-0 overflow-x-auto">
                <table class="w-full text-left"><thead class="text-slate-500 border-b border-white/5 bg-white/[0.01]"><tr><th>Model ID</th><th>Provider</th><th>Status</th><th>Actions</th></tr></thead><tbody class="divide-y divide-white/5">{}</tbody></table>
            </div>
        </div>"#,
        models_html
    );
    let scripts = r#"<script>
    async function toggleModel(provider_id, model_id, enabled) { const r = await fetch('/admin/config', {method:'PUT', headers:{'Content-Type':'application/json'}, body:JSON.stringify({models:[{provider_id, model_id, enabled}]})}); if (r.ok) location.reload(); else showModal('Error', 'Operation failed', true); }
    </script>"#;
    render_shell("Models", "models", &content, scripts, state)
}"""

new_models = r"""pub async fn render_models(state: &AppState) -> String {
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
                        <tr><th>Model ID</th><th>Provider</th><th>Status</th><th>Actions</th></tr>
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
    let scripts = format!(r#"<script>
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
                const status_html = enabled 
                    ? `<span class="px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-500 text-[10px]">Enabled</span>`
                    : `<span class="px-2 py-0.5 rounded bg-white/5 text-slate-500 text-[10px]">Disabled</span>`;
                html += `<tr><td class="font-mono text-sm text-cyan-400">${{u}}</td><td class="text-sm">${{p}}</td><td>${{status_html}}</td><td><button class="${{button_class}}" onclick="toggleModel('${{p.replace(/'/g, "\\'")}}','${{u.replace(/'/g, "\\'")}}',${{next_enabled}})">${{button_label}}</button></td></tr>`;
            }});
        }}
        document.getElementById('modelsTableBody').innerHTML = html;
        document.getElementById('pageInfo').innerText = `Page ${{currentPage}} of ${{totalPages}} (${{filtered.length}} total)`;
    }}

    function prevPage() {{ if (currentPage > 1) {{ currentPage--; renderTable(); }} }}
    function nextPage() {{ const search = document.getElementById('modelSearch').value.toLowerCase(); const prov = document.getElementById('providerFilter').value; const totalPages = Math.ceil(modelsArr.filter(m => (m.upstream_model_id || '').toLowerCase().includes(search) && (prov ? m.provider_id === prov : true)).length / pageSize); if (currentPage < totalPages) {{ currentPage++; renderTable(); }} }}

    async function toggleModel(provider_id, model_id, enabled) {{ const r = await fetch('/admin/config', {{method:'PUT', headers:{{'Content-Type':'application/json'}}, body:JSON.stringify({{models:[{{provider_id, model_id, enabled}}]}})}}); if (r.ok) location.reload(); else showModal('Error', 'Operation failed', true); }}
    
    renderTable();
    </script>"#, models_json);
    render_shell("Models", "models", &content, &scripts, state)
}"""

if old_models in code:
    code = code.replace(old_models, new_models)
else:
    print("WARNING: Could not replace render_models")

with open("src/ui/routes.rs", "w") as f:
    f.write(code)
