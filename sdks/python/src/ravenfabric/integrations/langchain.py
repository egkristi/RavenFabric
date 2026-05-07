"""LangChain tool integration for RavenFabric."""

from __future__ import annotations

from typing import Any

from ravenfabric.client import McpClient
from ravenfabric.transport import StdioTransport


class LangChainTool:
    """A LangChain-compatible tool that executes commands via RavenFabric.

    Usage with LangChain:
        from ravenfabric.integrations import LangChainTool

        tool = LangChainTool(transport)
        # Use in a LangChain agent as a tool
    """

    name: str = "ravenfabric_exec"
    description: str = (
        "Execute a shell command on a remote machine via RavenFabric. "
        "Commands are subject to security policy enforcement. "
        "Input should be the shell command to execute."
    )

    def __init__(
        self,
        transport: StdioTransport,
        timeout: float = 30.0,
    ) -> None:
        self._transport = transport
        self._client = McpClient(transport, timeout=timeout)
        self._initialized = False

    def _ensure_initialized(self) -> None:
        if not self._initialized:
            self._client.initialize_sync()
            self._initialized = True

    def run(self, command: str) -> str:
        """Execute a command and return the output (LangChain sync interface)."""
        self._ensure_initialized()
        result = self._client.exec_sync(command)
        if result.exit_code != 0:
            return f"Error (exit code {result.exit_code}): {result.output}"
        return result.output

    async def arun(self, command: str) -> str:
        """Execute a command and return the output (LangChain async interface)."""
        if not self._initialized:
            await self._client.initialize()
            self._initialized = True
        result = await self._client.exec(command)
        if result.exit_code != 0:
            return f"Error (exit code {result.exit_code}): {result.output}"
        return result.output

    @property
    def args_schema(self) -> dict[str, Any]:
        """JSON Schema for the tool input."""
        return {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute",
                }
            },
            "required": ["command"],
        }
