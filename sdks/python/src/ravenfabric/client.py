"""MCP client for RavenFabric."""

from __future__ import annotations

import asyncio
from typing import Any

from ravenfabric.error import McpError, McpProtocolError, McpTimeoutError
from ravenfabric.protocol import ExecResult, FileContent, PolicyDecision, ToolCapability
from ravenfabric.transport import StdioTransport


class McpClient:
    """Client for communicating with a RavenFabric MCP server.

    Provides both async and synchronous interfaces for all operations.
    The sync methods (suffixed with _sync) run the async event loop internally.
    """

    def __init__(self, transport: StdioTransport, timeout: float = 30.0) -> None:
        self._transport = transport
        self._timeout = timeout
        self._initialized = False

    # --- Async interface ---

    async def initialize(self) -> None:
        """Perform the MCP initialization handshake."""
        await self._transport.start()
        resp = await self._call("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "ravenfabric-python", "version": "0.2.0"},
        })
        if resp.error:
            raise McpProtocolError(f"Initialization failed: {resp.error}")
        # Send initialized notification
        await self._transport.send("notifications/initialized", {})
        self._initialized = True

    async def exec(
        self,
        command: str,
        timeout: float | None = None,
        reason: str | None = None,
    ) -> ExecResult:
        """Execute a command on the remote agent."""
        self._check_initialized()
        params: dict[str, Any] = {"command": command}
        if timeout is not None:
            params["timeout"] = timeout
        if reason is not None:
            params["reason"] = reason

        result = await self._call_tool("exec", params)
        return ExecResult(
            output=result.get("output", ""),
            exit_code=result.get("exit_code", -1),
            duration_ms=result.get("duration_ms", 0.0),
        )

    async def query_policy(self, command: str) -> PolicyDecision:
        """Check whether a command would be allowed by policy."""
        self._check_initialized()
        result = await self._call_tool("query_policy", {"command": command})
        return PolicyDecision(
            allowed=result.get("allowed", False),
            reason=result.get("reason", ""),
            matched_rule=result.get("matched_rule"),
        )

    async def file_read(self, path: str) -> FileContent:
        """Read a file from the remote agent."""
        self._check_initialized()
        result = await self._call_tool("file_read", {"path": path})
        return FileContent(
            path=result.get("path", path),
            content=result.get("content", ""),
            size=result.get("size", 0),
        )

    async def file_write(self, path: str, content: str) -> None:
        """Write content to a file on the remote agent."""
        self._check_initialized()
        await self._call_tool("file_write", {"path": path, "content": content})

    async def list_tools(self) -> list[ToolCapability]:
        """List available MCP tools on the server."""
        self._check_initialized()
        resp = await self._call("tools/list", {})
        if resp.error:
            raise McpProtocolError(f"list_tools failed: {resp.error}")
        tools = resp.result.get("tools", []) if resp.result else []
        return [
            ToolCapability(
                name=t["name"],
                description=t.get("description", ""),
                parameters=t.get("inputSchema", {}),
            )
            for t in tools
        ]

    async def request_approval(self, command: str, reason: str) -> str:
        """Request human approval for a command."""
        self._check_initialized()
        result = await self._call_tool(
            "request_approval", {"command": command, "reason": reason}
        )
        return result.get("approval_id", "")

    async def close(self) -> None:
        """Close the transport connection."""
        await self._transport.close()
        self._initialized = False

    # --- Synchronous interface ---

    def initialize_sync(self) -> None:
        """Perform the MCP initialization handshake (synchronous)."""
        self._transport.start_sync()
        resp = self._transport.send_sync("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "ravenfabric-python", "version": "0.2.0"},
        })
        if resp.error:
            raise McpProtocolError(f"Initialization failed: {resp.error}")
        self._transport.send_sync("notifications/initialized", {})
        self._initialized = True

    def exec_sync(
        self,
        command: str,
        timeout: float | None = None,
        reason: str | None = None,
    ) -> ExecResult:
        """Execute a command (synchronous)."""
        self._check_initialized()
        params: dict[str, Any] = {"command": command}
        if timeout is not None:
            params["timeout"] = timeout
        if reason is not None:
            params["reason"] = reason

        result = self._call_tool_sync("exec", params)
        return ExecResult(
            output=result.get("output", ""),
            exit_code=result.get("exit_code", -1),
            duration_ms=result.get("duration_ms", 0.0),
        )

    def query_policy_sync(self, command: str) -> PolicyDecision:
        """Check policy (synchronous)."""
        self._check_initialized()
        result = self._call_tool_sync("query_policy", {"command": command})
        return PolicyDecision(
            allowed=result.get("allowed", False),
            reason=result.get("reason", ""),
            matched_rule=result.get("matched_rule"),
        )

    def file_read_sync(self, path: str) -> FileContent:
        """Read a file (synchronous)."""
        self._check_initialized()
        result = self._call_tool_sync("file_read", {"path": path})
        return FileContent(
            path=result.get("path", path),
            content=result.get("content", ""),
            size=result.get("size", 0),
        )

    def file_write_sync(self, path: str, content: str) -> None:
        """Write a file (synchronous)."""
        self._check_initialized()
        self._call_tool_sync("file_write", {"path": path, "content": content})

    def list_tools_sync(self) -> list[ToolCapability]:
        """List tools (synchronous)."""
        self._check_initialized()
        resp = self._transport.send_sync("tools/list", {})
        if resp.error:
            raise McpProtocolError(f"list_tools failed: {resp.error}")
        tools = resp.result.get("tools", []) if resp.result else []
        return [
            ToolCapability(
                name=t["name"],
                description=t.get("description", ""),
                parameters=t.get("inputSchema", {}),
            )
            for t in tools
        ]

    def close_sync(self) -> None:
        """Close the transport (synchronous)."""
        self._transport.close_sync()
        self._initialized = False

    # --- Internal helpers ---

    def _check_initialized(self) -> None:
        if not self._initialized:
            raise McpError("Client not initialized. Call initialize() first.")

    async def _call(self, method: str, params: dict[str, Any]) -> Any:
        """Send an RPC call with timeout."""
        try:
            return await asyncio.wait_for(
                self._transport.send(method, params),
                timeout=self._timeout,
            )
        except asyncio.TimeoutError as e:
            raise McpTimeoutError(
                f"Request '{method}' timed out after {self._timeout}s"
            ) from e

    async def _call_tool(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        """Call an MCP tool and extract the result content."""
        resp = await self._call("tools/call", {"name": tool_name, "arguments": arguments})
        if resp.error:
            raise McpProtocolError(f"Tool '{tool_name}' failed: {resp.error}")
        return self._extract_tool_result(resp.result, tool_name)

    def _call_tool_sync(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        """Call an MCP tool synchronously."""
        resp = self._transport.send_sync(
            "tools/call", {"name": tool_name, "arguments": arguments}
        )
        if resp.error:
            raise McpProtocolError(f"Tool '{tool_name}' failed: {resp.error}")
        return self._extract_tool_result(resp.result, tool_name)

    def _extract_tool_result(self, result: Any, tool_name: str) -> dict[str, Any]:
        """Extract structured result from MCP tool response content array."""
        if not result:
            raise McpProtocolError(f"Empty result from tool '{tool_name}'")
        # MCP tools/call returns {content: [{type: "text", text: "..."}]}
        content = result.get("content", [])
        if not content:
            return {}
        # Parse the first text content block as JSON
        import json

        for block in content:
            if block.get("type") == "text":
                try:
                    return json.loads(block["text"])  # type: ignore[no-any-return]
                except (json.JSONDecodeError, KeyError):
                    return {"output": block.get("text", "")}
        return {}
