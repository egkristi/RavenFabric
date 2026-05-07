"""Exception types for RavenFabric MCP client."""


class McpError(Exception):
    """Base exception for all MCP client errors."""

    pass


class McpTransportError(McpError):
    """Raised when the transport layer fails (process crash, pipe broken)."""

    pass


class McpTimeoutError(McpError):
    """Raised when a request exceeds the configured timeout."""

    pass


class McpProtocolError(McpError):
    """Raised when the server sends an invalid or unexpected response."""

    pass
