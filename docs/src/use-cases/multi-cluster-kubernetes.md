# Multi-Cluster Kubernetes Operations

> **Scenario:** A platform team operates many Kubernetes clusters across
> different cloud providers, regions, and network topologies. Some are public
> cloud (AWS EKS, Azure AKS, GCP GKE), some are on-premises (vanilla
> Kubernetes, Rancher, OpenShift), some are at the edge (K3s, MicroK8s).
> Operators need consistent access, policy, and observability across all of
> them — without configuring each cluster's network ingress separately.

---

## The Problem

Multi-cluster Kubernetes has become the norm rather than the exception:

- **Disaster recovery** requires geographic distribution
- **Regulatory boundaries** demand regional clusters (data residency)
- **Cost optimization** drives workload placement decisions
- **Vendor diversification** reduces single-vendor lock-in
- **Edge use cases** push compute closer to users

But each cluster brings its own access model:

- Different `kubeconfig` files per cluster
- Different ingress controllers and gateway configurations
- Different identity providers (cluster-bound RBAC)
- Different VPN/bastion arrangements per environment
- Different audit logging destinations
- Different network policies and firewall rules

The result is **operational sprawl**: every new cluster requires its own
access plumbing, and operators juggle dozens of credentials and connection
strings.

### Traditional approaches and their problems

```
Operator workstation
    │
    ├─→ kubeconfig-prod-eu.yaml      → AKS API in Western Europe
    ├─→ kubeconfig-prod-us.yaml      → EKS API in us-east-1
    ├─→ kubeconfig-prod-asia.yaml    → GKE API in asia-southeast1
    ├─→ kubeconfig-staging.yaml      → On-prem RKE2
    ├─→ kubeconfig-edge-001.yaml     → K3s at retail location 001
    ├─→ kubeconfig-edge-002.yaml     → K3s at retail location 002
    │   ...
    └─→ kubeconfig-edge-247.yaml     → K3s at retail location 247

Each cluster requires:
- Its own VPN or bastion setup
- Its own identity federation
- Its own audit destination
- Its own ingress security
```

**Issues:**

- **Operational complexity scales linearly** with cluster count
- **Identity sprawl** — each cluster has its own RBAC subjects
- **No unified audit** — investigations require correlating logs from many sources
- **Network exposure** — every cluster needs its API server reachable somehow
- **Inconsistent security posture** — easy for one cluster to drift
- **Onboarding new clusters is expensive** — days of setup per cluster

---

## The RavenFabric Approach

```
Operator workstation
    │
    │  rf exec --selector "env=prod" "kubectl get nodes"
    │  rf exec --cluster "prod-eu" "kubectl rollout restart deployment/api"
    │  rf shell --cluster "edge-247"
    ▼
RavenFabric Relay (E2E encrypted)
    │
    ▼
Agents in every cluster (one per cluster, or one per node)
    │  ├─ Validates operator identity (Noise XX)
    │  ├─ Checks per-cluster policy
    │  ├─ Executes via in-cluster kubectl with cluster-bound credentials
    │  └─ Logs to unified audit trail
    ▼
Kubernetes API server (cluster-internal, never exposed)
```

### What this provides

| Capability | Description |
|------------|-------------|
| **Single operator identity** | One Curve25519 keypair, recognized by all clusters |
| **No exposed API servers** | Kubernetes API stays cluster-internal; agents connect outbound only |
| **Unified policy** | Same RPCPolicy structure across all clusters, with cluster-specific overrides |
| **Centralized audit** | All operations from all clusters land in one structured log |
| **Cross-cluster commands** | `rf exec --selector` runs the same command across many clusters |
| **No kubeconfig juggling** | Operators don't need cluster credentials at all — the agent has them, scoped |
| **Consistent across providers** | EKS, AKS, GKE, on-prem, edge — all look the same |
| **Outbound-only** | Works behind corporate proxies, NAT, air-gap-tolerant networks |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  Operator workstation                                               │
│                                                                     │
│  $ rf clusters list                                                 │
│  CLUSTER         REGION        TYPE       STATUS                    │
│  prod-eu         eu-west-1     AKS        connected                 │
│  prod-us         us-east-1     EKS        connected                 │
│  prod-asia       ap-southeast  GKE        connected                 │
│  staging         on-prem-1     RKE2       connected                 │
│  edge-001..247   distributed   K3s        242/247 connected         │
└────────┬────────────────────────────────────────────────────────────┘
         │ Noise XX
         ▼
┌─────────────────────────────────────────────────────────────────────┐
│  rf-relay (geo-distributed mesh)                                    │
│                                                                     │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐                      │
│  │ relay-eu │ ←→ │ relay-us │ ←→ │ relay-as │  Federated mesh      │
│  └──────────┘    └──────────┘    └──────────┘                      │
└────────┬────────────────┬─────────────────┬────────────────────────┘
         │                │                 │
         ▼                ▼                 ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ AKS Cluster  │  │ EKS Cluster  │  │ GKE Cluster  │
│ (Europe)     │  │ (US East)    │  │ (Asia)       │
│              │  │              │  │              │
│ rf-agent     │  │ rf-agent     │  │ rf-agent     │
│  (Deployment)│  │  (Deployment)│  │  (Deployment)│
│              │  │              │  │              │
│ ServiceAccount│ │ ServiceAccount│ │ ServiceAccount│
│ + RBAC       │  │ + RBAC       │  │ + RBAC       │
│              │  │              │  │              │
│ K8s API      │  │ K8s API      │  │ K8s API      │
│ (internal)   │  │ (internal)   │  │ (internal)   │
└──────────────┘  └──────────────┘  └──────────────┘
         │                │                 │
         ▼                ▼                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Hundreds of edge K3s clusters (retail, branches, vehicles)         │
│                                                                     │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐    ┌─────────┐                │
│  │edge-001 │ │edge-002 │ │edge-003 │... │edge-247 │                │
│  │ K3s+rf  │ │ K3s+rf  │ │ K3s+rf  │    │ K3s+rf  │                │
│  └─────────┘ └─────────┘ └─────────┘    └─────────┘                │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Deployment Patterns

### Pattern A: Single agent per cluster (Deployment)

The most straightforward pattern. One RavenFabric agent runs as a Deployment
in each cluster, with a ServiceAccount granting it the privileges it needs.

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: rf-agent
  namespace: ravenfabric-system
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: rf-agent-cluster-admin
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: cluster-admin       # or scoped role for less-privileged operators
subjects:
  - kind: ServiceAccount
    name: rf-agent
    namespace: ravenfabric-system
---
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
          image: ghcr.io/ravenfabric/rf-agent:0.1
          args:
            - --relay=wss://relay.ravenfabric.example.com
            - --identity=/etc/ravenfabric/identity.key
            - --policy=/etc/ravenfabric/policy.yaml
            - --cluster-name=prod-eu
            - --cluster-labels=env=prod,region=eu,provider=azure
          env:
            - name: KUBECONFIG
              value: /var/run/secrets/kubernetes.io/serviceaccount/kubeconfig
          volumeMounts:
            - name: identity
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
        - name: identity
          secret:
            secretName: rf-agent-identity
            defaultMode: 0400
```

### Pattern B: Per-namespace agents (multi-tenant)

For clusters serving multiple teams, each team gets a dedicated agent in
their namespace with namespace-scoped RBAC.

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: rf-agent-team-platform
  namespace: team-platform
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: admin       # namespace admin only
subjects:
  - kind: ServiceAccount
    name: rf-agent
    namespace: team-platform
```

This way, when an operator from the platform team uses `rf exec`, they only
get access to the `team-platform` namespace — even if their workstation has
identity that *could* reach broader scope in another namespace.

---

## Policy Configuration

### Cluster-level policy

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: RPCPolicy
metadata:
  name: prod-cluster-policy
spec:
  cluster:
    name: prod-eu
    labels:
      env: prod
      region: eu

  commands:
    allow:
      # Read-only inspection
      - pattern: "^kubectl get .*$"
      - pattern: "^kubectl describe .*$"
      - pattern: "^kubectl logs .*$"
      - pattern: "^kubectl top .*$"

      # Common operational tasks
      - pattern: "^kubectl rollout restart deployment.*$"
      - pattern: "^kubectl rollout status deployment.*$"
      - pattern: "^kubectl rollout undo deployment.*$"

      # Scaling
      - pattern: "^kubectl scale deployment .* --replicas=[0-9]+$"

      # Debugging
      - pattern: "^kubectl exec .* -- /bin/sh$"
      - pattern: "^kubectl port-forward .*$"

    deny:
      # Never allow destructive operations
      - pattern: "kubectl delete namespace"
      - pattern: "kubectl delete crd"
      - pattern: "kubectl delete pv"
      - pattern: "kubectl drain"
      - pattern: "kubectl cordon"

  # RBAC integration — even if pattern allows, K8s RBAC has final say
  identity:
    map_operator_to_serviceaccount: true
    operator_groups:
      - sre-team
      - on-call

  approval:
    required:
      - pattern: "kubectl delete pod.*--force"
        approvers: ["sre-lead"]
      - pattern: "kubectl rollout restart.*-n production"
        approvers: ["sre-team"]
        minApprovers: 1
```

### Operator-level policy (cross-cluster)

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: OperatorPolicy
metadata:
  name: senior-sre-policy
spec:
  operators:
    - alice
    - bob
    - carol

  # Which clusters this operator can access
  cluster_selector:
    matchLabels:
      env: prod          # only prod clusters
    matchExpressions:
      - key: region
        operator: In
        values: [eu, us]   # not Asia (different team)

  # Per-cluster permissions
  permissions:
    default: read_only
    overrides:
      - cluster: prod-eu
        permission: full_admin
        validity:
          notAfter: "2026-12-31T23:59:59Z"
      - cluster: prod-us
        permission: read_write
```

---

## Operator Workflows

### Workflow 1: Same command across all production clusters

```bash
$ rf exec --selector "env=prod" "kubectl get nodes -o wide"

[connecting to 3 clusters matching env=prod...]

═══ prod-eu (AKS, eu-west-1) ═══
NAME              STATUS   ROLES    AGE   VERSION
aks-pool1-001     Ready    <none>   42d   v1.30.4
aks-pool1-002     Ready    <none>   42d   v1.30.4
aks-pool2-001     Ready    <none>   42d   v1.30.4

═══ prod-us (EKS, us-east-1) ═══
NAME                        STATUS   ROLES    AGE   VERSION
ip-10-0-1-15.ec2.internal   Ready    <none>   38d   v1.30.5
ip-10-0-1-22.ec2.internal   Ready    <none>   38d   v1.30.5
ip-10-0-2-44.ec2.internal   Ready    <none>   38d   v1.30.5

═══ prod-asia (GKE, asia-southeast1) ═══
NAME                                      STATUS   ROLES    AGE   VERSION
gke-prod-asia-pool-1-abc123-def456        Ready    <none>   47d   v1.30.3
gke-prod-asia-pool-1-abc123-def457        Ready    <none>   47d   v1.30.3

  audited · 1.2s · 3 clusters · 8 nodes
```

### Workflow 2: Coordinated rolling deployment

```yaml
# rolling-deploy.yaml
apiVersion: ravenfabric.io/v1alpha1
kind: Playbook
metadata:
  name: api-service-rollout
spec:
  targets:
    selector:
      matchLabels:
        env: prod

  strategy:
    type: rolling
    parallel: 1                # one cluster at a time
    pause_between_seconds: 300 # 5 min observation between clusters
    onFailure: pause           # don't continue, alert humans

  steps:
    - name: pre-checks
      command: |
        kubectl get pods -n api-system | grep -v Running | wc -l
      expect_output: "0"
      onFailure: abort

    - name: deploy
      command: |
        kubectl set image deployment/api-service \
          api=registry.example.com/api:v2.5.0 \
          -n api-system

    - name: wait-for-rollout
      command: |
        kubectl rollout status deployment/api-service \
          -n api-system --timeout=10m

    - name: post-checks
      command: |
        kubectl exec -n api-system deployment/api-service \
          -- curl -sf http://localhost:8080/health
      expect_output: "OK"

    - name: smoke-test
      command: |
        /usr/local/bin/api-smoke-test \
          --endpoint https://api.${CLUSTER_REGION}.example.com
      timeout_seconds: 120
```

```bash
$ rf playbook apply rolling-deploy.yaml

[playbook: api-service-rollout]
[targets: 3 clusters matching env=prod]
[strategy: rolling, 1 at a time, 5min between]

═══ prod-eu ═══
[pre-checks: ok]
[deploy: ok]
[wait-for-rollout: ok (4m 12s)]
[post-checks: ok]
[smoke-test: ok]
[pause: 5 min observation...]
[no errors detected]

═══ prod-us ═══
[pre-checks: ok]
[deploy: ok]
[wait-for-rollout: ok (3m 47s)]
[post-checks: ok]
[smoke-test: ok]
[pause: 5 min observation...]
[no errors detected]

═══ prod-asia ═══
[pre-checks: ok]
[deploy: ok]
[wait-for-rollout: ok (4m 33s)]
[post-checks: ok]
[smoke-test: ok]

✓ Playbook completed successfully
  duration: 19m 14s
  clusters: 3/3 successful
  audit: rf-audit-2026-05-05-rollout.jsonl
```

### Workflow 3: Cross-cluster troubleshooting

```bash
$ rf exec --selector "env=prod" \
    "kubectl logs -l app=api --tail=20 --since=5m -n api-system"

# Output from all 3 clusters, automatically interleaved with cluster context

[prod-eu] api-service-7d4f-abc12: 14:22:01 INFO  request handled in 12ms
[prod-eu] api-service-7d4f-abc12: 14:22:01 INFO  request handled in 8ms
[prod-us] api-service-9f8e-xyz45: 14:22:01 ERROR connection timeout to db
[prod-us] api-service-9f8e-xyz45: 14:22:02 ERROR connection timeout to db
[prod-asia] api-service-3c1d-mno78: 14:22:01 INFO  request handled in 14ms
[prod-eu] api-service-7d4f-abc12: 14:22:02 INFO  request handled in 11ms

# Pattern detection: prod-us is having DB connectivity issues
$ rf exec --cluster prod-us "kubectl describe svc -n db postgres-rw"
```

### Workflow 4: Edge cluster fleet management

```bash
# Update K3s on all edge clusters
$ rf exec --selector "type=edge" "k3s --version" --parallel 50

[247 edge clusters in selector]
[connecting in batches of 50...]

[batch 1/5: 50 clusters, response in 2.3s]
[batch 2/5: 50 clusters, response in 2.1s]
[batch 3/5: 50 clusters, response in 2.4s]
[batch 4/5: 50 clusters, response in 2.2s]
[batch 5/5: 47 clusters, response in 2.0s]

[summary: 247/247 reachable]
[k3s versions found:]
  v1.30.5+k3s1: 198 clusters (80.2%)
  v1.30.4+k3s1: 41 clusters  (16.6%)
  v1.30.3+k3s1: 8 clusters   (3.2%)

[recommendation: 49 clusters need k3s upgrade]
```

---

## Comparison with Alternatives

| Feature | Multiple kubeconfigs | Rancher Multi-Cluster | Argo CD | Anthos / Tanzu | RavenFabric |
|---------|---------------------|----------------------|---------|----------------|-------------|
| **Single operator identity** | No | Yes | Partial | Yes | Yes |
| **No exposed K8s API** | No | Partial (Rancher proxy) | Partial (pull-based) | Partial | Yes |
| **Unified audit** | No | Partial (Rancher logs) | Yes | Yes | Yes |
| **Works across cloud providers** | Yes | Yes | Yes | Partial | Yes |
| **Edge cluster support** | Yes | Yes | Limited | No | Yes |
| **Air-gap support** | No | Partial | Partial | No | Yes |
| **Outbound-only agent** | No | No (inbound) | Yes (pull) | No | Yes |
| **No vendor dependency** | Yes | No (SUSE) | Yes (Foundation) | No (Google/VMware) | Yes (AGPLv3) |
| **Ad-hoc command execution** | Yes (kubectl) | Limited | No (GitOps only) | Partial | Yes |
| **Cost** | Free | $$$$ | Free | $$$$$ | Free |

---

## Implementation Status

### Available today (v0.1)

- Single agent in single cluster, executing commands
- Cryptographic operator identity (Curve25519)
- Policy-validated command execution
- Structured audit logging
- Outbound-only agent connection

### Coming in v0.2

- Multiple cluster support in CLI (`rf clusters list`)
- Cluster labeling and selectors
- Parallel execution across clusters

### Coming in v0.3

- Multi-agent playbooks with rolling/canary strategies
- Coordinated rollouts across clusters
- Cross-cluster log aggregation

### Coming in v0.4+

- Federated relay mesh (geo-distributed)
- MagicDNS for cluster naming
- ServiceAccount integration with cluster RBAC
- Approval workflows for cross-cluster operations

---

## Why This Matters

Multi-cluster Kubernetes is increasingly the norm, but the tools to manage it
have lagged. The most common patterns today:

1. **Service meshes (Istio, Linkerd) handle service-to-service traffic** but
   say nothing about operator workflows
2. **GitOps tools (Argo CD, Flux) manage declarative state** but don't help
   with debugging, troubleshooting, or ad-hoc operations
3. **Vendor multi-cluster platforms (Rancher, Tanzu, Anthos) offer unified
   management** but lock organizations into specific vendors and often
   struggle with non-cloud or edge environments
4. **Manual kubeconfig juggling** remains common but doesn't scale beyond
   a handful of clusters

RavenFabric proposes a model orthogonal to these:

> **The Kubernetes API stays cluster-internal. Operators don't need
> credentials per cluster. Policy is uniform. Audit is unified. The same
> fabric works for AKS, EKS, GKE, on-prem, and edge.**

This is particularly valuable for organizations whose Kubernetes footprint
spans:

- **Multiple cloud providers** for vendor diversification
- **Multiple geographic regions** for data residency or latency
- **Cloud and on-premises** in hybrid configurations
- **Production and edge** with hundreds or thousands of edge clusters

For these organizations, eliminating per-cluster access plumbing translates
directly into reduced operational risk, faster incident response, and lower
total cost of ownership.

---

## See Also

- [README.md](../README.md) — RavenFabric overview
- [CONNECTIVITY.md](../CONNECTIVITY.md) — Multi-transport architecture
- [usecase-cloudnativepg.md](usecase-cloudnativepg.md) — CloudNativePG admin access
- [usecase-edge-iot-fleet.md](usecase-edge-iot-fleet.md) — Edge & IoT fleet management
- [usecase-airgapped-ics.md](usecase-airgapped-ics.md) — Air-gapped industrial systems
- [usecase-msp-multitenant.md](usecase-msp-multitenant.md) — MSP multi-tenant operations
- [Argo CD](https://argoproj.github.io/cd/) — Pull-based GitOps reference
- [Cluster API](https://cluster-api.sigs.k8s.io/) — Kubernetes cluster
  lifecycle management
