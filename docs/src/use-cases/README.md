# Use Cases

RavenFabric is designed for environments where security, auditability, and network resilience are non-negotiable. These use cases demonstrate how the same fabric adapts to radically different deployment scenarios — from cloud-native Kubernetes to fully air-gapped industrial systems.

## Scenarios

| Use Case | Summary |
|----------|---------|
| [CloudNativePG Database Access](cloudnativepg.md) | Policy-controlled PostgreSQL access inside Kubernetes without exposing ports or creating VPN tunnels. |
| [Edge & IoT Fleet Management](edge-iot-fleet.md) | Manage thousands of distributed devices through a single encrypted control plane with bandwidth-aware policies. |
| [Multi-Cluster Kubernetes](multi-cluster-kubernetes.md) | Unified operations across clusters in different clouds, regions, or air-gapped environments. |
| [Air-Gapped Industrial Systems](airgapped-ics.md) | Remote maintenance of ICS/SCADA/OT networks using physical media transport and store-carry-forward delivery. |
| [MSP Multi-Tenant Operations](msp-multitenant.md) | Hundreds of client environments through one cryptographically isolated control plane with per-client policy. |

## Common Properties

Every use case shares the same foundational guarantees:

- **Deny-by-default policy** — only explicitly permitted operations execute
- **Mutual authentication** — Noise XX handshake on every connection, no certificates
- **Full audit trail** — structured JSON-lines log of every action, every agent
- **Transport agnostic** — WebSocket, QUIC, WireGuard, serial, or store-carry-forward
- **Single static binary** — no runtime dependencies, deploys anywhere

## Which Use Case Fits You?

| If you need... | Start with... |
|---------------|--------------|
| Database access in Kubernetes | [CloudNativePG](cloudnativepg.md) |
| Fleet management at scale | [Edge & IoT](edge-iot-fleet.md) |
| Cross-cloud operations | [Multi-Cluster Kubernetes](multi-cluster-kubernetes.md) |
| Offline/air-gapped environments | [Air-Gapped ICS](airgapped-ics.md) |
| Client isolation for service providers | [MSP Multi-Tenant](msp-multitenant.md) |
