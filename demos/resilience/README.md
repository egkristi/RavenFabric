# Resilience Demo

Demonstrates RavenFabric's resilience capabilities: agent reconnection with exponential backoff, relay restart recovery, network partition handling, and graceful degradation.

## Architecture

```
                    ┌─────────────┐
                    │  rf-relay   │ ←── restart, partition
                    │  (port 9094)│
                    └──────┬──────┘
                           │ WebSocket
              ┌────────────┼────────────┐
              │            │            │
        ┌─────┴─────┐ ┌───┴───┐ ┌─────┴─────┐
        │ rf-agent-1 │ │agent-2│ │ rf-agent-3 │
        │  (web-01)  │ │(db-01)│ │  (web-02)  │
        └────────────┘ └───────┘ └────────────┘
```

The agents automatically reconnect with exponential backoff + jitter when the relay restarts or network partitions occur.

## Scenarios

| # | Scenario | Script | Description |
|---|----------|--------|-------------|
| 1 | Agent Reconnect | `scenarios/01-agent-reconnect.sh` | Kill agent process, verify auto-restart and reconnect |
| 2 | Relay Restart | `scenarios/02-relay-restart.sh` | Restart relay, verify all agents reconnect |
| 3 | Network Partition | `scenarios/03-network-partition.sh` | Simulate partition, verify recovery when connectivity restores |
| 4 | Graceful Degradation | `scenarios/04-graceful-degradation.sh` | One agent down, others continue operating |
| 5 | Backoff Behavior | `scenarios/05-backoff-behavior.sh` | Observe exponential backoff + jitter in reconnect attempts |

## Prerequisites

- Docker
- `rf` CLI binary

## Quick Start

```bash
chmod +x setup.sh
./setup.sh
```

## Teardown

```bash
./setup.sh teardown
```
