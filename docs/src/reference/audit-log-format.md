# Audit Log Format

RavenFabric produces structured JSON-lines audit logs. Every RPC action generates exactly one audit entry — no exceptions.

## Log Location

Default: `/var/log/ravenfabric/audit.jsonl` (configurable via `audit_path` in `raven.toml`)

## Entry Format

Each line is a self-contained JSON object:

```json
{
  "timestamp": "2026-05-06T14:30:00.123Z",
  "action": "exec",
  "command": "systemctl status nginx",
  "agent_id": "prod-web-01",
  "caller": "alice@example.com",
  "decision": "allowed",
  "rule": "^systemctl status .*$",
  "result": "success",
  "exit_code": 0,
  "duration_ms": 142,
  "output_bytes": 1847
}
```

## Fields

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | string (ISO 8601) | When the action occurred (UTC) |
| `action` | string | Action type: `exec`, `file_read`, `file_write`, `metrics`, `shell`, `port_forward` |
| `command` | string | The command or operation requested |
| `agent_id` | string | Agent that processed the request |
| `caller` | string | Identity of the requester (public key fingerprint or alias) |
| `decision` | string | `allowed` or `denied` |
| `rule` | string | The policy rule that matched (allow or deny pattern) |
| `result` | string | `success`, `failed`, `timeout`, `denied` |
| `exit_code` | integer \| null | Process exit code (for exec actions) |
| `duration_ms` | integer | Execution time in milliseconds |
| `output_bytes` | integer | Size of output produced |
| `error` | string \| null | Error message if result is `failed` |

## Action Types

### `exec` — Command execution

```json
{
  "timestamp": "2026-05-06T14:30:00Z",
  "action": "exec",
  "command": "df -h",
  "agent_id": "prod-web-01",
  "caller": "alice@example.com",
  "decision": "allowed",
  "rule": "^df -h$",
  "result": "success",
  "exit_code": 0,
  "duration_ms": 12,
  "output_bytes": 423
}
```

### `exec` — Denied command

```json
{
  "timestamp": "2026-05-06T14:31:00Z",
  "action": "exec",
  "command": "rm -rf /tmp/*",
  "agent_id": "prod-web-01",
  "caller": "alice@example.com",
  "decision": "denied",
  "rule": ".*rm.*-rf.*",
  "result": "denied",
  "exit_code": null,
  "duration_ms": 0,
  "output_bytes": 0
}
```

### `metrics` — System metrics collection

```json
{
  "timestamp": "2026-05-06T14:32:00Z",
  "action": "metrics",
  "command": "system_metrics",
  "agent_id": "prod-web-01",
  "caller": "monitoring@example.com",
  "decision": "allowed",
  "rule": "builtin:metrics",
  "result": "success",
  "exit_code": null,
  "duration_ms": 5,
  "output_bytes": 2048
}
```

## Parsing Examples

### Count denied actions today

```bash
jq -r 'select(.decision == "denied") | .command' /var/log/ravenfabric/audit.jsonl | sort | uniq -c | sort -rn
```

### List all callers

```bash
jq -r '.caller' /var/log/ravenfabric/audit.jsonl | sort -u
```

### Find slow commands (> 5s)

```bash
jq 'select(.duration_ms > 5000)' /var/log/ravenfabric/audit.jsonl
```

### Export to CSV

```bash
jq -r '[.timestamp, .action, .caller, .command, .decision, .result] | @csv' \
  /var/log/ravenfabric/audit.jsonl > audit-export.csv
```

## Integrity Properties

- **Append-only** — no delete or truncate operations supported
- **Every action logged** — denied commands are logged too
- **Tamper-evident** — structured format makes gaps or modifications detectable
- **Rotation-safe** — use `copytruncate` in logrotate to avoid losing entries

## Integration

The JSON-lines format integrates directly with:

- **Elasticsearch / OpenSearch** — direct ingest via Filebeat
- **Splunk** — JSON sourcetype, automatic field extraction
- **Grafana Loki** — promtail scraping with JSON parser
- **SIEM systems** — any tool that reads structured JSON logs
- **jq** — command-line analysis and filtering
