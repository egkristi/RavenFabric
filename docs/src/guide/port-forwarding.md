# Port Forwarding

RavenFabric supports three port forwarding modes, equivalent to `ssh -L`, `ssh -R`, and `ssh -D`. All traffic is encrypted end-to-end over the agent channel. Network access is controlled by the agent's network policy.

## Modes

| Mode | CLI Flag | Description |
|------|----------|-------------|
| Local forward | `-L local:remote` | Listen locally, connect through the agent |
| Remote forward | `-R agent-port:local` | Agent listens, tunnel back to your machine |
| SOCKS5 proxy | `-D local-port` | Dynamic proxy for arbitrary destinations |
| HTTP proxy | `rf proxy` | HTTP-aware proxy with method/path policy |

---

## Local Port Forwarding (`-L`)

Forward a local port through the agent to a remote service. Useful for accessing internal databases or services without a VPN.

```bash
rf forward --token <TOKEN> -L 127.0.0.1:5432 -R db.internal:5432
```

This listens on `127.0.0.1:5432` locally. Any connection to that port is tunneled through the agent to `db.internal:5432`.

### Usage example: PostgreSQL

```bash
# Forward local port 5432 to the internal database
rf forward --token <TOKEN> -L 127.0.0.1:5432 -R postgres-primary.internal:5432 &

# Connect with psql as if it were local
psql -h 127.0.0.1 -U app -d mydb
```

### Options

| Option | Description |
|--------|-------------|
| `--token <TOKEN>` | Meet token for agent pairing |
| `-L <LOCAL_HOST:LOCAL_PORT>` | Local listen address |
| `-R <REMOTE_HOST:REMOTE_PORT>` | Remote target (resolved from the agent's network) |
| `--keep-alive` | Keep the forward open after the first connection closes |

Network access to the remote target is checked against the agent's `network` policy:

```yaml
spec:
  network:
    allow:
      - hostname: "postgres-primary.internal"
        ports: ["5432"]
    deny:
      - cidr: "0.0.0.0/0"   # deny everything else
```

---

## Remote Port Forwarding (`-R`)

The agent listens on a port and tunnels connections back to a service on your machine. Useful for exposing a local service to the agent's network without inbound firewall rules.

```bash
rf forward --token <TOKEN> -R 8080:127.0.0.1:3000
```

The agent listens on port `8080`. Any connection to the agent on that port is forwarded to `127.0.0.1:3000` on your local machine.

### Usage example: local development server

```bash
# Expose your local dev server to the agent's network
rf forward --token <TOKEN> -R 8080:127.0.0.1:3000 &

# From the agent's network, access your dev server at agent-ip:8080
```

---

## SOCKS5 Dynamic Proxy (`-D`)

The agent acts as a SOCKS5 proxy server. Any SOCKS5-compatible client connecting to the local port can reach arbitrary hosts via the agent's network.

```bash
rf forward --token <TOKEN> -D 127.0.0.1:1080
```

Configure your browser or `curl` to use `socks5://127.0.0.1:1080`:

```bash
curl --proxy socks5://127.0.0.1:1080 http://internal-service.corp/api/status
```

Network destinations are checked against the agent's `network` policy for every connection attempt.

---

## HTTP-Aware Proxy (`rf proxy`)

For HTTP traffic, `rf proxy` provides a full HTTP-aware proxy that enforces method, path, and header policies.

```bash
rf proxy --token <TOKEN> --listen 127.0.0.1:8888 --target https://api.internal
```

Every HTTP request through the proxy is:

1. Checked against method/path policy
2. Audited with method, path, status code, latency, and caller identity
3. Optional header injection/stripping (e.g., add auth headers, strip sensitive headers)

### Network policy for HTTP proxy

```yaml
spec:
  network:
    allow:
      - hostname: "api.internal"
        ports: ["443"]
  http:
    allow:
      - methods: ["GET", "POST"]
        paths: ["/api/v1/.*"]
    deny:
      - methods: ["DELETE"]
      - paths: ["/admin/.*"]
    inject_headers:
      - name: "X-RavenFabric-Caller"
        value: "{caller_id}"
    strip_headers:
      - "Authorization"
```

---

## Policy Enforcement

All forwarding modes are subject to the agent's network policy. Connections to disallowed destinations are rejected before any data is forwarded:

```yaml
spec:
  network:
    allow:
      - cidr: "10.0.0.0/8"
        ports: ["5432", "6379", "443"]
      - hostname: "*.internal.corp"
        ports: ["443"]
    deny:
      - cidr: "169.254.0.0/16"   # deny metadata endpoints
      - cidr: "0.0.0.0/0"        # deny everything else
```

Every forwarded connection attempt produces an audit log entry regardless of the policy decision.

---

## Audit Trail

```json
{
  "seq": 3102,
  "ts": "2026-05-21T11:00:00Z",
  "action": "tcp_forward",
  "caller": "f7a3..c912",
  "local": "127.0.0.1:54321",
  "remote": "db.internal:5432",
  "bytes_tx": 1024,
  "bytes_rx": 8192,
  "decision": "allow",
  "duration_ms": 4200
}
```

---

## See Also

- [CLI Reference: rf forward](../reference/cli.md#rf-forward) — Full option reference
- [Policy YAML: network](../reference/policy-yaml.md#network-policy) — Configuring allowed destinations
- [Remote Execution](execution.md) — Running commands on agents
- [Use Cases: CloudNativePG](../use-cases/cloudnativepg.md) — Database access via port forward
