# Asciinema Demo

## Recording

```bash
# Install asciinema
pip install asciinema

# Record the demo
asciinema rec --command ./demo-record.sh --title "RavenFabric: Policy Deny & Approval Flow" demo.cast

# Upload to asciinema.org
asciinema upload demo.cast
```

## What the Demo Shows

1. **Agent startup** — `rf dev` launches agent + relay in one process
2. **Allowed execution** — `rf exec 'uptime'` succeeds (matches allow pattern)
3. **Policy deny** — `rf exec 'rm -rf /tmp/important'` is blocked by deny rule
4. **Filesystem deny** — `rf exec 'cat /etc/shadow'` blocked by filesystem policy
5. **Audit trail** — every action logged with timestamp, result, and reason
6. **Policy inspection** — `rf policy show` displays active rules

## Embedding

After uploading, embed in README.md:

```markdown
[![asciicast](https://asciinema.org/a/XXXXX.svg)](https://asciinema.org/a/XXXXX)
```

## Local Playback

```bash
asciinema play demo.cast
```
