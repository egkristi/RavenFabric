# Playbooks — Multi-Agent Orchestration

Playbooks let you execute commands across multiple agents in a single operation,
with configurable rollout strategies, failure policies, and rollback support.

## Overview

A playbook is a YAML file that defines:

- A **command** to execute on target agents
- **Target selection** — which agents to run on
- **Rollout strategy** — how to roll out across agents (parallel, sequential, rolling, canary)
- **Failure policy** — what to do when an agent fails
- **Timeout** — per-agent execution timeout

```bash
rf playbook --file deploy.yaml --token <agent-token>
```

## YAML Schema

```yaml
command: "<shell command>"
target: <target-spec>
strategy: <strategy-spec>
on_failure: <failure-spec>
timeout_secs: <integer>
```

### Fields

| Field | Required | Description |
|---|---|---|
| `command` | Yes | Shell command to execute on each target agent. Supports YAML `\|` for multi-line. |
| `target` | Yes | Target agent selection (see below). |
| `strategy` | Yes | Rollout strategy (see below). |
| `on_failure` | Yes | Failure handling policy (see below). |
| `timeout_secs` | Yes | Maximum seconds to wait per-agent before timing out. |

## Target Selection

| Tag | Example | Description |
|---|---|---|
| `!agents` | `!agents [web-01, web-02]` | Target specific agents by ID. |
| `!label` | `!label {key: env, value: prod}` | Target agents matching a label. |
| `!group` | `!group web-fleet` | Target all agents in a named group. |
| `!pattern` | `!pattern "web-*"` | Target agents whose ID matches a glob pattern. |
| `!all` | `!all` | Target all connected agents. |

> **Note:** The CLI currently supports `!agents` targeting. Other target types
> require an agent registry (planned for v1.0).

## Rollout Strategies

### Parallel

Execute on all target agents simultaneously.

```yaml
strategy: parallel
```

Best for: read-only queries, health checks, data collection — operations where
agent independence is guaranteed.

### Sequential

Execute on one agent at a time. Stops on first failure.

```yaml
strategy: sequential
```

Best for: ordered operations, dependency chains, debugging.

### Rolling Update

Execute in batches, waiting for each batch to succeed before proceeding.

```yaml
strategy: !rolling {batch_percent: 25}
```

| Parameter | Required | Description |
|---|---|---|
| `batch_percent` | Yes | Percentage of agents per batch (1–100). |

Best for: service updates, configuration changes — operations where you want to
limit blast radius.

### Canary Deploy

Execute on a small canary group first. If the canary succeeds, proceed to all
remaining agents. If the canary fails, stop immediately.

```yaml
strategy: !canary {canary_count: 1}
```

| Parameter | Required | Description |
|---|---|---|
| `canary_count` | Yes | Number of agents in the canary group. |

Best for: risky deployments, configuration changes — validate on a subset before
full rollout.

## Failure Policies

### Stop Only

Stop execution on failure. Do not roll back already-succeeded agents.

```yaml
on_failure: stop_only
```

### Rollback

Stop execution and run a rollback command on agents that already succeeded.

```yaml
on_failure: !rollback {command: "systemctl revert myapp"}
```

| Parameter | Required | Description |
|---|---|---|
| `command` | Yes | Shell command to run on each succeeded agent during rollback. |

### Continue

Continue execution despite failures (best-effort).

```yaml
on_failure: continue
```

Best for: data collection, monitoring sweeps — operations where partial results
are still valuable.

## Examples

### Parallel Health Check

```yaml
# healthcheck.yaml
command: "uptime && df -h / | tail -1 && free -h | grep Mem"
target: !agents [web-01, web-02, db-01]
strategy: parallel
on_failure: continue
timeout_secs: 30
```

```bash
rf playbook --file healthcheck.yaml --token <token>
```

### Canary Deploy with Rollback

```yaml
# deploy.yaml
command: |
  echo "Deploying v2.0 on $(hostname)"
  mkdir -p /opt/app
  echo "v2.0" > /opt/app/version.txt
  systemctl restart myapp
target: !agents [web-01, web-02, web-03, web-04]
strategy: !canary {canary_count: 1}
on_failure: !rollback {command: "systemctl revert myapp"}
timeout_secs: 60
```

```bash
rf playbook --file deploy.yaml --token <token>
```

### Rolling OS Update

```yaml
# os-update.yaml
command: "apt-get update -qq && apt-get upgrade -y && reboot"
target: !agents [node-01, node-02, node-03, node-04]
strategy: !rolling {batch_percent: 25}
on_failure: stop_only
timeout_secs: 300
```

```bash
rf playbook --file os-update.yaml --token <token>
```

### Sequential Log Collection

```yaml
# logs.yaml
command: |
  echo "=== $(hostname) ==="
  journalctl -n 50 --no-pager
  echo "=== disk ==="
  df -h
target: !agents [web-01, web-02, db-01]
strategy: sequential
on_failure: continue
timeout_secs: 60
```

```bash
rf playbook --file logs.yaml --token <token>
```

## Output

The playbook command prints results per-agent as they complete:

```
Agent web-01: OK (exit=0, 1.2s, 2.3 KB stdout)
Agent web-02: OK (exit=0, 1.1s, 2.1 KB stdout)
Agent web-03: FAILED (exit=1, 0.5s, "command not found")
  → Rollback triggered on web-01, web-02
Agent web-04: SKIPPED (rollback)
```

## Security

- Playbook execution respects the agent's **deny-by-default policy engine**.
- Each agent evaluates the command against its local policy before execution.
- All playbook actions produce structured audit log entries.
- The agent token must be provided via `--token` or `RAVENFABRIC_TOKEN` env var.
