# Fleet Orchestration

RavenFabric supports coordinated operations across multiple agents: ordered playbooks, parallel execution, rolling deployments, and automatic rollback on failure.

## Concepts

| Concept | Description |
|---------|-------------|
| Playbook | Ordered list of steps executed across one or more agents |
| Rolling deployment | Execute steps on agents one-by-one (or in batches) with health checks between waves |
| Rollback | If a step fails, preceding steps on affected agents are automatically reversed |
| Background job | Long-running task with an ID that can be queried or awaited |

---

## Playbooks

### Basic Playbook

```yaml
# deploy.yaml
name: Deploy myapp v2.1
agents:
  - web-01
  - web-02
  - web-03

steps:
  - name: Stop service
    command: "systemctl stop myapp"
    rollback: "systemctl start myapp"

  - name: Deploy binary
    command: "cp /opt/deploy/myapp-2.1 /opt/app/myapp && chmod +x /opt/app/myapp"
    rollback: "cp /opt/deploy/myapp-2.0 /opt/app/myapp"

  - name: Start service
    command: "systemctl start myapp"
    rollback: "systemctl stop myapp"

  - name: Health check
    command: "curl -sf http://localhost:8080/health"
    retries: 5
    retry_interval_seconds: 3
```

Run it:

```bash
rf playbook deploy.yaml --token <TOKEN>
```

Output:

```
Running playbook: Deploy myapp v2.1
Targets: web-01, web-02, web-03

[web-01] Stop service       ✓  142ms
[web-01] Deploy binary      ✓  320ms
[web-01] Start service      ✓  89ms
[web-01] Health check       ✓  1.2s (attempt 2/5)

[web-02] Stop service       ✓  138ms
[web-02] Deploy binary      ✓  308ms
...

Playbook complete. 3 agents, 12 steps, 0 failures.
Audit entries: 12.
```

### Rolling Deployment

Apply changes to one agent at a time (or in batches), with a health check gate between waves:

```yaml
name: Rolling deploy
agents:
  selector:
    labels:
      role: web

rolling:
  batch_size: 1           # agents per wave (or "25%" for percentage)
  pause_seconds: 10       # wait between waves
  health_check:
    command: "curl -sf http://localhost:8080/health"
    retries: 10
    retry_interval_seconds: 3
  abort_on_failure: true  # stop rolling out if any agent's health check fails

steps:
  - name: Upgrade
    command: "apt-get install -y myapp=2.1.0"
    rollback: "apt-get install -y myapp=2.0.0"
  - name: Reload
    command: "systemctl reload myapp"
```

```bash
rf playbook rolling-deploy.yaml --token <TOKEN>
```

### Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--token <TOKEN>` | `-t` | Meet token | Required |
| `--dry-run` | — | Show what would execute without running | — |
| `--parallel` | — | Run steps across all agents in parallel | Sequential |
| `--timeout <SECS>` | — | Per-step timeout override | Policy default |
| `--continue-on-error` | — | Do not abort on first failure | Abort |

---

## Automatic Rollback

If any step fails, the playbook automatically runs the `rollback` command for all preceding steps that succeeded on that agent:

```
[web-02] Deploy binary      ✗  exit code 1 — disk full
[web-02] Rolling back...
[web-02] Restore binary     ✓  restored myapp-2.0
[web-02] Start service      ✓  service running again
```

Rollback is executed in reverse step order. If a rollback command itself fails, the failure is logged and subsequent rollback steps continue.

---

## Background Jobs

For long-running tasks, submit as a background job:

```bash
# Start a background job — returns immediately with a job ID
rf exec --token <TOKEN> --background "apt-get dist-upgrade -y"
```

Output:

```
job_id: bf3a9c12
pid: 4821
started: 2026-05-21T10:30:00Z
```

Query the job status:

```bash
rf job status --token <TOKEN> --id bf3a9c12
```

Wait for completion:

```bash
rf job wait --token <TOKEN> --id bf3a9c12
```

Stream output of a running background job:

```bash
rf job logs --token <TOKEN> --id bf3a9c12 --follow
```

---

## Multi-Agent Parallel Execution

Run the same command on multiple agents at once without a playbook:

```bash
# Run on named agents
rf exec --token <TOKEN> --agents "web-01,web-02,web-03" "hostname"

# Run on agents matching a label selector
rf exec --token <TOKEN> --selector "role=web,env=prod" "systemctl status nginx"
```

Results are printed as they arrive, tagged with the agent ID:

```
[web-01]  web-01.internal
[web-03]  web-03.internal
[web-02]  web-02.internal
```

Use `--ordered` to print results in agent-list order rather than arrival order.

---

## Event Triggers

Agents can execute commands in response to events, without polling from the controller:

```yaml
# raven.toml
[[agent.triggers]]
event = "cron"
schedule = "0 * * * *"         # every hour
command = "logrotate /etc/logrotate.conf"

[[agent.triggers]]
event = "file_change"
path = "/etc/nginx/conf.d/"
command = "nginx -t && systemctl reload nginx"

[[agent.triggers]]
event = "process_exit"
process = "myapp"
command = "systemctl restart myapp"
exit_codes = [1, 2]             # only on non-zero exits

[[agent.triggers]]
event = "webhook"
path = "/hooks/deploy"          # agent listens for POST to this path
command = "env:DEPLOY_COMMAND"  # command from env var
```

Triggered commands are subject to the same policy, audit, and resource limits as manually executed commands.

---

## Result Parsing

Commands can return structured results that are parsed and queryable:

```bash
# Parse JSON output
rf exec --token <TOKEN> --output-format json "cat /etc/myapp/status.json"

# Parse as key=value
rf exec --token <TOKEN> --output-format kv "env | grep APP_"

# Parse CSV output
rf exec --token <TOKEN> --output-format csv "df -h | tail -n+2"
```

Parsed results can be used in playbook conditionals:

```yaml
steps:
  - name: Check disk space
    command: "df /var --output=pcent | tail -1"
    parse: "int"
    assert:
      less_than: 80
    on_failure: skip_remaining   # or: abort | rollback | continue
```

---

## See Also

- [Desired-State Convergence](desired-state.md) — Declarative state management
- [Remote Execution](execution.md) — Single-agent execution modes
- [CLI Reference: rf playbook](../reference/cli.md#rf-playbook) — Playbook command reference
- [Audit Log Format](../reference/audit-log-format.md) — Playbook audit entries
- [Use Cases: MSP Multi-Tenant](../use-cases/msp-multitenant.md) — Fleet operations at scale
