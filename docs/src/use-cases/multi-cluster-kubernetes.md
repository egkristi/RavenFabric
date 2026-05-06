# Multi-Cluster Kubernetes

Unified operations across multiple Kubernetes clusters — different clouds, regions, or air-gapped environments — through a single control plane without cross-cluster network connectivity.

## The Problem

Organizations run Kubernetes in multiple clouds (AWS, Azure, GCP), on-prem, and at the edge. Each cluster has its own `kubectl` context, VPN, and access controls. Operators juggle dozens of kubeconfigs, and cross-cluster automation requires complex service mesh or VPN infrastructure.

## How RavenFabric Solves It

```
Platform Team (rf-cli)
    │
    ▼
┌─────────────────────────────────┐
│ Relay (central or per-region)   │
└───────┬──────────┬──────────┬───┘
        │          │          │
        ▼          ▼          ▼
   ┌────────┐ ┌────────┐ ┌────────┐
   │Agent   │ │Agent   │ │Agent   │
   │AWS-EKS │ │AZ-AKS  │ │On-Prem │
   │us-east │ │eu-west │ │dc-1    │
   └────────┘ └────────┘ └────────┘
```

- **One agent per cluster** — deployed as a Deployment with `kubectl` access
- **No cross-cluster networking** — each agent connects outbound to relay
- **Consistent policy** — same RBAC rules across all clusters
- **Unified audit** — who did what, on which cluster, when

## Example: Cross-Cluster Operations

```bash
# Check deployments across all clusters
rf exec aws-prod "kubectl get deployments -n app"
rf exec azure-staging "kubectl get deployments -n app"
rf exec onprem-dc1 "kubectl get deployments -n app"

# Rolling restart on production (with playbook)
rf playbook rolling-restart.yaml --target 'env=production'

# Emergency: scale down everywhere
rf exec --target 'role=k8s' "kubectl scale deployment/api --replicas=0 -n app"
```

## Policy: Cluster-Tier Separation

```yaml
spec:
  commands:
    allow:
      # Read-only for all operators
      - pattern: "^kubectl get .*"
      - pattern: "^kubectl describe .*"
      - pattern: "^kubectl logs .*"
      - pattern: "^kubectl top .*"

      # Mutations only for senior operators
      - pattern: "^kubectl rollout restart .*"
        requires_role: senior-operator
      - pattern: "^kubectl scale .*"
        requires_role: senior-operator

    deny:
      - pattern: ".*kubectl delete namespace.*"
      - pattern: ".*kubectl delete pv.*"
      - pattern: ".*--force --grace-period=0.*"
      - pattern: ".*exec.*sh.*"  # No interactive shells into pods

  resources:
    timeoutSeconds: 120
    maxOutputBytes: 10485760
```

## Deployment Pattern

Each cluster gets a RavenFabric agent with access to the cluster API:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ravenfabric-agent
  namespace: ravenfabric-system
spec:
  replicas: 1
  template:
    spec:
      serviceAccountName: ravenfabric-ops
      containers:
        - name: rf-agent
          image: ghcr.io/egkristi/ravenfabric-agent:latest
          args:
            - --relay=wss://relay.platform.example.com/meet
            - --id=aws-prod-us-east-1
            - --policy=/etc/ravenfabric/policy.yaml
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: ravenfabric-ops
subjects:
  - kind: ServiceAccount
    name: ravenfabric-ops
    namespace: ravenfabric-system
roleRef:
  kind: ClusterRole
  name: admin  # Or a custom, scoped role
  apiGroup: rbac.authorization.k8s.io
```

## Multi-Cluster Playbook

```yaml
# rolling-restart.yaml
name: Rolling Restart API Service
targets:
  - label: "env=production"
steps:
  - name: Check current status
    command: "kubectl rollout status deployment/api -n app"
    timeout: 30s

  - name: Restart deployment
    command: "kubectl rollout restart deployment/api -n app"

  - name: Wait for rollout
    command: "kubectl rollout status deployment/api -n app --timeout=300s"
    timeout: 360s

  - name: Verify health
    command: "kubectl get pods -n app -l app=api --field-selector=status.phase=Running"

rollback:
  command: "kubectl rollout undo deployment/api -n app"
```

## Why Not Alternatives?

| Approach | Problem |
|----------|---------|
| Multi-cluster service mesh | Complex, requires network connectivity |
| Cloud-specific tools (EKS Connector, Arc) | Vendor lock-in, inconsistent |
| VPN + shared kubeconfig | Security risk, no audit, over-provisioned |
| Rancher / Lens | UI-focused, limited automation |
| **RavenFabric** | Works everywhere, no network dependency, policy-driven, audited |
