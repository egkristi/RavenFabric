# Glossary

Technical terms used throughout RavenFabric documentation.

## Protocols & Standards

| Term | Definition |
|------|-----------|
| **Noise XX** | A [Noise Protocol Framework](http://noiseprotocol.org/) pattern providing mutual authentication. Both parties exchange static keys during the handshake. Used by WireGuard. |
| **QUIC** | A transport protocol (RFC 9000) providing multiplexed, encrypted streams over UDP. Lower latency than TCP+TLS. |
| **DTN** | Delay-Tolerant Networking. A store-carry-forward architecture for environments with intermittent connectivity (satellites, air-gapped transfers, IoT). |
| **yamux** | Yet Another Multiplexer. A stream multiplexing protocol that allows multiple logical streams over a single connection. |
| **msgpack** | MessagePack. A binary serialization format — like JSON but smaller and faster. Used for all RPC encoding. |
| **E2E** | End-to-end encryption. Only the communicating parties can decrypt the payload; intermediaries (relays) see only ciphertext. |

## Security

| Term | Definition |
|------|-----------|
| **RBAC** | Role-Based Access Control. Permissions are assigned to roles, roles are assigned to identities. |
| **OTP** | One-Time Password. A token valid for a single use during agent enrollment. Hash-stored, TTL-enforced. |
| **PQ hybrid KEM** | Post-Quantum hybrid Key Encapsulation Mechanism. Combines classical (X25519) and quantum-resistant (ML-KEM) key exchange for future-proof security. |
| **Zero trust** | A security model where no connection is trusted by default. Every request is authenticated and authorized regardless of network position. |
| **Deny-by-default** | Policy stance where all actions are rejected unless explicitly allowed by a matching rule. |

## Data Structures

| Term | Definition |
|------|-----------|
| **CRDT** | Conflict-free Replicated Data Type. A data structure that can be independently updated on multiple nodes and merged without conflicts. Used for policy convergence in mesh deployments. |
| **TrustStore** | RavenFabric's identity registry. Maps agent public keys to names and tracks enrollment state. |
| **SecureChannel** | The encrypted byte-stream layer built on top of a completed Noise XX handshake. Provides framing, encryption, and MAC verification. |

## Infrastructure

| Term | Definition |
|------|-----------|
| **Relay** | A stateless broker that pairs agents with clients. Forwards opaque encrypted bytes without decryption capability. |
| **NAT traversal** | Techniques (STUN, relay fallback) that allow connections between peers behind network address translation. |
| **musl** | A lightweight C standard library used for fully static Linux binary compilation (no runtime dependencies). |
| **LTO** | Link-Time Optimization. A compiler optimization that produces smaller, faster binaries by optimizing across crate boundaries. |

## File Formats

| Term | Definition |
|------|-----------|
| **raven.toml** | The agent/CLI configuration file. Specifies relay address, key paths, transport settings. |
| **policy.yaml** | The policy specification file. Defines allowed/denied commands, filesystem paths, resource limits, and RBAC roles. |
| **audit.jsonl** | The append-only audit log. One JSON object per line, one entry per action. |
