#!/usr/bin/env bash
# Human Approval for AI-Controlled Agents — Multi-Distro Linux
#
# Demonstrates that the human approval workflow works identically across
# all Linux distributions. The MCP server is a static binary — same
# approval gate on Ubuntu, Alpine, Fedora, or any other distro.
#
# Prerequisites: rf and rf-mcp-server binaries in $PATH

set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== Human Approval — Multi-Distro ==="
echo ""

# --- Part 1: Same Approval Gate, Any Distro ---

echo "--- Part 1: Universal Approval Gate ---"
echo ""
echo "  The MCP server is a static binary — no dependencies."
echo "  Human approval works identically on every distribution:"
echo ""
echo "  Ubuntu (glibc):   rf-mcp-server → rf_request_approval → operator"
echo "  Alpine (musl):    rf-mcp-server → rf_request_approval → operator"
echo "  Fedora (rpm):     rf-mcp-server → rf_request_approval → operator"
echo ""
echo "  Same binary. Same approval workflow. Same security guarantees."
echo ""
sleep 6

# --- Part 2: Cross-Distro AI Safety ---

echo "--- Part 2: Why This Matters for AI Safety ---"
echo ""
echo "  AI agents running on different distros get the same protection:"
echo ""
echo "  1. Policy engine denies disallowed commands (first gate)"
echo "  2. Human approval required for high-risk operations (second gate)"
echo "  3. Rate limiting prevents abuse (60 req/min default)"
echo "  4. Every action audited in structured JSON"
echo ""
echo "  No distro-specific configuration needed."
echo ""
sleep 6

# --- Part 3: Deployment Pattern ---

echo "--- Part 3: MCP Server Deployment ---"
echo ""
echo "  ┌───────────────┬─────────────────────────────────────┐"
echo "  │ Component     │ Description                         │"
echo "  ├───────────────┼─────────────────────────────────────┤"
echo "  │ rf-mcp-server │ Static binary, any Linux distro     │"
echo "  │ --policy      │ YAML policy (deny-by-default)       │"
echo "  │ --audit       │ JSON-lines audit log                │"
echo "  │ --api-token   │ Bearer token for AI authentication  │"
echo "  │ --rate-limit  │ Requests per minute (default: 60)   │"
echo "  │ --callers     │ RBAC: per-AI-agent policy profiles  │"
echo "  └───────────────┴─────────────────────────────────────┘"
echo ""
sleep 6

# --- Key Takeaways ---

echo "=== Key Takeaways ==="
echo ""
echo "  1. Human approval gate works on every Linux distribution"
echo "  2. Static binary — no per-distro packaging or dependencies"
echo "  3. RBAC lets different AI agents have different permissions"
echo "  4. Same audit format across all distros"
echo ""
echo "=== Human Approval Scenario Complete ==="
