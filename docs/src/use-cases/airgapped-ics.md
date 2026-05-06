# Air-Gapped Industrial Control Systems

Secure remote maintenance of industrial control systems (ICS/SCADA/OT) in environments with no internet connectivity — using physical media transport and store-carry-forward delivery.

## The Problem

Critical infrastructure (power grids, water treatment, manufacturing) operates air-gapped networks by regulation (IEC 62443, NERC CIP). Maintenance requires physical presence or risky "temporary" network connections that often become permanent attack surfaces.

## How RavenFabric Solves It

```
Engineering Workstation (corporate network)
    │
    │ rf-cli → commands queued
    │
    ▼
┌──────────────────────────────────────┐
│ DTN Queue (store-carry-forward)      │
│ Encrypted bundles written to media   │
└──────────────────┬───────────────────┘
                   │
          Physical media transfer
          (USB, optical disc, serial)
                   │
                   ▼
┌──────────────────────────────────────┐
│ Air-Gapped OT Network               │
│                                      │
│  Agent ← reads bundles from media    │
│    │                                 │
│    ├─ Policy check                   │
│    ├─ Execute command                │
│    ├─ Audit log                      │
│    └─ Response bundle → media out    │
└──────────────────────────────────────┘
```

- **No network connection required** — commands travel on physical media
- **Cryptographically sealed** — bundles are Noise-encrypted, tamper-evident
- **Policy-checked on the agent** — even air-gapped commands are denied if not allowed
- **Full audit trail** — every action logged, exportable for compliance

## Transport Options for Air-Gapped Sites

| Transport | Use Case | Latency |
|-----------|----------|---------|
| USB drive (NNCP-style) | Planned maintenance windows | Hours |
| Serial cable | Direct agent connection | Seconds |
| Optical disc (WORM) | Compliance-required immutable transfer | Hours |
| QR-stream visual channel | Small commands, no electronic media | Minutes |
| Bluetooth/BLE | Proximity maintenance tablet | Seconds |

## Example: Scheduled Maintenance via USB

```bash
# On corporate workstation: queue commands for air-gapped site
rf exec --dtn --ttl 24h scada-plc-01 "cat /var/log/plc-diagnostics.log"
rf exec --dtn --ttl 24h scada-plc-01 "systemctl status plc-runtime"
rf exec --dtn --ttl 24h scada-hmi-01 "screenshot-capture /tmp/hmi-state.png"

# Export bundles to USB media
rf dtn export --media /mnt/usb-transfer/

# --- Physical transfer to air-gapped site ---

# On air-gapped site: import and execute
rf dtn import --media /mnt/usb-transfer/
# Agent processes commands, writes responses to outbound queue

# Export responses
rf dtn export --media /mnt/usb-transfer/ --direction outbound

# --- Physical transfer back ---

# On corporate workstation: read responses
rf dtn import --media /mnt/usb-transfer/
rf dtn show-responses
```

## Policy for ICS Environments

ICS policies are extremely restrictive — read-only diagnostics only:

```yaml
spec:
  commands:
    allow:
      # Diagnostics only
      - pattern: "^cat /var/log/.*\\.log$"
      - pattern: "^systemctl status .*$"
      - pattern: "^journalctl -u .* --since '.*' --until '.*'$"
      - pattern: "^df -h$"
      - pattern: "^free -m$"
      - pattern: "^uptime$"
      - pattern: "^ip addr show$"

    deny:
      # No modifications whatsoever
      - pattern: ".*systemctl (start|stop|restart|enable|disable).*"
      - pattern: ".*apt.*"
      - pattern: ".*yum.*"
      - pattern: ".*pip.*"
      - pattern: ".*rm .*"
      - pattern: ".*mv .*"
      - pattern: ".*cp .*"
      - pattern: ".*chmod.*"
      - pattern: ".*chown.*"
      - pattern: ".*reboot.*"
      - pattern: ".*shutdown.*"
      - pattern: ".*kill.*"
      - pattern: ".*dd .*"
      - pattern: ".*mkfs.*"
      - pattern: ".*iptables.*"

  resources:
    timeoutSeconds: 30
    maxOutputBytes: 1048576  # 1 MB
  
  # Immutable rules — cannot be overridden even by admin
  immutable:
    deny:
      - pattern: ".*"  # Deny everything by default
    # Only the explicit allow list above passes
```

## Compliance Mapping

| Regulation | RavenFabric Control |
|------------|---------------------|
| IEC 62443 SR 1.1 | Noise XX mutual authentication |
| IEC 62443 SR 2.8 | Append-only audit logging |
| IEC 62443 SR 3.1 | E2E encryption on physical media |
| NERC CIP-005 | No electronic network path (physical media only) |
| NERC CIP-007 | Policy-enforced command restrictions |
| NIS2 Art. 21 | Risk management, incident detection, audit |

## DTN Configuration for Air-Gap Mode

```toml
[agent]
id = "scada-plc-01"
mode = "airgap"  # No network, media-only

[dtn]
enabled = true
queue_path = "/var/lib/ravenfabric/queue"
media_path = "/mnt/transfer"  # Watch for incoming bundles
max_queue_size = "100MB"
ttl_seconds = 172800  # 48h validity
verify_content_address = true  # Reject tampered bundles

[transport]
driver = "nncp"  # Physical media transport
poll_interval = 10  # Check media mount every 10s
```
