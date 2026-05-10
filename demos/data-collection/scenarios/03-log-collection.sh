#!/usr/bin/env bash
# Scenario 3: Log Collection
#
# Collects and analyzes logs from across the fleet:
# access logs, query logs, agent logs, error patterns.
#
# Prerequisites: ./setup.sh has been run

set -euo pipefail
cd "$(dirname "$0")/.."

RELAY="${RF_RELAY:-ws://127.0.0.1:9092}"
RF="${RF_CLI:-rf}"

echo "=== Scenario 3: Log Collection ==="
echo ""

# 1. Web server access logs — recent entries
echo "[1] Web server access logs (last 10 entries):"
$RF --relay "$RELAY" exec --token webserver 'tail -10 /var/log/access.log' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

# 2. Web server — HTTP status code distribution
echo "[2] HTTP status code distribution:"
$RF --relay "$RELAY" exec --token webserver 'echo "Status codes:" && cat /var/log/access.log | grep -oP "HTTP/1.1\" \K[0-9]+" | sort | uniq -c | sort -rn' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

# 3. Database query logs — recent entries
echo "[3] Database query logs (last 10 entries):"
$RF --relay "$RELAY" exec --token database 'tail -10 /var/log/db-query.log' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

# 4. Database — slow queries (>200ms)
echo "[4] Slow database queries (>200ms):"
$RF --relay "$RELAY" exec --token database 'grep -E "([2-4][0-9]{2}|500)ms" /var/log/db-query.log | head -5 || echo "No slow queries found"' 2>/dev/null | grep -v "^2"
echo ""
sleep 6

# 5. Log file sizes across the fleet
echo "[5] Log file sizes:"
for token in collector webserver database; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'find /var/log -name "*.log" -o -name "*.jsonl" | head -10 | while read f; do ls -la "$f" 2>/dev/null; done' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 6. RavenFabric audit trail — what has the agent been doing?
echo "[6] Audit trail samples:"
for token in collector webserver database; do
    echo "  --- $token ---"
    $RF --relay "$RELAY" exec --token "$token" 'wc -l /var/log/rf-audit.jsonl 2>/dev/null && echo "Last entry:" && tail -1 /var/log/rf-audit.jsonl 2>/dev/null || echo "No audit log yet"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

# 7. Aggregate log line counts
echo "[7] Total log lines per agent:"
for token in collector webserver database; do
    echo -n "  $token: "
    $RF --relay "$RELAY" exec --token "$token" 'find /var/log -type f \( -name "*.log" -o -name "*.jsonl" \) -exec wc -l {} + 2>/dev/null | tail -1 || echo "0 total"' 2>/dev/null | grep -v "^2"
    sleep 6
done
echo ""

echo "=== Scenario 3 Complete ==="
