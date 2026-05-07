# Multi-Cluster Kubernetes Operations

> **Scenario:** A platform team operates Kubernetes clusters across cloud
> providers, regions, and topologies (EKS, AKS, GKE, on-prem, edge K3s).
> Operators need consistent access, policy, and audit across all — without
> exposing each cluster's API server or managing per-cluster VPN/bastion setups.

---

## The Problem

Each Kubernetes cluster requires its own access plumbing:

- Different `kubeconfig` files, VPN connections, or bastion hosts per cluster
- Different identity providers and RBAC configurations
- Different audit log destinations
- No unified view across clusters
- Operational complexity scales linearly with cluster count
- New cluster onboarding takes days of setup

Traditional approaches:
- **Multiple kubeconfigs** — doesn't scale past ~10 clusters
- **Rancher/Tanzu/Anthos** — vendor lock-in, costly, struggle with edge
- **Argo CD/Flux** — declarative state only, no ad-hoc troubleshooting
- **Service meshes** — service-to-service, not operator workflows

---

## How RavenFabric Addresses This

```
Operator workstation
    │  rf exec --selector "env=prod" "kubectl get nodes"
    │  rf shell prod-eu-agent
    │  rf playbook apply rolling-deploy.yaml
    ▼
rf-relay (E2E encrypted, outbound-only)
    ▼
Per-cluster rf-agent (Deployment with ServiceAccount)
    ├─ Validates operator identity (Noise XX)
    ├─ Checks policy locally (deny-by-default)
    ├─ Executes kubectl with in-cluster credentials
    └─ Logs to structured audit trail
    ▼
Kubernetes API (cluster-internal, never exposed externally)
```

| Capability | How |
|------------|-----|
| Single operator identity | One Curve25519 keypair, all clusters recognize it |
| No exposed API servers | Agent connects outbound only; K8s API stays internal |
| Cross-cluster commands | `rf exec --selector` targets by labels (env, region, provider) |
| Coordinated rollouts | `rf playbook` with rolling/canary strategy across clusters |
| Unified audit | All operations, all clusters, one structured log |
| No kubeconfig juggling | Agent has in-cluster credentials; operator never needs them |
| Works everywhere | EKS, AKS, GKE, on-prem, K3s edge — all identical from operator's perspective |
| Approval workflows | Human-in-loop for destructive cross-cluster operations |

---

## Deployment

### Agent per cluster (Deployment)

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rf-agent
  namespace: ravenfabric-system
spec:
  replicas: 1
  selector:
    matchLabels:
      app: rf-agent
  template:
    metadata:
      labels:
        app: rf-agent
    spec:
      serviceAccountName: rf-agent
      containers:
        - name: agent
          image: ghcr.io/ravenfabric/rf-agent:latest
          volumeMounts:
            - name: config
              mountPath: /etc/ravenfabric
              readOnly: true
          resources:
            requests:
              cpu: 100m
              memory: 64Mi
            limits:
              cpu: 500m
              memory: 256Mi
      volumes:
        - name: config
          secret:
            secretName: rf-agent-config
            defaultMode: 0400
```

Agent configuration (`raven.toml`):

```toml
[agent]
id = "prod-eu"
relay = "wss://relay.example.com"
key_path = "/etc/ravenfabric/identity.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"
```

The ServiceAccount gets appropriate RBAC (e.g., `cluster-admin` for full ops, or namespace-scoped `admin` for multi-tenant clusters).

---

## Policy Configuration

```yaml
spec:
  commands:
    allow:
      # Read-only inspection
      - pattern: "^kubectl get .*$"
      - pattern: "^kubectl describe .*$"
      - pattern: "^kubectl logs .*$"
      - pattern: "^kubectl top .*$"
      # Operational
      - pattern: "^kubectl rollout restart deployment.*$"
      - pattern: "^kubectl rollout status deployment.*$"
      - pattern: "^kubectl scale deployment .* --replicas=[0-9]+$"

    deny:
      # Never allow destructive operations
      - pattern: "kubectl delete namespace"
      - pattern: "kubectl delete crd"
      - pattern: "kubectl delete pv"
      - pattern: "kubectl drain"

  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

---

## Example Workflows

### Cross-cluster status

```bash
$ rf exec --selector "env=prod" "kubectl get nodes -o wide"

[3 targets matching env=prod]
═══ prod-eu ═══
aks-pool1-001   Ready   42d   v1.30.4
aks-pool1-002   Ready   42d   v1.30.4

═══ prod-us ═══
ip-10-0-1-15    Ready   38d   v1.30.5
ip-10-0-1-22    Ready   38d   v1.30.5

═══ prod-asia ═══
gke-pool-1-abc  Ready   47d   v1.30.3
  3 clusters · 6 nodes · 1.2s
```

### Coordinated rolling deployment

```bash
$ rf playbook apply rolling-deploy.yaml
# Playbook targets: selector env=prod
# Strategy: rolling, 1 cluster at a time, 5min pause between
# Steps: pre-check → deploy → wait-rollout → health-check → smoke-test

[prod-eu: all steps passed (4m 12s)]
[pause: 5 min observation...]
[prod-us: all steps passed (3m 47s)]
[pause: 5 min observation...]
[prod-asia: all steps passed (4m 33s)]
✓ 3/3 clusters successful · 19m 14s
```

### Edge fleet upgrade check

```bash
$ rf exec --selector "type=edge" "k3s --version"

[247 targets: 242 reachable, 5 offline]
k3s versions:
  v1.30.5+k3s1: 198 clusters (80%)
  v1.30.4+k3s1: 41 clusters (17%)
  v1.30.3+k3s1: 3 clusters (1%)
  49 clusters need upgrade
```

---

## Comparison with Alternatives

| Feature | Multiple kubeconfigs | Rancher | Argo CD | Anthos/Tanzu | RavenFabric |
|---------|---------------------|---------|---------|--------------|-------------|
| Single operator identity | No | Yes | Partial | Yes | Yes |
| No exposed K8s API | No | Partial | Partial | Partial | Yes |
| Unified audit | No | Partial | Yes | Yes | Yes |
| Cross-provider | Yes | Yes | Yes | Partial | Yes |
| Edge cluster support | Yes | Yes | Limited | No | Yes |
| Ad-hoc command execution | Yes | Limited | No (GitOps) | Partial | Yes |
| Outbound-only agent | No | No | Yes (pull) | No | Yes |
| Vendor dependency | None | SUSE | None | Google/VMware | None (AGPL-3.0) |
| Cost | Free | $$$$ | Free | $$$$$ | Free |

---

## Implementation Status

| Feature | Status | Notes |
|---------|--------|-------|
| Agent in-cluster execution | Done | Deployment + ServiceAccount pattern |
| Cryptographic operator identity | Done | Curve25519 via Noise XX |
| Policy-validated kubectl commands | Done | Deny-by-default |
| Structured audit logging | Done | JSON-lines |
| Outbound-only agent connection | Done | Relay pattern |
| Label selector targeting | Done | `matches_labels()` for grain-based selection |
| Multi-agent playbooks | Done | `Orchestrator` + rolling/canary strategy |
| Approval workflows | Done | Human-in-loop for destructive ops |
| MagicDNS | Done | UDP DNS server for agent naming |
| AgentRegistry + controller | Done | Heartbeat, stale detection, label selection |
| Interactive shell | Done | `rf shell` with bidirectional I/O |
| Federated relay mesh | Planned | Geo-distributed relay interconnection |
| ServiceAccount ↔ operator mapping | Planned | Map RF identity to K8s RBAC |

---

## See Also

- [Edge & IoT Fleet Management](edge-iot-fleet.md) — Related patterns for large device fleets
- [CloudNativePG](cloudnativepg.md) — Database admin access in Kubernetes
- [MSP Multi-Tenant](msp-multitenant.md) — Multi-client isolation
- [Air-Gapped ICS](airgapped-ics.md) — Clusters in restricted networks
