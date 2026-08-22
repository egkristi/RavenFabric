#!/usr/bin/env bash
# Record the asciinema demo for RavenFabric
# Usage: ./demos/record.sh
#
# Prerequisites:
#   - asciinema installed (pipx install asciinema)
#   - cargo build --release -p rf-cli -p rf-agent -p rf-relay
#
# Output: demos/ravenfabric-demo.cast

set -euo pipefail
cd "$(dirname "$0")/.."

CAST_FILE="demos/ravenfabric-demo.cast"
SCRIPT="demos/demo-script.sh"

echo "Recording demo to $CAST_FILE ..."
echo "This will run the scripted demo automatically."
echo ""

# Wrap asciinema in `script` for a stable PTY (needed in CI/containers where
# asciinema's own PTY allocation can be interrupted, truncating the recording).
script -qec \
  "asciinema rec \"$CAST_FILE\" \
    --title 'RavenFabric — Security-first remote execution & mesh networking' \
    --cols 100 \
    --rows 30 \
    --command 'bash $SCRIPT' \
    --overwrite" \
  /dev/null

echo ""
echo "Done! Cast saved to: $CAST_FILE"
echo ""
echo "Preview:  asciinema play $CAST_FILE"
echo "Upload:   asciinema upload $CAST_FILE"
echo "Embed:    <script src=\"https://asciinema.org/a/XXXX.js\" async></script>"
