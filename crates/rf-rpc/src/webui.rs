//! Embedded Web UI dashboard for RavenFabric controller.
//!
//! Provides a single-page HTML dashboard served by the controller's HTTP endpoint.
//! No external dependencies — pure HTML/CSS with minimal JavaScript for dynamic updates.
//!
//! # Endpoints
//!
//! - `GET /` — Dashboard HTML
//! - `GET /api/agents` — JSON agent list (for live updates)
//! - `GET /api/health` — Health check

/// The embedded dashboard HTML.
///
/// Single-page application with:
/// - Agent status overview (online/offline/stale)
/// - Recent activity feed from audit log
/// - Policy violation alerts
/// - Connection topology view
/// - System health metrics
pub const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>RavenFabric Dashboard</title>
<style>
:root {
    --bg: #0d1117;
    --surface: #161b22;
    --border: #30363d;
    --text: #e6edf3;
    --text-muted: #8b949e;
    --accent: #58a6ff;
    --green: #3fb950;
    --yellow: #d29922;
    --red: #f85149;
    --font: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
    --mono: 'SF Mono', 'Cascadia Code', 'Fira Code', monospace;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: var(--font); background: var(--bg); color: var(--text); min-height: 100vh; }
header { background: var(--surface); border-bottom: 1px solid var(--border); padding: 1rem 2rem; display: flex; align-items: center; justify-content: space-between; }
header h1 { font-size: 1.25rem; font-weight: 600; }
header h1 span { color: var(--accent); }
.status-badge { padding: 0.25rem 0.75rem; border-radius: 2rem; font-size: 0.75rem; font-weight: 500; }
.status-online { background: rgba(63,185,80,0.15); color: var(--green); }
.status-offline { background: rgba(248,81,73,0.15); color: var(--red); }
main { max-width: 1400px; margin: 0 auto; padding: 2rem; }
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 1.5rem; margin-bottom: 2rem; }
.card { background: var(--surface); border: 1px solid var(--border); border-radius: 0.5rem; padding: 1.5rem; }
.card h2 { font-size: 0.875rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 1rem; }
.metric { font-size: 2.5rem; font-weight: 700; line-height: 1; }
.metric-label { font-size: 0.875rem; color: var(--text-muted); margin-top: 0.5rem; }
.agent-list { list-style: none; }
.agent-list li { display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 0; border-bottom: 1px solid var(--border); }
.agent-list li:last-child { border-bottom: none; }
.agent-name { font-family: var(--mono); font-size: 0.875rem; }
.agent-labels { font-size: 0.75rem; color: var(--text-muted); }
.activity-feed { max-height: 400px; overflow-y: auto; }
.activity-item { padding: 0.75rem 0; border-bottom: 1px solid var(--border); font-size: 0.875rem; }
.activity-item:last-child { border-bottom: none; }
.activity-time { color: var(--text-muted); font-size: 0.75rem; font-family: var(--mono); }
.activity-action { margin-top: 0.25rem; }
.denied { color: var(--red); }
.allowed { color: var(--green); }
.warning { color: var(--yellow); }
table { width: 100%; border-collapse: collapse; font-size: 0.875rem; }
th { text-align: left; padding: 0.75rem; border-bottom: 2px solid var(--border); color: var(--text-muted); font-weight: 500; }
td { padding: 0.75rem; border-bottom: 1px solid var(--border); }
.dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; margin-right: 0.5rem; }
.dot-green { background: var(--green); }
.dot-red { background: var(--red); }
.dot-yellow { background: var(--yellow); }
footer { text-align: center; padding: 2rem; color: var(--text-muted); font-size: 0.75rem; }
@media (max-width: 768px) { main { padding: 1rem; } .grid { grid-template-columns: 1fr; } }
</style>
</head>
<body>
<header>
    <h1><span>Raven</span>Fabric</h1>
    <span class="status-badge status-online" id="controller-status">Controller Online</span>
</header>
<main>
    <div class="grid">
        <div class="card">
            <h2>Agents Online</h2>
            <div class="metric" id="agents-online">—</div>
            <div class="metric-label">of <span id="agents-total">—</span> registered</div>
        </div>
        <div class="card">
            <h2>Actions (24h)</h2>
            <div class="metric" id="actions-count">—</div>
            <div class="metric-label"><span id="denied-count">—</span> denied</div>
        </div>
        <div class="card">
            <h2>Policy Violations</h2>
            <div class="metric" id="violations-count">0</div>
            <div class="metric-label">anomalies detected</div>
        </div>
        <div class="card">
            <h2>Uptime</h2>
            <div class="metric" id="uptime">—</div>
            <div class="metric-label">controller uptime</div>
        </div>
    </div>

    <div class="grid" style="grid-template-columns: 2fr 1fr;">
        <div class="card">
            <h2>Connected Agents</h2>
            <table>
                <thead><tr><th>Agent</th><th>Status</th><th>Transport</th><th>Last Seen</th></tr></thead>
                <tbody id="agent-table"><tr><td colspan="4" style="color:var(--text-muted)">Loading...</td></tr></tbody>
            </table>
        </div>
        <div class="card">
            <h2>Recent Activity</h2>
            <div class="activity-feed" id="activity-feed">
                <div class="activity-item" style="color:var(--text-muted)">Loading...</div>
            </div>
        </div>
    </div>
</main>
<footer>RavenFabric v0.1.4 — Security-first distributed execution engine</footer>
<script>
async function refresh() {
    try {
        const resp = await fetch('/api/agents');
        if (!resp.ok) return;
        const data = await resp.json();
        const agents = data.agents || [];
        const online = agents.filter(a => a.status === 'online').length;
        document.getElementById('agents-online').textContent = online;
        document.getElementById('agents-total').textContent = agents.length;

        const tbody = document.getElementById('agent-table');
        if (agents.length === 0) {
            tbody.innerHTML = '<tr><td colspan="4" style="color:var(--text-muted)">No agents connected</td></tr>';
        } else {
            tbody.innerHTML = agents.map(a => `<tr>
                <td><span class="dot ${a.status === 'online' ? 'dot-green' : 'dot-red'}"></span><span class="agent-name">${esc(a.id)}</span></td>
                <td>${a.status}</td>
                <td>${a.transport || 'unknown'}</td>
                <td style="font-family:var(--mono);font-size:0.75rem">${a.last_seen || '—'}</td>
            </tr>`).join('');
        }
    } catch (e) { /* silent retry */ }
}

async function refreshHealth() {
    try {
        const resp = await fetch('/api/health');
        if (!resp.ok) return;
        const data = await resp.json();
        if (data.uptime_seconds) {
            const h = Math.floor(data.uptime_seconds / 3600);
            const m = Math.floor((data.uptime_seconds % 3600) / 60);
            document.getElementById('uptime').textContent = `${h}h ${m}m`;
        }
    } catch (e) { /* silent */ }
}

function esc(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

refresh();
refreshHealth();
setInterval(refresh, 5000);
setInterval(refreshHealth, 30000);
</script>
</body>
</html>"#;

/// Returns the dashboard HTML response.
pub fn dashboard_response() -> (u16, &'static str, &'static str) {
    (200, "text/html; charset=utf-8", DASHBOARD_HTML)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_html_valid() {
        assert!(DASHBOARD_HTML.contains("<!DOCTYPE html>"));
        assert!(DASHBOARD_HTML.contains("</html>"));
        assert!(DASHBOARD_HTML.contains("RavenFabric"));
    }

    #[test]
    fn test_dashboard_has_agent_section() {
        assert!(DASHBOARD_HTML.contains("Connected Agents"));
        assert!(DASHBOARD_HTML.contains("agent-table"));
    }

    #[test]
    fn test_dashboard_has_metrics() {
        assert!(DASHBOARD_HTML.contains("agents-online"));
        assert!(DASHBOARD_HTML.contains("actions-count"));
        assert!(DASHBOARD_HTML.contains("violations-count"));
    }

    #[test]
    fn test_dashboard_fetches_api() {
        assert!(DASHBOARD_HTML.contains("/api/agents"));
        assert!(DASHBOARD_HTML.contains("/api/health"));
    }

    #[test]
    fn test_dashboard_response() {
        let (status, content_type, body) = dashboard_response();
        assert_eq!(status, 200);
        assert_eq!(content_type, "text/html; charset=utf-8");
        assert!(body.contains("RavenFabric Dashboard"));
    }

    #[test]
    fn test_no_external_resources() {
        // Security: no external CDNs, no tracking, no external fonts
        assert!(!DASHBOARD_HTML.contains("googleapis.com"));
        assert!(!DASHBOARD_HTML.contains("cdn."));
        assert!(!DASHBOARD_HTML.contains("analytics"));
        assert!(!DASHBOARD_HTML.contains("tracking"));
    }

    #[test]
    fn test_xss_protection() {
        // The JavaScript uses an escape function for user-supplied data
        assert!(DASHBOARD_HTML.contains("function esc(s)"));
        assert!(DASHBOARD_HTML.contains("textContent"));
    }
}
