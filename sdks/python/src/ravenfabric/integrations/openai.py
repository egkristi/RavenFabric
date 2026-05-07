"""OpenAI function-calling adapter for RavenFabric.

Translates OpenAI tool/function schemas into RavenFabric MCP calls.
Use this to define RavenFabric as a tool in OpenAI's function calling API.
"""

from __future__ import annotations

from typing import Any

from ravenfabric.client import McpClient
from ravenfabric.transport import StdioTransport


# OpenAI tool definition for use with the Chat Completions API
OPENAI_TOOL_DEFINITION: dict[str, Any] = {
    "type": "function",
    "function": {
        "name": "ravenfabric_exec",
        "description": (
            "Execute a shell command on a remote machine via RavenFabric. "
            "Commands are subject to security policy enforcement (deny-by-default). "
            "Returns the command output, exit code, and execution duration."
        ),
        "parameters": {
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
    },
}

OPENAI_POLICY_TOOL_DEFINITION: dict[str, Any] = {
    "type": "function",
    "function": {
        "name": "ravenfabric_query_policy",
        "description": (
            "Check whether a command would be allowed by the security policy "
            "before actually executing it. Returns allowed status and the matching rule."
        ),
        "parameters": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to check against the policy.",
                },
            },
            "required": ["command"],
        },
    },
}


class OpenAIAdapter:
    """Adapter that handles OpenAI function call results using RavenFabric.

    Usage:
        from ravenfabric.integrations.openai import OpenAIAdapter, OPENAI_TOOL_DEFINITION

        adapter = OpenAIAdapter(transport)

        # Include OPENAI_TOOL_DEFINITION in your tools list when calling OpenAI
        # Then handle the function call:
        result = adapter.handle_function_call("ravenfabric_exec", {"command": "ls -la"})
    """

    def __init__(self, transport: StdioTransport, timeout: float = 30.0) -> None:
        self._client = McpClient(transport, timeout=timeout)
        self._initialized = False

    def _ensure_initialized(self) -> None:
        if not self._initialized:
            self._client.initialize_sync()
            self._initialized = True

    def handle_function_call(
        self, function_name: str, arguments: dict[str, Any]
    ) -> str:
        """Handle an OpenAI function call and return the result as a string.

        Args:
            function_name: The function name from the OpenAI response.
            arguments: The parsed arguments dict from the OpenAI response.

        Returns:
            A string result to send back as the function call response.
        """
        self._ensure_initialized()

        if function_name == "ravenfabric_exec":
            result = self._client.exec_sync(
                arguments["command"],
                reason=arguments.get("reason"),
            )
            return (
                f"Exit code: {result.exit_code}\n"
                f"Duration: {result.duration_ms:.1f}ms\n"
                f"Output:\n{result.output}"
            )
        elif function_name == "ravenfabric_query_policy":
            decision = self._client.query_policy_sync(arguments["command"])
            status = "ALLOWED" if decision.allowed else "DENIED"
            parts = [f"Status: {status}", f"Reason: {decision.reason}"]
            if decision.matched_rule:
                parts.append(f"Rule: {decision.matched_rule}")
            return "\n".join(parts)
        else:
            return f"Unknown function: {function_name}"

    @property
    def tool_definitions(self) -> list[dict[str, Any]]:
        """Get all RavenFabric tool definitions for OpenAI."""
        return [OPENAI_TOOL_DEFINITION, OPENAI_POLICY_TOOL_DEFINITION]
