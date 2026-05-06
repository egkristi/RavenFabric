# Edge & IoT Fleet Management

Manage thousands of distributed edge devices — from Raspberry Pis to industrial gateways — through a single encrypted control plane with bandwidth-aware policies.

## The Problem

Edge and IoT fleets are deployed on unreliable networks (cellular, satellite, LoRa), behind NATs and firewalls, often in remote locations. Traditional management tools assume always-on connectivity and waste bandwidth on polling. SSH doesn't scale, and commercial IoT platforms lock you in.

## How RavenFabric Solves It

```
Fleet Operator (rf-cli)
    │
    ▼
┌──────────────────────────┐
│ Relay (cloud/on-prem)    │
└──────────┬───────────────┘
           │
    ┌──────┼──────┬──────────┐
    ▼      ▼      ▼          ▼
 Agent   Agent   Agent     Agent
 (RPi)  (Gateway) (Sensor) (Edge GPU)
    │
    └── Metered cellular (DTN queuing)
```

- **Agents connect outbound** — no inbound ports, works behind any NAT
- **DTN store-carry-forward** — commands queue when offline, execute on reconnect
- **Bandwidth-aware** — agents on metered connections batch responses
- **Multi-transport** — WebSocket, QUIC, serial, LoRa, Bluetooth

## Example: Firmware Update Across Fleet

```bash
# Target all agents with label "device=gateway" in region "eu-west"
rf playbook deploy-firmware.yaml \
  --target 'device=gateway,region=eu-west' \
  --rollback-on-failure

# Check which agents are currently reachable
rf status --label 'device=gateway'

# Execute on a single device
rf exec edge-gw-042 "cat /etc/firmware-version"
```

## Policy for Constrained Devices

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl restart iot-agent$"
      - pattern: "^cat /etc/firmware-version$"
      - pattern: "^df -h$"
      - pattern: "^journalctl -u iot-agent --since '1 hour ago'$"
      - pattern: "^apt-get update && apt-get upgrade -y --only-security$"
    deny:
      - pattern: ".*reboot.*"
      - pattern: ".*dd if=.*"
      - pattern: ".*rm -rf.*"
  resources:
    timeoutSeconds: 120
    maxOutputBytes: 1048576  # 1 MB — constrained uplink
```

## DTN Configuration for Intermittent Connectivity

```toml
[agent]
id = "edge-gw-042"
relay = "wss://relay.fleet.example.com/meet"

[transport]
driver = "websocket"
reconnect_interval = 30
max_retries = 0  # Never give up

[dtn]
enabled = true
queue_path = "/var/lib/ravenfabric/queue"
max_queue_size = "50MB"
ttl_seconds = 86400  # Commands valid for 24h
priority_delivery = true  # Metrics/health first
```

## Fleet Scale

| Metric | Value |
|--------|-------|
| Tested concurrent agents | 10,000+ per relay |
| Memory per idle agent | < 5 MB |
| Binary size (musl, stripped) | ~8 MB |
| Reconnect after network loss | Exponential backoff (5s → 5min) |
| Offline command queue | Persistent (survives reboot) |

## Supported Device Classes

| Device | Transport | Notes |
|--------|-----------|-------|
| Raspberry Pi 3/4/5 | WebSocket, QUIC | Full agent, arm64 musl |
| Industrial gateways | WebSocket | Standard deployment |
| LoRa mesh nodes | Serial + LoRa driver | Minimal agent profile |
| Cellular IoT (NB-IoT) | WebSocket over cellular | DTN mode recommended |
| Satellite-connected | WebSocket | High-latency tolerant |
| Air-gapped sites | Serial, USB sneakernet | NNCP-style physical media |
