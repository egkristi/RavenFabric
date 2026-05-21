# SIEM Integration & Alerting

RavenFabric forwards audit events to external security information and event management (SIEM) systems in real time. Configure one or more destinations; the agent buffers events locally during network interruptions and delivers them without gaps when connectivity resumes.

## Supported Destinations

| Destination | Format | Transport |
|-------------|--------|-----------|
| Splunk HEC | JSON | HTTPS POST |
| Elasticsearch | JSON | HTTPS bulk API |
| Datadog | JSON | HTTPS POST |
| Syslog (RFC 5424) | Syslog | TCP / UDP / TLS |
| Generic webhook | JSON | HTTPS POST |

## Supported Export Formats

| Format | Standard | Use case |
|--------|----------|----------|
| Native JSON-lines | RavenFabric | Default, richest data |
| CEF | ArcSight Common Event Format | SIEM interop |
| LEEF | IBM QRadar LEEF | QRadar integration |
| OCSF | Open Cybersecurity Schema Framework | Cross-vendor normalization |

---

## Configuration

SIEM destinations are configured in `raven.toml`:

### Splunk HEC

```toml
[[audit.destinations]]
type = "splunk_hec"
url = "https://splunk.internal:8088/services/collector"
token = "env:SPLUNK_HEC_TOKEN"
index = "ravenfabric"
source_type = "ravenfabric:audit"
tls_verify = true

# Optional batching
batch_size = 100
flush_interval_ms = 5000
```

### Elasticsearch

```toml
[[audit.destinations]]
type = "elasticsearch"
url = "https://elastic.internal:9200"
index = "ravenfabric-audit"
username = "env:ELASTIC_USER"
password = "env:ELASTIC_PASSWORD"
# Or API key:
# api_key = "env:ELASTIC_API_KEY"

batch_size = 200
flush_interval_ms = 3000
```

### Datadog

```toml
[[audit.destinations]]
type = "datadog"
api_key = "env:DATADOG_API_KEY"
site = "datadoghq.eu"              # or datadoghq.com
service = "ravenfabric"
tags = ["env:prod", "team:infra"]
```

### Syslog (RFC 5424)

```toml
[[audit.destinations]]
type = "syslog"
address = "siem.internal:514"
transport = "tcp_tls"             # tcp | udp | tcp_tls
facility = "local0"
app_name = "ravenfabric"
```

### Generic Webhook

```toml
[[audit.destinations]]
type = "webhook"
url = "https://ingest.internal/events"
method = "POST"
headers = { "Authorization" = "env:INGEST_TOKEN", "Content-Type" = "application/json" }
format = "json"                   # json | cef | leef | ocsf
```

---

## Export Formats

### CEF (Common Event Format)

```toml
[[audit.destinations]]
type = "syslog"
address = "arcsight.internal:514"
format = "cef"
cef_device_vendor = "RavenFabric"
cef_device_product = "RavenFabric Agent"
cef_device_version = "0.20.0"
```

CEF output example:
```
CEF:0|RavenFabric|RavenFabric Agent|0.20.0|exec|Command Execution|5|
  src=10.0.1.5 suser=f7a3c912 msg=systemctl status nginx
  outcome=allow durationMs=105
```

### LEEF (IBM QRadar)

```toml
[[audit.destinations]]
type = "syslog"
address = "qradar.internal:514"
format = "leef"
```

### OCSF (Open Cybersecurity Schema Framework)

```toml
[[audit.destinations]]
type = "webhook"
url = "https://ocsf-ingestor.internal/api/events"
format = "ocsf"
ocsf_class = "process_activity"   # activity_class_uid
```

---

## Buffered Delivery

If a SIEM destination is temporarily unavailable, the agent writes events to a local buffer:

```toml
[audit]
buffer_path = "/var/lib/ravenfabric/audit-buffer"
buffer_max_mb = 512               # max buffer size on disk
retry_interval_seconds = 30       # how often to retry failed deliveries
max_retry_age_hours = 48          # discard events older than this on delivery
```

Events are delivered in order when the destination comes back online. The local JSON-lines audit log is always written first — the buffer is a delivery retry queue, not the primary record.

---

## Alert Rules

Alert rules fire on specific event patterns and route to notification destinations. They are evaluated against the audit stream in real time.

```toml
[[audit.alerts]]
name = "policy-denials"
description = "Notify when any command is denied by policy"
condition = "event.decision == 'deny'"
destinations = ["slack-security", "pagerduty-infra"]
deduplicate_window_seconds = 300   # suppress duplicates for 5 min

[[audit.alerts]]
name = "anomaly-high"
description = "High anomaly score detected"
condition = "event.anomaly_score >= 0.8"
destinations = ["slack-security"]
severity = "high"

[[audit.alerts]]
name = "root-commands"
description = "Alert when a command is run as root by an AI agent"
condition = "event.caller_type == 'ai_agent' && event.euid == 0"
destinations = ["slack-security", "pagerduty-infra"]
```

### Alert Condition Syntax

Conditions use a simple expression language over event fields:

| Field | Type | Description |
|-------|------|-------------|
| `event.decision` | string | `allow` or `deny` |
| `event.action` | string | `exec`, `file_push`, `shell`, `forward`, … |
| `event.caller` | string | Caller public key fingerprint |
| `event.caller_type` | string | `human`, `ai_agent`, `service` |
| `event.command` | string | The command string |
| `event.anomaly_score` | float | 0.0–1.0 (0 = normal, 1 = highly anomalous) |
| `event.path` | string | Filesystem path (for file operations) |
| `event.euid` | int | Effective UID on the agent |
| `event.exit_code` | int | Process exit code |

Operators: `==`, `!=`, `>`, `>=`, `<`, `<=`, `&&`, `||`, `!`, `contains`, `matches` (regex)

---

## Alert Destinations

### Slack

```toml
[[audit.alert_destinations]]
name = "slack-security"
type = "slack"
webhook_url = "env:SLACK_WEBHOOK_URL"
channel = "#security-alerts"
username = "RavenFabric"
```

### PagerDuty

```toml
[[audit.alert_destinations]]
name = "pagerduty-infra"
type = "pagerduty"
routing_key = "env:PD_ROUTING_KEY"
severity_map = { high = "critical", medium = "warning", low = "info" }
```

### OpsGenie

```toml
[[audit.alert_destinations]]
name = "opsgenie-oncall"
type = "opsgenie"
api_key = "env:OPSGENIE_API_KEY"
team = "sre"
priority_map = { high = "P1", medium = "P3" }
```

### Generic Webhook

```toml
[[audit.alert_destinations]]
name = "custom-ingestor"
type = "webhook"
url = "https://alerts.internal/api/alert"
method = "POST"
headers = { "Authorization" = "env:ALERT_TOKEN" }
template = """
{
  "title": "{{ alert.name }}",
  "severity": "{{ alert.severity }}",
  "agent": "{{ event.agent_id }}",
  "message": "{{ event.command }}"
}
"""
```

---

## Testing SIEM Integration

Send a test event to verify connectivity:

```bash
rf audit test --token <TOKEN> --destination splunk_hec
```

Query recent audit events from the agent:

```bash
# Show last 20 audit entries
rf audit query --token <TOKEN> --limit 20

# Show only denied events in the last hour
rf audit query --token <TOKEN> --decision deny --since 1h

# Show events for a specific action
rf audit query --token <TOKEN> --action exec --since 24h
```

---

## See Also

- [Anomaly Detection](anomaly-detection.md) — Behavioral baselines and auto-containment
- [Audit Log Format](../reference/audit-log-format.md) — Full event schema
- [Policy Configuration](policy-config.md) — Generating deny events
- [Compliance: NIS2](../compliance/frameworks/nis2-directive.md) — SIEM as NIS2 incident detection evidence
