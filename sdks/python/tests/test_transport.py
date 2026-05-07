"""Tests for transport layer."""

import pytest

from ravenfabric.error import McpTransportError
from ravenfabric.transport import StdioTransport


def test_transport_init():
    transport = StdioTransport("rf-mcp-server", ["--policy", "test.yaml"])
    assert not transport.is_running


def test_transport_with_env():
    transport = StdioTransport("rf-mcp-server", env={"RUST_LOG": "debug"})
    assert transport._env == {"RUST_LOG": "debug"}


def test_transport_start_sync_missing_binary():
    transport = StdioTransport("nonexistent-binary-xyz-123")
    with pytest.raises(McpTransportError, match="not found"):
        transport.start_sync()


@pytest.mark.asyncio
async def test_transport_start_async_missing_binary():
    transport = StdioTransport("nonexistent-binary-xyz-123")
    with pytest.raises(McpTransportError, match="not found"):
        await transport.start()


def test_transport_send_sync_not_started():
    transport = StdioTransport("rf-mcp-server")
    with pytest.raises(McpTransportError, match="not started"):
        transport.send_sync("test", {})


@pytest.mark.asyncio
async def test_transport_send_async_not_started():
    transport = StdioTransport("rf-mcp-server")
    with pytest.raises(McpTransportError, match="not started"):
        await transport.send("test", {})


def test_transport_id_increments():
    transport = StdioTransport("rf-mcp-server")
    assert transport._next_id() == 1
    assert transport._next_id() == 2
    assert transport._next_id() == 3


def test_transport_parse_response_valid():
    import json

    transport = StdioTransport("rf-mcp-server")
    data = json.dumps({"jsonrpc": "2.0", "id": 1, "result": {"ok": True}}).encode()
    resp = transport._parse_response(data, expected_id=1)
    assert resp.id == 1
    assert resp.result == {"ok": True}


def test_transport_parse_response_id_mismatch():
    import json

    transport = StdioTransport("rf-mcp-server")
    data = json.dumps({"jsonrpc": "2.0", "id": 99, "result": {}}).encode()
    with pytest.raises(McpTransportError, match="mismatch"):
        transport._parse_response(data, expected_id=1)


def test_transport_parse_response_invalid_json():
    transport = StdioTransport("rf-mcp-server")
    with pytest.raises(McpTransportError, match="Invalid JSON"):
        transport._parse_response(b"not json at all", expected_id=1)
