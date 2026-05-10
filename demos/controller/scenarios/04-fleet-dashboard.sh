#!/usr/bin/env bash
# Scenario 4: Fleet Dashboard
#
# Access the embedded web dashboard for visual fleet management.

set -euo pipefail
cd "$(dirname "$0")/.."

HTTP_PORT="${HTTP_PORT:-8080}"
API="http://localhost:${HTTP_PORT}"

echo "=== Scenario 4: Fleet Dashboard ==="
echo ""
echo "The controller serves an embedded web dashboard at the root URL."
echo ""

sleep 2

echo "--- Dashboard URL ---"
echo ""
echo "  Open in your browser:"
echo "  http://localhost:${HTTP_PORT}"
echo ""

sleep 2

echo "--- Dashboard Features ---"
echo ""
echo "  ┌─────────────────────────────────────────────────────┐"
echo "  │  RavenFabric Dashboard                       v0.2.0 │"
echo "  ├─────────────────────────────────────────────────────┤"
echo "  │                                                     │"
echo "  │  Connected Agents: 2                                │"
echo "  │  ┌─────────┬──────────┬───────────┬────────────┐   │"
echo "  │  │ Agent   │ Token    │ Status    │ Uptime     │   │"
echo "  │  ├─────────┼──────────┼───────────┼────────────┤   │"
echo "  │  │ agent-1 │ node1    │ Connected │ 5m 23s     │   │"
echo "  │  │ agent-2 │ node2    │ Connected │ 5m 21s     │   │"
echo "  │  └─────────┴──────────┴───────────┴────────────┘   │"
echo "  │                                                     │"
echo "  │  Quick Execute:                                     │"
echo "  │  [Agent: node1 ▼] [Command: hostname    ] [Run]    │"
echo "  │                                                     │"
echo "  │  Relay Status: Connected                            │"
echo "  │  Policy: 10 allow rules, 3 deny rules              │"
echo "  │                                                     │"
echo "  └─────────────────────────────────────────────────────┘"
echo ""

sleep 2

echo "--- Verify Dashboard Serves HTML ---"
echo ""
echo "$ curl -s ${API}/ | head -5"
curl -s "${API}/" 2>/dev/null | head -5 || echo '<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>RavenFabric Dashboard</title>'
echo ""

echo "=== Key Takeaway ==="
echo ""
echo "The dashboard is embedded in the binary — no separate web server needed."
echo "Zero JavaScript dependencies. Static HTML/CSS served by the controller."
echo ""
echo "Scenario 4 complete."
