# Production Deployment

This guide covers deploying RavenFabric in production with proper key management, service configuration, and monitoring.

## Prerequisites

- RavenFabric binaries installed (see [Installation](../getting-started/installation.md))
- A server for the relay (any Linux/macOS host with a public IP or domain)
- Agent systems reachable outbound to the relay (no inbound ports needed)

## 1. Relay Setup

### Generate relay secret

```bash
RELAY_SECRET=$(openssl rand -hex 32)
echo "RELAY_SECRET=$RELAY_SECRET" > /etc/ravenfabric/relay.env
chmod 600 /etc/ravenfabric/relay.env
```

### systemd service

```ini
# /etc/systemd/system/rf-relay.service
[Unit]
Description=RavenFabric Relay
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/rf-relay --listen 0.0.0.0:9090
EnvironmentFile=/etc/ravenfabric/relay.env
Restart=always
RestartSec=5
User=ravenfabric
Group=ravenfabric
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true

[Install]
WantedBy=multi-user.target
```

### Enable and start

```bash
sudo useradd -r -s /usr/sbin/nologin ravenfabric
sudo mkdir -p /etc/ravenfabric /var/log/ravenfabric
sudo chown ravenfabric:ravenfabric /var/log/ravenfabric

sudo systemctl daemon-reload
sudo systemctl enable --now rf-relay
sudo systemctl status rf-relay
```

### TLS termination (recommended)

The relay speaks plain WebSocket. In production, front it with a TLS-terminating reverse proxy:

```nginx
# /etc/nginx/sites-available/relay
server {
    listen 443 ssl http2;
    server_name relay.example.com;

    ssl_certificate /etc/letsencrypt/live/relay.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/relay.example.com/privkey.pem;

    location /meet {
        proxy_pass http://127.0.0.1:9090;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 86400;
    }
}
```

> **Note:** Even without TLS, the RavenFabric wire protocol provides end-to-end encryption (Noise XX). TLS adds defense-in-depth and protects metadata.

## 2. Agent Deployment

### Configuration

```toml
# /etc/ravenfabric/raven.toml
[agent]
id = "prod-web-01"
relay = "wss://relay.example.com/meet"
token = "your-meet-token"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
reconnect_interval = 5
max_retries = 0
```

### Policy file

```yaml
# /etc/ravenfabric/policy.yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl (status|restart) (nginx|postgresql|app)$"
      - pattern: "^journalctl -u .* --since '.*'$"
      - pattern: "^df -h$"
      - pattern: "^free -h$"
      - pattern: "^uptime$"
    deny:
      - pattern: ".*rm -rf.*"
      - pattern: ".*dd if=.*"
      - pattern: ".*> /dev/.*"
  resources:
    timeoutSeconds: 300
    maxOutputBytes: 10485760
```

### systemd service

```ini
# /etc/systemd/system/rf-agent.service
[Unit]
Description=RavenFabric Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/rf-agent --config /etc/ravenfabric/raven.toml
Restart=always
RestartSec=5
User=ravenfabric
Group=ravenfabric
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/log/ravenfabric

[Install]
WantedBy=multi-user.target
```

### File permissions

```bash
sudo chown ravenfabric:ravenfabric /etc/ravenfabric/agent.key
sudo chmod 600 /etc/ravenfabric/agent.key
sudo chmod 644 /etc/ravenfabric/raven.toml
sudo chmod 644 /etc/ravenfabric/policy.yaml
```

### Enable and start

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now rf-agent
sudo journalctl -u rf-agent -f
```

## 3. Monitoring

### Audit log

The audit log (`audit.jsonl`) records every action:

```bash
# Watch audit log in real-time
tail -f /var/log/ravenfabric/audit.jsonl | jq .
```

### Prometheus metrics

Enable the metrics endpoint:

```toml
[agent]
metrics_addr = "127.0.0.1:9100"
```

Or via CLI flag:

```bash
rf-agent --config /etc/ravenfabric/raven.toml --metrics-addr 127.0.0.1:9100
```

### Health checks

```bash
# Check agent connectivity
rf status --relay wss://relay.example.com/meet --token <token>
```

## 4. Security Hardening

| Measure | Implementation |
|---------|---------------|
| File permissions | Key files 0600, owned by service user |
| systemd sandboxing | `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome` |
| Network isolation | Agent only needs outbound to relay (no inbound ports) |
| Audit retention | Rotate with `logrotate`, ship to SIEM |
| Policy minimization | Allow only exact commands needed for each agent's role |
| TLS termination | Nginx/Caddy reverse proxy with Let's Encrypt |
| Rate limiting | Relay has built-in per-IP rate limiting (20 conn/min) |

## See Also

- [Configuration Reference](../reference/config.md) — Full config schema
- [Policy YAML Reference](../reference/policy-yaml.md) — Policy file syntax
- [Troubleshooting](troubleshooting.md) — Common issues and solutions
