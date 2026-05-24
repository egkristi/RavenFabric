# File Transfer

`rf cp` transfers files and directory trees between any two points in the mesh over the encrypted channel. Transfers are compressed, chunked, and integrity-verified. The same filesystem path policy that governs `rf exec` applies to all transfers.

## Syntax

```text
rf cp [OPTIONS] <SOURCE> <DEST>
```

Source and destination use the format `[agent:]path`. If no agent prefix is given, the path is local.

| Format | Meaning |
|--------|---------|
| `./file.txt` | Local file |
| `web-01:/etc/app/config.txt` | Remote file on agent `web-01` |
| `web-01:/var/log/` | Remote directory |

---

## Single File Transfer

### Local → Remote

```bash
rf cp --token <TOKEN> ./config.tar.gz web-01:/etc/app/config.tar.gz
```

Output:

```text
→ Transferring config.tar.gz to web-01 (1.2 MB)
  ████████████████████ 100%  1.2 MB  2.3 MB/s
✓  sha256:4f8e9c2d1a3b5f7e...  verified on web-01
   audited · policy-checked · 540ms
```

### Remote → Local

```bash
rf cp --token <TOKEN> web-01:/var/log/app/error.log ./error.log
```

### Remote → Remote

Transfers between two agents are routed through the CLI:

```bash
rf cp --token <TOKEN> web-01:/etc/nginx/nginx.conf web-02:/etc/nginx/nginx.conf
```

---

## Recursive Directory Transfer

Use `-r` to copy a directory tree:

```bash
# Upload certificates to a remote agent
rf cp -r --token <TOKEN> ./certs/ db-01:/etc/app/certs/

# Download logs from a remote agent
rf cp -r --token <TOKEN> web-01:/var/log/app/ ./logs/
```

Output:

```text
→ Transferring certs/ to db-01 (3 files, 28 KB)
  ████████████████████ 100%
✓  3 files verified on db-01
   audit entries: 3 · policy-checked
```

The transfer is policy-checked per file. If any file path is denied by agent policy, the entire transfer aborts and the denial is logged.

---

## Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--token <TOKEN>` | `-t` | Meet token | Required |
| `--relay <URL>` | `-r` | Relay URL | `RF_RELAY` env / `ws://127.0.0.1:9090` |
| `-r` / `--recursive` | — | Recursive directory transfer | Off |
| `--no-compress` | — | Disable automatic compression | Compression on |
| `--chunk-size <BYTES>` | — | Chunk size in bytes | 131072 (128 KB) |
| `--overwrite` | — | Overwrite existing files | Error if exists |
| `--dry-run` | — | Show what would be transferred without doing it | — |

---

## Compression

Compression is applied automatically based on file extension. Binary formats (`.gz`, `.zip`, `.jpg`, `.png`, `.bin`) are sent uncompressed; text formats (`.log`, `.yaml`, `.json`, `.toml`, `.txt`, `.conf`) are compressed with zstd before transfer.

Override with `--no-compress` to skip compression entirely.

---

## Integrity Verification

Every file is verified on arrival using SHA-256:

1. Sender computes SHA-256 of the source file before transfer
2. File data is sent in chunks over the encrypted channel
3. Receiver recomputes SHA-256 of the received bytes
4. Hashes are compared — if they differ, the file is rejected and an error is returned
5. The verified hash appears in the audit log entry

If integrity verification fails, the destination file is removed and the error is logged.

---

## Policy Enforcement

File transfers obey the `filesystem` policy of the destination agent:

```yaml
# policy.yaml — agent allows uploads only to /opt/deploy/
spec:
  filesystem:
    allow:
      - path: /opt/deploy
    deny:
      - path: /etc
      - path: /root
```

A transfer to `/etc/shadow` is denied before any data is sent. The denial is written to the audit log.

Symlinks in the destination path are resolved to their real path before the policy check, preventing traversal attacks.

---

## Audit Trail

Each file transfer produces one audit entry per file:

```json
{
  "seq": 2041,
  "ts": "2026-05-21T10:45:00Z",
  "action": "file_push",
  "caller": "f7a3..c912",
  "path": "/etc/app/config.tar.gz",
  "bytes": 1258291,
  "sha256": "4f8e9c2d1a3b5f7e...",
  "decision": "allow",
  "duration_ms": 540
}
```

---

## Examples

### Deploy a build artifact

```bash
# Build locally, transfer to all web nodes
cargo build --release
for TOKEN in $WEB_TOKENS; do
  rf cp --token "$TOKEN" ./target/release/myapp web-01:/opt/app/myapp
done
```

### Sync configuration

```bash
rf cp --token <TOKEN> ./nginx.conf web-01:/etc/nginx/nginx.conf
rf exec --token <TOKEN> "nginx -t && systemctl reload nginx"
```

### Collect logs

```bash
rf cp -r --token <TOKEN> web-01:/var/log/app/ ./collected-logs/$(date +%Y%m%d)/
```

---

## See Also

- [CLI Reference: rf cp](../reference/cli.md#rf-cp) — Full option reference
- [Policy Configuration](policy-config.md) — Filesystem path policies
- [Remote Execution](execution.md) — Run commands after transfer
- [Audit Log Format](../reference/audit-log-format.md) — Transfer log entries
