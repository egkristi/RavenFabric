# Relay Setup

The relay is a stateless encrypted broker. It pairs agents with clients and forwards opaque bytes. It never sees plaintext or keys.

## Quick Start

```bash
rf-relay --listen 0.0.0.0:9090 --secret "your-meet-secret"
```

## Production Setup

### Systemd Service

```ini
# /etc/systemd/system/rf-relay.service
[Unit]
Description=RavenFabric Relay
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/rf-relay --listen 0.0.0.0:9090
Environment=RELAY_SECRET=your-secret-here
Restart=always
RestartSec=5
LimitNOFILE=65535

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

### Behind Reverse Proxy

The relay uses WebSocket, so configure your reverse proxy accordingly:

**Nginx:**

```nginx
server {
    listen 443 ssl;
    server_name relay.example.com;

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

## Geo-Distribution

Deploy multiple relays for latency optimization:

```text
relay-eu.example.com  (Frankfurt)
relay-us.example.com  (Virginia)
relay-ap.example.com  (Singapore)
```

Agents connect to the nearest relay. The relay selection is automatic based on latency probing.

## Security Properties

- **Stateless** — No session data stored on disk
- **Opaque** — Relay sees only encrypted bytes, never plaintext
- **No keys** — Relay has no access to agent or client keys
- **Ephemeral** — Can be restarted/replaced without state loss

## See Also

- [Production Deployment](production-deployment.md) — TLS termination, systemd, monitoring
- [Architecture: Transport](../architecture/transport.md) — Transport driver internals
- [Troubleshooting](troubleshooting.md) — Connection and relay issues
