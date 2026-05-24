# Troubleshooting

Common issues and their solutions.

## Agent Not Connecting

**Symptom:** Agent starts but never establishes a connection to the relay.

**Check relay is reachable:**

```bash
# Test WebSocket connectivity
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: dGVzdA==" \
  https://relay.example.com/meet
```

**Check agent logs:**

```bash
RUST_LOG=debug rf-agent --config /etc/ravenfabric/raven.toml
```

**Common causes:**

- Firewall blocking outbound WebSocket (port 443 or 9090)
- Incorrect relay URL in config (must include `/meet` path)
- Relay not running or not yet accepting connections
- DNS resolution failure

**Fix:** Ensure the relay URL is correct and reachable. Check that any proxy between agent and relay supports WebSocket upgrade.

---

## Policy Denied My Command

**Symptom:** `rf exec` returns "denied by policy" error.

**Check what the policy allows:**

```bash
# View the active policy
cat /etc/ravenfabric/policy.yaml
```

**Test a pattern match:**

```bash
# The command must match an allow pattern AND not match any deny pattern
echo "your-command-here" | grep -P '^systemctl status .*$'
```

**Common causes:**

- Command doesn't match any `allow` pattern (deny-by-default)
- Command matches a `deny` pattern (deny always wins)
- Regex anchoring — missing `^` or `$` in the pattern
- Whitespace mismatch — extra spaces or arguments not covered by the pattern

**Fix:** Add a specific `allow` pattern for the command. Use anchored regex (`^...$`) to be precise:

```yaml
spec:
  commands:
    allow:
      - pattern: "^your-exact-command-here$"
```

---

## Handshake Failed

**Symptom:** Connection established but Noise handshake fails.

**Common causes:**

- Corrupted or wrong key file
- Key file permissions too open (must be 0600)
- Client and agent using different protocol versions
- Relay or MITM tampering with handshake bytes

**Diagnostic:**

```bash
# Check key file permissions
ls -la /etc/ravenfabric/agent.key
# Should show: -rw------- 1 ravenfabric ravenfabric

# Regenerate key if corrupted
rf-agent --generate-key /etc/ravenfabric/agent.key.new
```

---

## Agent Keeps Reconnecting

**Symptom:** Agent connects, then disconnects repeatedly (visible in logs as reconnect loop).

**Common causes:**

- Relay overloaded (rate limiting)
- Network instability (packet loss, MTU issues)
- Proxy/load balancer dropping idle WebSocket connections
- Meet token expired or invalid

**Fix for proxy timeout:**

```nginx
# Increase WebSocket timeout in nginx
proxy_read_timeout 86400;
proxy_send_timeout 86400;
```

**Fix for rate limiting:**

```bash
# Check if your IP is being rate-limited
journalctl -u rf-relay | grep "rate_limit"
```

---

## Command Timeout

**Symptom:** Command starts but returns timeout error before completing.

**Check timeout settings:**

```yaml
# In policy.yaml
spec:
  resources:
    timeoutSeconds: 300  # Increase if commands take longer
```

**Common causes:**

- Command genuinely takes too long (increase timeout)
- Command is interactive (waiting for stdin) — not supported
- Network interruption during execution

---

## Audit Log Not Writing

**Symptom:** No entries appearing in audit log file.

**Check:**

```bash
# Verify path exists and is writable
ls -la /var/log/ravenfabric/
touch /var/log/ravenfabric/test && rm /var/log/ravenfabric/test

# Check disk space
df -h /var/log/ravenfabric/
```

**Common causes:**

- Directory doesn't exist
- Permission denied (service user can't write)
- Disk full
- Path mismatch between config and actual filesystem

---

## Output Truncated

**Symptom:** Command output is cut off mid-stream.

**Cause:** Output exceeded `maxOutputBytes` limit in policy.

**Fix:** Increase the limit:

```yaml
spec:
  resources:
    maxOutputBytes: 52428800  # 50 MB
```

Or pipe to limit output on the agent side:

```bash
rf exec my-agent "journalctl -u app --since '1 hour ago' | tail -1000"
```

---

## Dev Mode Issues

**Symptom:** `rf dev` doesn't work.

**Common causes:**

- Port 9090 already in use (another relay or service)
- Missing policy file at default path

**Fix:**

```bash
# Check what's using port 9090
lsof -i :9090

# Run dev mode with explicit port
rf dev --port 9091
```

---

## Debug Logging

Enable verbose logging for any component:

```bash
# Full debug output
RUST_LOG=debug rf-agent --config /etc/ravenfabric/raven.toml

# Component-specific
RUST_LOG=rf_transport=debug,rf_crypto=trace rf-agent --config /etc/ravenfabric/raven.toml

# Only warnings and errors
RUST_LOG=warn rf-agent --config /etc/ravenfabric/raven.toml
```

---

## Getting Help

If the issue persists:

1. Reproduce with `RUST_LOG=debug` and capture full output
2. Report issues via email to <security@ravenfabric.io>
3. Include: OS, version, config (redact secrets), debug log output
