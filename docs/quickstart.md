# Quick Start

Get RavenFabric running locally in development mode.

## 1. Build

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build --release
```

## 2. Development Mode (single machine)

The `rf dev` command starts a local relay + agent pair with no authentication for testing:

```bash
# Terminal 1: Start dev mode (relay + agent)
./target/release/rf dev

# Terminal 2: Execute a command
./target/release/rf exec local "echo hello world"
```

## 3. Production Setup

### Start the Relay

```bash
export RELAY_SECRET="your-secret-here"
./target/release/rf-relay --config relay.toml
```

### Start the Agent

```bash
# Generate keys (first time only)
./target/release/rf-agent --generate-key /etc/ravenfabric/agent.key

# Start agent
./target/release/rf-agent --config /etc/ravenfabric/raven.toml
```

### Execute Commands

```bash
# Single agent
rf exec web-01 "systemctl status nginx"

# Check status
rf status
```

## 4. Policy

Create `/etc/ravenfabric/policy.yaml`:

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^cat /var/log/.*"
    deny:
      - pattern: ".*rm.*-rf.*"
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 60
```

Any command not matching an allow rule is denied by default.
