"""Stdio transport for MCP communication."""

from __future__ import annotations

import asyncio
import json
import os
import subprocess
from typing import Any

from ravenfabric.error import McpTransportError
from ravenfabric.protocol import JsonRpcRequest, JsonRpcResponse


class StdioTransport:
    """Spawns an MCP server subprocess and communicates via JSON-RPC over stdio.

    The subprocess stdin/stdout carry newline-delimited JSON-RPC messages.
    Stderr is captured separately for diagnostics.
    """

    def __init__(
        self,
        command: str,
        args: list[str] | None = None,
        env: dict[str, str] | None = None,
    ) -> None:
        self._command = command
        self._args = args or []
        self._env = env
        self._process: asyncio.subprocess.Process | None = None
        self._sync_process: subprocess.Popen[bytes] | None = None
        self._request_id = 0

    async def start(self) -> None:
        """Start the MCP server subprocess (async)."""
        env = os.environ.copy()
        if self._env:
            env.update(self._env)

        try:
            self._process = await asyncio.create_subprocess_exec(
                self._command,
                *self._args,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env=env,
            )
        except FileNotFoundError as e:
            raise McpTransportError(
                f"MCP server binary not found: {self._command}"
            ) from e
        except OSError as e:
            raise McpTransportError(
                f"Failed to start MCP server: {e}"
            ) from e

    def start_sync(self) -> None:
        """Start the MCP server subprocess (synchronous)."""
        env = os.environ.copy()
        if self._env:
            env.update(self._env)

        try:
            self._sync_process = subprocess.Popen(
                [self._command, *self._args],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=env,
            )
        except FileNotFoundError as e:
            raise McpTransportError(
                f"MCP server binary not found: {self._command}"
            ) from e
        except OSError as e:
            raise McpTransportError(
                f"Failed to start MCP server: {e}"
            ) from e

    def _next_id(self) -> int:
        self._request_id += 1
        return self._request_id

    async def send(self, method: str, params: dict[str, Any]) -> JsonRpcResponse:
        """Send a JSON-RPC request and read the response (async)."""
        if self._process is None or self._process.stdin is None:
            raise McpTransportError("Transport not started. Call start() first.")

        request = JsonRpcRequest(method=method, params=params, id=self._next_id())
        line = json.dumps(request.to_dict()) + "\n"

        try:
            self._process.stdin.write(line.encode())
            await self._process.stdin.drain()
        except (BrokenPipeError, ConnectionError) as e:
            raise McpTransportError(f"Failed to write to MCP server: {e}") from e

        return await self._read_response(request.id)

    def send_sync(self, method: str, params: dict[str, Any]) -> JsonRpcResponse:
        """Send a JSON-RPC request and read the response (synchronous)."""
        if self._sync_process is None or self._sync_process.stdin is None:
            raise McpTransportError("Transport not started. Call start_sync() first.")

        request = JsonRpcRequest(method=method, params=params, id=self._next_id())
        line = json.dumps(request.to_dict()) + "\n"

        try:
            self._sync_process.stdin.write(line.encode())
            self._sync_process.stdin.flush()
        except (BrokenPipeError, OSError) as e:
            raise McpTransportError(f"Failed to write to MCP server: {e}") from e

        return self._read_response_sync(request.id)

    async def _read_response(self, expected_id: int) -> JsonRpcResponse:
        """Read a JSON-RPC response line from stdout."""
        if self._process is None or self._process.stdout is None:
            raise McpTransportError("Transport not started.")

        try:
            line = await self._process.stdout.readline()
        except (asyncio.IncompleteReadError, ConnectionError) as e:
            raise McpTransportError(f"Failed to read from MCP server: {e}") from e

        if not line:
            raise McpTransportError("MCP server closed connection unexpectedly.")

        return self._parse_response(line, expected_id)

    def _read_response_sync(self, expected_id: int) -> JsonRpcResponse:
        """Read a JSON-RPC response line from stdout (synchronous)."""
        if self._sync_process is None or self._sync_process.stdout is None:
            raise McpTransportError("Transport not started.")

        line = self._sync_process.stdout.readline()
        if not line:
            raise McpTransportError("MCP server closed connection unexpectedly.")

        return self._parse_response(line, expected_id)

    def _parse_response(self, line: bytes, expected_id: int) -> JsonRpcResponse:
        """Parse a JSON-RPC response from raw bytes."""
        try:
            data = json.loads(line)
        except json.JSONDecodeError as e:
            raise McpTransportError(f"Invalid JSON from MCP server: {e}") from e

        resp_id = data.get("id")
        if resp_id != expected_id:
            raise McpTransportError(
                f"Response ID mismatch: expected {expected_id}, got {resp_id}"
            )

        return JsonRpcResponse(
            id=resp_id,
            result=data.get("result"),
            error=data.get("error"),
        )

    async def close(self) -> None:
        """Terminate the MCP server subprocess."""
        if self._process is not None:
            self._process.terminate()
            await self._process.wait()
            self._process = None

    def close_sync(self) -> None:
        """Terminate the MCP server subprocess (synchronous)."""
        if self._sync_process is not None:
            self._sync_process.terminate()
            self._sync_process.wait()
            self._sync_process = None

    @property
    def is_running(self) -> bool:
        """Check if the server process is still running."""
        if self._process is not None:
            return self._process.returncode is None
        if self._sync_process is not None:
            return self._sync_process.poll() is None
        return False
