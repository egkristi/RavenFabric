# Kubernetes Operator

RavenFabric ships Kubernetes custom resource definitions (CRDs) so the fabric's
desired state — agents, policies, relays, and mesh topologies — can be declared
and managed natively in Kubernetes.

The CRD schemas mirror the type definitions in `rf-rpc` (`standard_crds()`),
which define four resources under the `ravenfabric.io` API group.

## CRDs

| Kind | Plural | Scope | Purpose |
|------|--------|-------|---------|
| `RavenAgent` | `ravenagents` | Namespaced | An enrolled agent and its relay/policy/labels |
| `RavenPolicy` | `ravenpolicies` | Namespaced | A deny-by-default policy applied to agents |
| `RavenRelay` | `ravenrelays` | Cluster | A stateless encrypted relay broker |
| `RavenMesh` | `ravenmeshes` | Cluster | A mesh network topology |

The manifests live in `deploy/helm/ravenfabric/crds/` and are installed
automatically by the Helm chart (see `crds.enabled` in `values.yaml`).

## Install

```bash
# Via Helm (installs CRDs from crds/ on `helm install`)
helm install ravenfabric deploy/helm/ravenfabric

# Or apply the CRD manifests directly
kubectl apply -f deploy/helm/ravenfabric/crds/
```

## Example: Declare an Agent

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: RavenAgent
metadata:
  name: web-01
  namespace: ravenfabric
spec:
  id: web-01
  relayUrl: ws://relay-svc:9090
  region: eu-west
  labels:
    role: web
    env: prod
```

```bash
kubectl get ravenagent -n ravenfabric
# NAME     AGENT ID   REGION    AGE
# web-01   web-01     eu-west   10s
```

## Example: Declare a Policy

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: RavenPolicy
metadata:
  name: prod-readonly
  namespace: ravenfabric
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
    deny:
      - pattern: ".*rm.*-rf.*"
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

## Validation

CRDs include OpenAPI v3 schemas. Required fields are enforced by the API
server — for example, a `RavenAgent` without `spec.id` is rejected:

```text
The RavenAgent "bad-agent" is invalid: spec.id: Required value
```

## Reconciler

The `Reconciler` in `rf-rpc` compares desired vs. observed state and produces
`Create`/`Update`/`Delete`/`Skip` actions. A full operator controller loop
(watch → reconcile → apply) is a follow-up enhancement; today the CRDs provide
declarative, schema-validated state plus the reconciliation planning logic.

## See Also

- [Controller](controller.md) — the `rf-controller` management plane binary
- [Multi-Cluster Kubernetes](../use-cases/multi-cluster-kubernetes.md) — operator workflows across clusters
- [Configuration File](../reference/config.md) — `raven.toml` reference
