"""Tests for error types."""

from ravenfabric.error import McpError, McpProtocolError, McpTimeoutError, McpTransportError


def test_error_hierarchy():
    assert issubclass(McpTransportError, McpError)
    assert issubclass(McpTimeoutError, McpError)
    assert issubclass(McpProtocolError, McpError)


def test_error_messages():
    e = McpTransportError("pipe broken")
    assert str(e) == "pipe broken"

    e = McpTimeoutError("timed out after 30s")
    assert "30s" in str(e)
