# Secret Management

RavenFabric provides a built-in sealed secret store on every agent. Secrets are pushed over the encrypted channel, encrypted at rest, and never written to disk in plaintext. Fleet-wide distribution and zero-downtime rotation are first-class operations.

## Overview

| Capability | Description |
|------------|-------------|
| Per-agent sealed store | Each agent holds an encrypted secret store |
| Fleet-wide push | Push one secret to all matching agents in a single command |
| Zero-downtime rotation | Old value stays valid during a grace period while the new value propagates |
| External backends | Pull from HashiCorp Vault, AWS Secrets Manager, Azure Key Vault, GCP Secret Manager |
| Audit trail | Every push, rotation, and access is logged — value hashes only, never plaintext |

Secrets are zeroed from memory when released. The `rf secret list` command returns only secret names and value hashes — never the plaintext values.

---

## Pushing a Secret

### Single Agent

```bash
rf secret push \
  --token <TOKEN> \
  --name DB_PASSWORD \
  --value "hunter2"
```

Output:

```text
✓  web-01  sealed  sha256:a3f9b2c1...  (new)
```

### Fleet-Wide (All Agents)

```bash
rf secret push \
  --relay wss://relay.example.com:9090 \
  --token <TOKEN> \
  --name DB_PASSWORD \
  --value "$DB_PASSWORD" \
  --selector role=web
```

Output:

```text
✓  web-01  sealed  sha256:a3f9b2c1...  (new)
✓  web-02  sealed  sha256:a3f9b2c1...  (new)
✓  web-03  sealed  sha256:a3f9b2c1...  (new)
```

The `--selector` flag matches agents by label. Omitting it targets the single agent identified by `--token`.

### Options

| Option | Short | Description | Required |
|--------|-------|-------------|----------|
| `--token <TOKEN>` | `-t` | Meet token for relay pairing | Yes |
| `--name <NAME>` | `-n` | Secret name (identifier) | Yes |
| `--value <VALUE>` | `-v` | Secret plaintext value | Yes |
| `--grace-period <SECS>` | `-g` | Seconds old value stays valid during rotation | No (default: 0) |
| `--selector <LABELS>` | — | Label selector for fleet targeting | No |

---

## Zero-Downtime Rotation

Use `--grace-period` when rotating a secret that running processes still reference. The agent keeps the old value valid alongside the new one for the specified number of seconds:

```bash
rf secret push \
  --token <TOKEN> \
  --name DB_PASSWORD \
  --value "$NEW_PASSWORD" \
  --grace-period 300
```

Output:

```text
✓  web-01  sealed  sha256:d7e8f9a0...  (rotated, old valid 300s)
✓  web-02  sealed  sha256:d7e8f9a0...  (rotated, old valid 300s)
✓  web-03  sealed  sha256:d7e8f9a0...  (rotated, old valid 300s)
```

During the grace period:

- Both old and new values are valid
- Applications can restart against the new value without a hard cutover
- After the grace period expires, the old value is purged automatically

**Recommended rotation workflow:**

1. Push new value with `--grace-period 300`
2. Restart your application pods/services to pick up the new value
3. Wait for grace period to expire
4. Old value is purged — rotation complete

---

## Listing Secrets

```bash
rf secret list --token <TOKEN>
```

Output:

```text
NAME          HASH              ROTATED AT
DB_PASSWORD   sha256:d7e8f9a0   2026-05-21T10:30:00Z
API_KEY       sha256:b1c2d3e4   2026-05-20T08:15:00Z
TLS_CERT      sha256:f5a6b7c8   —
```

Only names and value hashes are returned. The plaintext is never exposed over the RPC channel.

---

## Injecting Secrets into Commands

Sealed secrets are available as environment variables during command execution:

```yaml
# policy.yaml
spec:
  secrets:
    inject:
      - name: DB_PASSWORD
        env: DATABASE_PASSWORD
      - name: API_KEY
        env: API_KEY
```

The agent injects the secret value into the process environment at execution time. The value is never written to disk.

```bash
rf exec --token <TOKEN> "echo DB is at $DATABASE_PASSWORD"
# Note: env var is set on the agent side, not expanded client-side
```

---

## External Secret Backends

Instead of pushing values directly, configure the agent to pull from an external backend. Values are fetched at execution time — not stored locally.

### HashiCorp Vault

```toml
# raven.toml
[secrets.backend]
type = "vault"
address = "https://vault.internal:8200"
token = "env:VAULT_TOKEN"           # or a file path
path = "secret/data/ravenfabric"    # KV v2 path
```

```yaml
# policy.yaml
spec:
  secrets:
    backend: vault
    inject:
      - vault_key: db_password
        env: DB_PASSWORD
```

### AWS Secrets Manager

```toml
[secrets.backend]
type = "aws"
region = "eu-west-1"
# Uses standard AWS credential chain (env vars, IAM role, ~/.aws/credentials)
```

```yaml
spec:
  secrets:
    backend: aws
    inject:
      - aws_secret_id: "prod/ravenfabric/db"
        aws_key: "password"
        env: DB_PASSWORD
```

### Azure Key Vault

```toml
[secrets.backend]
type = "azure"
vault_url = "https://myvault.vault.azure.net"
# Uses Azure managed identity or env: AZURE_CLIENT_ID / AZURE_CLIENT_SECRET
```

```yaml
spec:
  secrets:
    backend: azure
    inject:
      - secret_name: "db-password"
        env: DB_PASSWORD
```

### GCP Secret Manager

```toml
[secrets.backend]
type = "gcp"
project_id = "my-project"
# Uses Application Default Credentials
```

```yaml
spec:
  secrets:
    backend: gcp
    inject:
      - secret_id: "db-password"
        version: "latest"
        env: DB_PASSWORD
```

### Generic HTTP Backend

For custom secret stores:

```toml
[secrets.backend]
type = "http"
url = "https://secrets.internal/api/v1/secret/{name}"
auth_header = "Authorization"
auth_value = "env:SECRET_STORE_TOKEN"
json_path = "$.data.value"         # JSONPath to extract value
```

---

## Security Properties

- **Never logged in plaintext** — audit entries record `value_hash` only
- **Zeroed from memory** — secret values are wiped from memory immediately after use
- **Encrypted at rest** — the agent's sealed store uses hardware-grade symmetric encryption
- **Authenticated channel** — secrets travel only over the Noise XX encrypted channel; the relay is content-blind
- **No policy check on push** — the Noise handshake already authenticates the caller; only enrolled clients can push secrets
- **Immutable audit** — every push, rotation, and list operation is append-only in the audit log

---

## See Also

- [CLI Reference: rf secret](../reference/cli.md#rf-secret) — Full option reference
- [Policy YAML: secrets](../reference/policy-yaml.md) — Injection configuration
- [Audit Log Format](../reference/audit-log-format.md) — What gets logged
- [Enrollment](enrollment.md) — How agents are authenticated before they can receive secrets
