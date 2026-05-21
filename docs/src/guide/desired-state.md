# Desired-State Convergence

RavenFabric can maintain declarative desired state on remote agents. You define what the system should look like; the executor continuously checks and corrects drift. This replaces Ansible playbooks, Salt states, and Puppet manifests.

## How It Works

1. You create a `DesiredState` YAML manifest
2. The agent evaluates current state against the manifest
3. Any drift triggers remediation actions
4. Results are logged to the audit trail
5. Drift events can trigger alerts

The agent re-evaluates at a configurable interval (`intervalSeconds`) and also on demand via `rf apply`.

---

## Manifest Format

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: web-baseline
spec:
  targets:
    selector:
      labels:
        role: web

  state:
    packages:
      - name: nginx
        state: installed          # installed | absent | at-version
        version: "1.24.*"         # optional — only for at-version
      - name: telnet
        state: absent
      - name: curl
        state: installed

    services:
      - name: nginx
        state: running            # running | stopped | disabled
        enabled: true             # start on boot
      - name: apache2
        state: stopped
        enabled: false

    files:
      - path: /etc/nginx/nginx.conf
        content: |
          worker_processes auto;
          events { worker_connections 1024; }
        mode: "0644"
        owner: root
        group: root
      - path: /opt/app/config.yaml
        source: ./config.yaml     # local file to upload
        mode: "0640"
        owner: app
        group: app

    sysctl:
      - key: net.ipv4.tcp_syncookies
        value: "1"
      - key: vm.swappiness
        value: "10"

  convergence:
    mode: remediate               # report | remediate | enforce
    intervalSeconds: 300
    onDrift:
      alert: true
      webhookUrl: "https://hooks.example.com/drift-alert"
```

---

## Convergence Modes

| Mode | Behaviour |
|------|-----------|
| `report` | Detect drift and log it; take no action |
| `remediate` | Detect drift and fix it; log what changed |
| `enforce` | Prevent drift in real-time; block changes that would violate the manifest |

Start with `report` to understand the current state before enabling remediation.

---

## Applying State On Demand

Push a desired-state manifest immediately:

```bash
rf apply --token <TOKEN> desired-state.yaml
```

Output:
```
Evaluating desired-state.yaml on web-01...

  packages:
    ✓  nginx          installed (1.24.0)
    ✗  telnet         present — removing
    ✓  curl           installed

  services:
    ✓  nginx          running, enabled
    →  apache2        stopping, disabling

  files:
    →  /etc/nginx/nginx.conf  content changed — updating
    ✓  /opt/app/config.yaml  matches

  sysctl:
    ✓  net.ipv4.tcp_syncookies  1
    →  vm.swappiness            60 → 10 — updating

Remediation complete. 4 changes applied. Audit entries: 4.
```

Use `--dry-run` to preview changes without applying them:

```bash
rf apply --token <TOKEN> --dry-run desired-state.yaml
```

---

## Package Management

```yaml
packages:
  - name: nginx
    state: installed

  # Pin to a specific version range
  - name: postgresql-client-16
    state: at-version
    version: "16.2.*"

  # Ensure a package is absent
  - name: netcat-traditional
    state: absent
```

Supported package backends:
- `apt` (Debian, Ubuntu)
- `yum` / `dnf` (RHEL, CentOS, Fedora, Amazon Linux)
- `apk` (Alpine)
- `brew` (macOS)
- Custom — define a `packageManager` override

---

## File Management

```yaml
files:
  # Inline content
  - path: /etc/myapp/config.toml
    content: |
      [server]
      port = 8080
    mode: "0644"
    owner: myapp
    group: myapp

  # From a local file (uploaded via the encrypted channel)
  - path: /etc/myapp/tls.crt
    source: ./certs/server.crt
    mode: "0644"

  # Template with agent grain substitution
  - path: /etc/hostname
    content: "{{ grain.agent_id }}"
    mode: "0644"
```

File content is SHA-256 hashed and compared before upload. No transfer occurs if the content already matches.

---

## Service Management

```yaml
services:
  - name: nginx
    state: running
    enabled: true

  # Restart after config change
  - name: nginx
    state: running
    enabled: true
    restart_on_change:
      - /etc/nginx/nginx.conf
      - /etc/nginx/conf.d/
```

`restart_on_change` triggers a service restart if any of the listed files were modified during the same convergence cycle.

---

## Sysctl Settings

```yaml
sysctl:
  - key: net.core.somaxconn
    value: "65535"
  - key: net.ipv4.ip_forward
    value: "1"
  - key: vm.overcommit_memory
    value: "1"
```

Settings are applied via `sysctl -w` and persisted to `/etc/sysctl.d/ravenfabric.conf`.

---

## Drift Detection and Alerting

When drift is detected, the agent can:
1. Log a structured drift event to the audit trail
2. Send an alert to a webhook
3. Expose the drift status via the health endpoint

```yaml
convergence:
  mode: report
  onDrift:
    alert: true
    webhookUrl: "https://alertmanager.internal/webhook"
    webhookFormat: slack         # slack | pagerduty | generic
```

Drift events in the audit log:

```json
{
  "seq": 4201,
  "ts": "2026-05-21T12:00:00Z",
  "event": "drift_detected",
  "agent": "web-01",
  "manifest": "web-baseline",
  "drifted": [
    { "type": "package", "name": "telnet", "actual": "installed", "expected": "absent" },
    { "type": "sysctl", "key": "vm.swappiness", "actual": "60", "expected": "10" }
  ]
}
```

---

## Grains

Grains are automatically collected facts about the agent's system, available for use in templates and targeting:

| Grain | Example Value |
|-------|---------------|
| `grain.agent_id` | `web-01` |
| `grain.os` | `ubuntu` |
| `grain.os_version` | `22.04` |
| `grain.arch` | `x86_64` |
| `grain.hostname` | `web-01.internal` |
| `grain.ip_addresses` | `["10.0.1.5", "192.168.1.5"]` |
| `grain.cpu_count` | `8` |
| `grain.mem_total_mb` | `16384` |

Query grains from the CLI:

```bash
rf exec --token <TOKEN> --grains
```

---

## See Also

- [Fleet Orchestration](fleet-orchestration.md) — Multi-agent playbooks and rolling deployments
- [Policy Configuration](policy-config.md) — Command and path policies
- [Audit Log Format](../reference/audit-log-format.md) — Drift and convergence log entries
- [Use Cases: Edge & IoT Fleet](../use-cases/edge-iot-fleet.md) — Desired state on constrained devices
