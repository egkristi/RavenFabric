# Controller / Web UI Demo

Demonstrates the RavenFabric controller's HTTP API server and embedded web dashboard for fleet management, real-time monitoring, and remote execution.

## Architecture

```
                 ┌───────────────────────┐
   Browser ────► │  Controller (HTTP)    │ ◄──── rf CLI
                 │  port 8080            │
                 │  ┌─────────────────┐  │
                 │  │ Embedded Web UI │  │
                 │  │ (dashboard)     │  │
                 │  └─────────────────┘  │
                 └──────────┬────────────┘
                            │ WebSocket
                    ┌───────┼───────┐
                    │       │       │
              ┌─────┴┐  ┌──┴──┐  ┌─┴─────┐
              │agent-1│  │ag-2 │  │agent-3│
              └───────┘  └─────┘  └───────┘
```

The controller provides an HTTP API and web dashboard on top of the relay, allowing fleet management through a browser or REST calls.

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Web dashboard (embedded HTML) |
| `/api/agents` | GET | List connected agents |
| `/api/health` | GET | Controller health check |
| `/api/exec` | POST | Execute command on agent |
| `/api/policy` | GET | View current policy |

## Scenarios

| # | Scenario | Script | Description |
|---|----------|--------|-------------|
| 1 | Agent List | `scenarios/01-agent-list.sh` | Query `/api/agents` to see connected fleet |
| 2 | Health Check | `scenarios/02-health-check.sh` | Verify controller and agent health via API |
| 3 | Remote Execution | `scenarios/03-remote-execution.sh` | Execute commands through the HTTP API |
| 4 | Fleet Dashboard | `scenarios/04-fleet-dashboard.sh` | Access the web UI dashboard |
| 5 | Policy View | `scenarios/05-policy-view.sh` | Inspect policy configuration through API |

## Prerequisites

- Docker
- `curl`
- `rf` CLI binary (optional)

## Quick Start

```bash
chmod +x setup.sh
./setup.sh
```

Then open [http://localhost:8080](http://localhost:8080) in your browser.

## Teardown

```bash
./setup.sh teardown
```
