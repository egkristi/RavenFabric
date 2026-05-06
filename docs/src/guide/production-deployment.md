# Production Deployment

This guide covers deploying RavenFabric in production environments with proper key management, service configuration, and monitoring.

## Prerequisites

- RavenFabric binaries installed (see [Installation](../getting-started/installation.md))
- A server for the relay (any Linux/macOS host with a public IP or domain)
- Agent systems reachable outbound to the relay (no inbound ports needed)

## 1. Relay Setup

### Generate relay secret

```bash
# Generate a strong meet secret
RELAY_SECRET=$(openssl rand -hex 32)
echo "RELAY_SECRET=$RELAY_SECRET" >> /etc/ravenfabric/relay.env
chmod 600 /etc/ravenfabric/relay.env
```

### Configuration

```toml
# /etc/ravenfabric/relay.toml
[relay]
listen = "0.0.0.0:9090"
meet_secret = "env:RELAY_SECRET"

[rate_limit]
requests_per_second = 100
burst = 200
per_ip = true
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
ExecStart=/usr/local/bin/rf-relay --config /etc/ravenfabric/relay.toml
EnvironmentFile=/etc/ravenfabric/relay.env
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

The relay itself speaks WebSocket without TLS. In production, place it behind a reverse proxy:

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
        proxy_read_timeout 86400;  # Keep WebSocket alive
    }
}
```

## 2. Agent Deployment

### Generate agent key

```bash
sudo mkdir -p /etc/ravenfabric
rf-agent --generate-key /etc/ravenfabric/agent.key
sudo chmod 600 /etc/ravenfabric/agent.key
sudo chown ravenfabric:ravenfabric /etc/ravenfabric/agent.key
```

### Agent configuration

```toml
# /etc/ravenfabric/raven.toml
[agent]
id = "prod-web-01"
relay = "wss://relay.example.com/meet"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
driver = "websocket"
reconnect_interval = 5
max_retries = 0  # Infinite reconnect

[resources]
max_output_bytes = 10485760  # 10 MB
timeout_seconds = 300
max_concurrent = 10
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
ReadWritePaths=/var/log/ravenfabric

[Install]
WantedBy=multi-user.target
```

### Enable

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now rf-agent
sudo journalctl -u rf-agent -f  # Watch logs
```

## 3. Key Rotation

Rotate agent keys periodically:

```bash
# Generate new key
rf-agent --generate-key /etc/ravenfabric/agent.key.new

# Swap keys (atomic)
sudo mv /etc/ravenfabric/agent.key.new /etc/ravenfabric/agent.key
sudo chmod 600 /etc/ravenfabric/agent.key
sudo chown ravenfabric:ravenfabric /etc/ravenfabric/agent.key

# Restart agent to pick up new key
sudo systemctl restart rf-agent
```

## 4. Monitoring

### Health check

```bash
# Check agent is running and connected
rf status prod-web-01

# Prometheus metrics endpoint (if enabled)
curl http://localhost:9100/metrics
```

### Agent configuration for metrics

```toml
[agent]
metrics_addr = "127.0.0.1:9100"
```

### Log rotation

```
# /etc/logrotate.d/ravenfabric
/var/log/ravenfabric/audit.jsonl {
    daily
    rotate 90
    compress
    delaycompress
    missingok
    notifempty
    copytruncate
}
```

## 5. Security Hardening

| Control | Implementation |
|---------|----------------|
| Minimal permissions | `User=ravenfabric`, no root |
| System protection | `ProtectSystem=strict`, `NoNewPrivileges=true` |
| Key file permissions | `chmod 600`, owned by service user |
| Relay secret | Environment variable, not in config file |
| Audit log integrity | Append-only, separate partition recommended |
| Network exposure | Agent: outbound only. Relay: single port behind TLS proxy |
| Policy reload | SIGHUP for hot-reload without restart |
