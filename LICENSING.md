# RavenFabric — Licensing

RavenFabric uses a **dual-license model**: open source for the community,
commercial for enterprise use.

---

## Open Source Core — AGPLv3

The RavenFabric core is licensed under the
[GNU Affero General Public License v3.0](LICENSES/AGPLv3.txt) (AGPLv3).

This covers:
- Transport layer (all drivers: WebSocket, QUIC, WireGuard, Reticulum, ...)
- Noise XX end-to-end encryption
- RPC executor and RPCPolicy engine
- ConnectivityPolicy and negotiator
- Metrics collection and outputs
- Bootstrap and relay

**AGPLv3 in plain English:**
- Free to use, modify, and distribute
- If you modify and run it as a service (SaaS), you must publish your modifications
- If you distribute it as part of a product, that product must also be AGPLv3
- Protects against cloud providers silently forking and offering as managed service

---

## Commercial License — Enterprise

A commercial license is required if you:

1. Use RavenFabric commercially with **>50 managed agents** and **>$5M revenue/year**
2. Distribute RavenFabric inside a product **without** releasing your source under AGPLv3
3. Offer RavenFabric as a **hosted/managed service** without releasing modifications

A commercial license grants:
- Usage rights without AGPLv3 obligations
- Access to enterprise-only features (see below)
- Priority support and SLA options

See [LICENSES/COMMERCIAL.txt](LICENSES/COMMERCIAL.txt) for full terms.

---

## What Is Free vs. Commercial

| Feature | AGPLv3 (Free) | Commercial |
|---------|:---:|:---:|
| All transport drivers (WG, QUIC, WSS, Reticulum, ...) | ✅ | ✅ |
| Noise XX E2E encryption | ✅ | ✅ |
| Fire-and-forget, fire-and-verify | ✅ | ✅ |
| Task execution (ordered steps) | ✅ | ✅ |
| RPCPolicy (command/path/service rules) | ✅ | ✅ |
| ConnectivityPolicy (transport cascade) | ✅ | ✅ |
| Metrics collection + InfluxDB/Prometheus output | ✅ | ✅ |
| Bootstrap + relay | ✅ | ✅ |
| Up to 50 agents, up to $5M revenue | ✅ | ✅ |
| **Playbooks (multi-agent, rolling, rollback)** | — | ✅ |
| **Desired state + drift detection** | — | ✅ |
| **SecurityPolicy (immutable rules, blast radius)** | — | ✅ |
| **RBAC + multi-tenant isolation** | — | ✅ |
| **SSO / SAML integration** | — | ✅ |
| **Compliance audit reporting** | — | ✅ |
| **Air-gap / offline license** | — | ✅ |
| **Priority support + SLA** | — | ✅ |

---

## Why AGPLv3?

We chose AGPLv3 over MIT/Apache 2.0 deliberately:

**The SaaS loophole:** MIT and Apache 2.0 allow cloud providers to offer RavenFabric
as a managed service, fork it, add proprietary features, and never contribute back.
This happened to MongoDB (AWS DocumentDB), Elasticsearch (AWS OpenSearch), and
Redis. AGPLv3 closes this loophole — if you run it as a service, your modifications
must be open.

**We commit to the core staying open:** The transport, crypto, RPC, and policy
layers will remain AGPLv3 forever. We will not retroactively relicense code already
released under AGPLv3. Enterprise features that we build on top may be commercial,
but the foundation will not be.

---

## Contributor License Agreement (CLA)

Contributors to RavenFabric must sign a Contributor License Agreement (CLA).
This allows us to offer the commercial license while accepting community contributions.

The CLA grants us the right to:
- Include your contribution in the AGPLv3 release
- Include your contribution in commercial releases

It does NOT transfer copyright ownership. You retain copyright over your contributions.
