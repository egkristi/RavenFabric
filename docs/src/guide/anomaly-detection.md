# Behavioral Anomaly Detection

RavenFabric tracks per-identity behavioral baselines and alerts when an agent's activity deviates significantly from normal patterns. This catches compromised sessions, prompt-injected AI agents, and unusual operator activity without relying on signatures.

## What Is Monitored

| Signal | Description |
|--------|-------------|
| Command velocity | Commands per minute for the caller identity |
| Command novelty | Commands not previously seen from this identity |
| Path novelty | Filesystem paths not previously accessed |
| Timing patterns | Unusual times of day relative to historical baseline |
| Escalation attempts | Policy denials from an identity that rarely triggers denials |
| Session duration | Unusually long or unusually short sessions |
| Output volume | Unexpectedly large output volumes |

Each signal produces a score from 0.0 (normal) to 1.0 (highly anomalous). Scores are combined into a per-session anomaly score.

---

## Configuration

```toml
[anomaly_detection]
enabled = true
baseline_window_hours = 168     # use last 7 days to build baseline
min_baseline_events = 50        # minimum events before scoring starts
score_threshold_warn = 0.5      # log warning, trigger WARN alert
score_threshold_auto = 0.8      # trigger AUTO_CONTAIN alert
score_threshold_block = 0.95    # refuse further actions from this session

[anomaly_detection.signals]
command_velocity.enabled = true
command_velocity.baseline_multiplier = 3.0   # flag if >3x normal rate

command_novelty.enabled = true
command_novelty.threshold = 0.4              # flag if >40% novel commands

timing.enabled = true
timing.off_hours_multiplier = 2.0

escalation.enabled = true
escalation.deny_rate_threshold = 0.3         # flag if >30% of commands denied

output_volume.enabled = true
output_volume.baseline_multiplier = 5.0
```

---

## Anomaly Scores in the Audit Log

Every audit event includes the current anomaly score for the caller:

```json
{
  "seq": 5201,
  "ts": "2026-05-21T14:22:00Z",
  "action": "exec",
  "command": "cat /etc/passwd",
  "decision": "allow",
  "caller": "f7a3..c912",
  "anomaly_score": 0.72,
  "anomaly_signals": ["command_novelty", "off_hours"]
}
```

---

## Auto-Containment

When the anomaly score exceeds `score_threshold_auto`, the agent automatically reduces the caller's capabilities without terminating the session:

- Commands that were previously allowed are re-checked against a stricter inline policy
- File access is restricted to read-only for paths outside the baseline
- Output limits are halved
- A `ANOMALY_AUTO_CONTAIN` event is written to the audit log

When the score exceeds `score_threshold_block`, the session is terminated:

```
Session f7a3..c912 terminated — anomaly score 0.97 exceeded block threshold
Audit entry: seq=5214, event=ANOMALY_SESSION_BLOCK
```

To re-admit a blocked session, the operator generates a new meet token.

---

## Alert Rules for Anomaly Events

Combine anomaly detection with alert rules to notify on suspicious activity:

```toml
[[audit.alerts]]
name = "anomaly-ai-agent"
description = "AI agent anomaly score above threshold"
condition = "event.caller_type == 'ai_agent' && event.anomaly_score >= 0.5"
destinations = ["slack-security"]
deduplicate_window_seconds = 300

[[audit.alerts]]
name = "anomaly-auto-contain"
description = "Auto-containment triggered"
condition = "event.event_type == 'ANOMALY_AUTO_CONTAIN'"
destinations = ["slack-security", "pagerduty-infra"]
severity = "high"

[[audit.alerts]]
name = "anomaly-block"
description = "Session blocked due to anomaly"
condition = "event.event_type == 'ANOMALY_SESSION_BLOCK'"
destinations = ["slack-security", "pagerduty-infra"]
severity = "critical"
```

---

## Baseline Management

### Viewing the Current Baseline

```bash
rf anomaly baseline --token <TOKEN> --caller <CALLER_FINGERPRINT>
```

Output:
```
Baseline for caller f7a3..c912 (ai-deploy-bot)
  Observation window: 2026-05-14 – 2026-05-21
  Total events in baseline: 3,420

  Command velocity:  avg 2.3/min, max 12/min, std 1.8
  Command novelty:   2.1% novel commands per session (baseline)
  Active hours:      Mon–Fri 08:00–18:00 UTC
  Deny rate:         0.4%
```

### Resetting the Baseline

After a planned operational change (a new deployment process, new AI agent behavior), reset the baseline so the detection adapts:

```bash
rf anomaly reset --token <TOKEN> --caller <CALLER_FINGERPRINT>
```

### Per-Identity Thresholds

Override global thresholds for specific callers:

```toml
[[anomaly_detection.per_identity]]
caller = "f7a3..c912"               # AI deploy bot — expected high velocity
command_velocity.baseline_multiplier = 10.0
command_novelty.enabled = false     # deploy bots run known commands only
```

---

## Prompt Injection Detection

For AI agent sessions, RavenFabric applies additional heuristics to detect prompt injection attempts — adversarial instructions hidden in documents or tool outputs that try to manipulate the AI into issuing unauthorized commands.

Detection signals:
- Base64-encoded payloads in command arguments
- Unicode homoglyphs in command strings
- `eval`, `exec`, `base64 -d |` patterns in commands from AI sessions
- Sudden change in command category after a file-read or web-fetch operation

When an injection signal is detected:
1. The session's anomaly score is incremented
2. A `PROMPT_INJECTION_SIGNAL` event is written to the audit log
3. If the score exceeds the containment threshold, capabilities are reduced

The policy engine remains the final defence — even if an injection succeeds in generating a command, the command still goes through the deny-by-default policy check before execution.

---

## See Also

- [SIEM Integration](siem-integration.md) — Routing anomaly alerts to Slack, PagerDuty, and SIEM
- [AI Agent Security Layer](../use-cases/ai-agent-access.md) — Full AI access model
- [Audit Log Format](../reference/audit-log-format.md) — Anomaly event schema
- [Compliance: EU AI Act](../compliance/frameworks/nis2-directive.md) — Anomaly detection as AI oversight evidence
