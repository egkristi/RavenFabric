# Desired-State Convergence Demo

Demonstrates RavenFabric's declarative desired-state engine — define what your
infrastructure should look like, and the agent continuously reconciles actual
state against the declaration.

## Architecture

```text
┌──────────────────────────────────────────────────────┐
│            Desired-State Convergence Flow             │
├──────────────────────────────────────────────────────┤
│                                                       │
│  ┌─────────────┐    ┌────────────┐    ┌───────────┐  │
│  │  YAML Spec  │───▶│ Convergence│───▶│  System   │  │
│  │  (declare)  │    │   Engine   │    │   Probe   │  │
│  └─────────────┘    └─────┬──────┘    └─────┬─────┘  │
│                           │                  │        │
│                    ┌──────▼──────┐    ┌──────▼──────┐ │
│                    │    Drift    │    │   Actual    │ │
│                    │   Report    │    │    State    │ │
│                    └──────┬──────┘    └─────────────┘ │
│                           │                           │
│                    ┌──────▼──────┐                     │
│                    │ Remediator  │  (if mode=remediate)│
│                    │  (fix it)   │                     │
│                    └─────────────┘                     │
└──────────────────────────────────────────────────────┘
```

## Concepts

| Concept | Description |
|---------|-------------|
| **DesiredStateSpec** | YAML document declaring target state (packages, files, services, sysctl) |
| **SystemProbe** | Trait that queries actual system state (is package X installed? what's in file Y?) |
| **Remediator** | Trait that fixes drifted resources (install package, write file, start service) |
| **ConvergenceEngine** | Core engine: loads spec, checks via probe, optionally remediates |
| **DriftItem** | Per-resource check result: Converged, Drifted, Remediated, or Failed |
| **ConvergenceReport** | Full report with all drift items, timestamp, and mode |
| **Grains** | Auto-collected system facts (OS, arch, hostname) used for target selection |
| **EventTrigger** | Trigger types (Timer, Cron, FileWatch, Webhook) that can invoke convergence |

## Convergence Modes

| Mode | Behavior |
|------|----------|
| `report` | Detect drift and report it — no changes made (monitoring/audit) |
| `remediate` | Detect drift and automatically fix it (self-healing) |

## Resource Types

| Resource | Attributes | Example |
|----------|-----------|---------|
| **Package** | name, state (installed/absent), version constraint | `nginx >= 1.24.0` |
| **File** | path, content, mode, owner, group, state | `/etc/nginx/nginx.conf` with `0644` |
| **Service** | name, state (running/stopped), enabled | `nginx` running and enabled |
| **Sysctl** | key, value | `net.ipv4.ip_forward = 0` |

## Example Spec

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: web-server-baseline
spec:
  targets:
    selector:
      labels:
        role: web-server
  state:
    packages:
      - name: nginx
        state: installed
        version: ">=1.24.0"
      - name: telnet
        state: absent
    files:
      - path: /etc/nginx/nginx.conf
        content: "worker_processes auto;"
        mode: "0644"
        owner: root
    services:
      - name: nginx
        state: running
        enabled: true
    sysctl:
      - key: net.ipv4.ip_forward
        value: "0"
  convergence:
    mode: remediate
    intervalSeconds: 300
```

## Scenarios

| # | Script | What It Demonstrates |
|---|--------|---------------------|
| 01 | `01-drift-detection.sh` | Detect drift across packages, files, services, and sysctl |
| 02 | `02-remediation.sh` | Auto-remediate drifted resources back to desired state |
| 03 | `03-report-mode.sh` | Report-only mode: detect drift without changing anything |
| 04 | `04-grains-targeting.sh` | Grains-based target selection: match agents by OS/arch labels |
| 05 | `05-event-triggers.sh` | Event triggers that invoke convergence (Timer, Webhook, Cron) |
| 06 | `06-version-constraints.sh` | Package version constraint matching (>=, >, <, exact) |
| 07 | `07-all-scenarios.sh` | Run all scenarios sequentially |

## Running

```bash
# Run all scenarios
./scenarios/07-all-scenarios.sh

# Run individual scenarios
./scenarios/01-drift-detection.sh
./scenarios/02-remediation.sh
```

All scenarios use `cargo test` to execute the integration tests — no real
system changes are made. The tests use mock `SystemProbe` and `Remediator`
implementations to simulate real infrastructure.
