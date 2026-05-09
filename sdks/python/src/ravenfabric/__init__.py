"""RavenFabric MCP Client SDK for Python.

A client library for communicating with RavenFabric MCP servers via the
Model Context Protocol. Supports async/await and synchronous interfaces.
"""

from ravenfabric.client import McpClient
from ravenfabric.error import McpError, McpTimeoutError, McpTransportError
from ravenfabric.protocol import ExecResult, FileContent, PolicyDecision, ToolCapability
from ravenfabric.transport import StdioTransport

__version__ = "0.1.4"
__all__ = [
    "McpClient",
    "McpError",
    "McpTimeoutError",
    "McpTransportError",
    "ExecResult",
    "FileContent",
    "PolicyDecision",
    "StdioTransport",
    "ToolCapability",
]
