"""MCP protocol data types."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class ExecResult:
    """Result of a command execution."""

    output: str
    exit_code: int
    duration_ms: float


@dataclass(frozen=True)
class PolicyDecision:
    """Result of a policy query."""

    allowed: bool
    reason: str
    matched_rule: str | None = None


@dataclass(frozen=True)
class FileContent:
    """Content of a file read operation."""

    path: str
    content: str
    size: int


@dataclass(frozen=True)
class ToolCapability:
    """Description of an available MCP tool."""

    name: str
    description: str
    parameters: dict[str, Any]


@dataclass(frozen=True)
class JsonRpcRequest:
    """JSON-RPC 2.0 request."""

    method: str
    params: dict[str, Any]
    id: int

    def to_dict(self) -> dict[str, Any]:
        return {
            "jsonrpc": "2.0",
            "method": self.method,
            "params": self.params,
            "id": self.id,
        }


@dataclass(frozen=True)
class JsonRpcResponse:
    """JSON-RPC 2.0 response."""

    id: int
    result: Any | None = None
    error: dict[str, Any] | None = None
