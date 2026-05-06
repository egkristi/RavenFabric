# MSP Multi-Tenant Operations

Managed Service Providers operating hundreds of client environments through a single, cryptographically isolated control plane — with per-client policy, per-technician audit, and client-controlled access.

## The Problem

MSPs use 5-10 different remote access tools across their client base. Each tool is a separate attack surface, a separate credential store, and a separate audit trail. Clients can't independently verify what technicians did. A breach in one tool (like the 2021 Kaseya VSA incident) compromises all clients simultaneously.

## How RavenFabric Solves It

```
MSP Operations Center
┌─────────────────────────────────────────┐
│ Technician 1 (rf-cli)                   │
│ Technician 2 (rf-cli)                   │
│ Technician N (rf-cli)                   │
└──────────────────┬──────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────┐
│ MSP Relay (multi-tenant)                 │
│                                          │
│ Cryptographic tenant isolation           │
│ No cross-tenant data flow possible       │
└────┬──────────────┬──────────────┬───────┘
     │              │              │
     ▼              ▼              ▼
┌─────────┐   ┌─────────┐   ┌─────────┐
│Client A │   │Client B │   │Client C │
│(3 agents)│  │(2 agents)│  │(10 agents)│
└─────────┘   └─────────┘   └─────────┘
```

- **One tool for all clients** — technicians use `rf-cli` everywhere
- **Client controls access** — each client defines who can do what
- **Cryptographic isolation** — cross-tenant access is mathematically impossible
- **Per-client audit** — clients receive their own tamper-evident logs
- **Instant offboarding** — revoke a client's keys = complete access termination

## Per-Client Access Control

Each client defines which MSP technicians can access their environment:

```yaml
spec:
  tenant_id: acme-corp

  authorized_technicians:
    - identity: alice@msp.example.com
      role: senior-technician
      validity:
        notAfter: "2026-12-31T23:59:59Z"

    - identity: bob@msp.example.com
      role: technician
      restrictions:
        time_windows:
          - days: [Mon, Tue, Wed, Thu, Fri]
            hours: ["08:00-18:00"]
            timezone: "Europe/Oslo"

    - identity: carol@msp.example.com
      role: emergency-only
      restrictions:
        require_incident_ticket: true
```

## Per-Client Policy

Each client controls what technicians can do on their systems:

```yaml
spec:
  commands:
    allow:
      - pattern: "^kubectl get .*$"
      - pattern: "^kubectl describe .*$"
      - pattern: "^systemctl status .*$"
      - pattern: "^journalctl -u .* --since.*$"
      - pattern: "^df -h$"
      - pattern: "^free -h$"
      - pattern: "^systemctl restart [a-z-]+$"

    deny:
      - pattern: ".*rm -rf.*"
      - pattern: ".*DROP DATABASE.*"
      - pattern: ".*useradd.*"
      - pattern: ".*passwd.*"
      - pattern: ".*chmod 777.*"

  resources:
    timeoutSeconds: 120
    maxOutputBytes: 5242880
```

## Example: Day-to-Day MSP Operations

```bash
# Check server health across a client's fleet
rf exec --target 'tenant=acme-corp' "uptime && df -h && free -h"

# Restart a service for a specific client
rf exec acme-web-01 "systemctl restart nginx"

# Emergency patching across all clients
rf playbook security-patch.yaml --target 'os=ubuntu,patch-group=auto'
```

## Onboarding a New Client

```bash
# 1. Generate client identity
rf admin create-tenant \
  --id "newclient-corp" \
  --relay wss://relay.msp.example.com/meet

# 2. Deploy agents (outputs enrollment tokens)
rf admin create-enrollment \
  --tenant "newclient-corp" \
  --count 5

# 3. Install agent on client systems (one-liner)
curl -sSL https://get.ravenfabric.io | sh -s -- \
  --relay wss://relay.msp.example.com/meet \
  --token "OTP-TOKEN-HERE" \
  --tenant "newclient-corp"
```

## Offboarding a Client

```bash
# Revoke all keys for a tenant — instant, complete disconnection
rf admin revoke-tenant --id "former-client-corp"

# All agents for that tenant immediately lose connectivity
# No dangling access, no forgotten credentials
```

## Advantages Over Traditional MSP Tools

| Feature | Traditional (ConnectWise, Datto) | RavenFabric |
|---------|----------------------------------|-------------|
| Cross-tenant isolation | Logical (database row) | Cryptographic (separate keys) |
| Client audit access | Vendor portal (delayed) | Direct, real-time, tamper-evident |
| Breach blast radius | All clients via shared platform | Single tenant only |
| Client offboarding | Disable account, hope nothing lingers | Revoke keys = mathematically excluded |
| Network requirement | Always-on internet | Works air-gapped, intermittent, any transport |
| Vendor dependency | Complete (SaaS) | Self-hosted, open source |
| Per-technician audit | Limited | Every action, per-technician, per-client |
