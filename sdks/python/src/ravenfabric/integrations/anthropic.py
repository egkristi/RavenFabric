"""Anthropic tool-use adapter for RavenFabric.

Provides native Claude API tool definitions backed by RavenFabric MCP calls.
Use this to define RavenFabric tools in Anthropic's tool_use format.
"""

from __future__ import annotations

from typing import Any

from ravenfabric.client import McpClient
from ravenfabric.transport import StdioTransport


# Anthropic tool definition for use with the Messages API
ANTHROPIC_EXEC_TOOL: dict[str, Any] = {
    "name": "ravenfabric_exec",
    "description": (
        "Execute a shell command on a remote machine via RavenFabric. "
        "Commands are subject to security policy enforcement (deny-by-default). "
        "Returns the command output, exit code, and execution duration."
    ),
    "input_schema": {
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "The shell command to execute on the remote agent.",
            },
            "reason": {
                "type": "string",
                "description": "Optional reason/justification for the command execution (for audit).",
            },
        },
        "required": ["command"],
    },
}

ANTHROPIC_POLICY_TOOL: dict[str, Any] = {
    "name": "ravenfabric_query_policy",
    "description": (
        "Check whether a command would be allowed by the security policy "
        "before actually executing it. Returns allowed status and the matching rule."
    ),
    "input_schema": {
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "The command to check against the policy.",
            },
        },
        "required": ["command"],
    },
}


class AnthropicAdapter:
    """Adapter that handles Anthropic tool_use blocks using RavenFabric.

    Usage:
        from ravenfabric.integrations.anthropic import AnthropicAdapter, ANTHROPIC_EXEC_TOOL

        adapter = AnthropicAdapter(transport)

        # Include tool definitions in your Messages API call
        # Then handle the tool_use block:
        result = adapter.handle_tool_use("ravenfabric_exec", {"command": "ls -la"})
    """

    def __init__(self, transport: StdioTransport, timeout: float = 30.0) -> None:
        self._client = McpClient(transport, timeout=timeout)
        self._initialized = False

    def _ensure_initialized(self) -> None:
        if not self._initialized:
            self._client.initialize_sync()
            self._initialized = True

    def handle_tool_use(
        self, tool_name: str, tool_input: dict[str, Any]
    ) -> dict[str, Any]:
        """Handle an Anthropic tool_use block and return the tool_result content.

        Args:
            tool_name: The tool name from the tool_use content block.
            tool_input: The input dict from the tool_use content block.

        Returns:
            A dict suitable for use as tool_result content.
        """
        self._ensure_initialized()

        if tool_name == "ravenfabric_exec":
            result = self._client.exec_sync(
                tool_input["command"],
                reason=tool_input.get("reason"),
            )
            return {
                "type": "text",
                "text": (
                    f"Exit code: {result.exit_code}\n"
                    f"Duration: {result.duration_ms:.1f}ms\n"
                    f"Output:\n{result.output}"
                ),
            }
        elif tool_name == "ravenfabric_query_policy":
            decision = self._client.query_policy_sync(tool_input["command"])
            return {
                "type": "text",
                "text": (
                    f"Allowed: {decision.allowed}\n"
                    f"Reason: {decision.reason}\n"
                    + (f"Rule: {decision.matched_rule}\n" if decision.matched_rule else "")
                ),
            }
        else:
            return {"type": "text", "text": f"Unknown tool: {tool_name}"}

    @property
    def tool_definitions(self) -> list[dict[str, Any]]:
        """Get all RavenFabric tool definitions for Anthropic."""
        return [ANTHROPIC_EXEC_TOOL, ANTHROPIC_POLICY_TOOL]
