"""Tests for the MCP client."""

import json
import subprocess
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from ravenfabric.client import McpClient
from ravenfabric.error import McpError, McpProtocolError, McpTimeoutError
from ravenfabric.protocol import JsonRpcResponse
from ravenfabric.transport import StdioTransport


def test_client_not_initialized():
    transport = StdioTransport("rf-mcp-server")
    client = McpClient(transport)
    with pytest.raises(McpError, match="not initialized"):
        client.exec_sync("ls")


def test_client_custom_timeout():
    transport = StdioTransport("rf-mcp-server")
    client = McpClient(transport, timeout=60.0)
    assert client._timeout == 60.0


def test_extract_tool_result_text_json():
    transport = StdioTransport("rf-mcp-server")
    client = McpClient(transport)
    result = {"content": [{"type": "text", "text": '{"output": "hello", "exit_code": 0}'}]}
    extracted = client._extract_tool_result(result, "exec")
    assert extracted["output"] == "hello"
    assert extracted["exit_code"] == 0


def test_extract_tool_result_plain_text():
    transport = StdioTransport("rf-mcp-server")
    client = McpClient(transport)
    result = {"content": [{"type": "text", "text": "plain output"}]}
    extracted = client._extract_tool_result(result, "exec")
    assert extracted["output"] == "plain output"


def test_extract_tool_result_empty():
    transport = StdioTransport("rf-mcp-server")
    client = McpClient(transport)
    with pytest.raises(McpProtocolError, match="Empty result"):
        client._extract_tool_result(None, "exec")


def test_extract_tool_result_no_content():
    transport = StdioTransport("rf-mcp-server")
    client = McpClient(transport)
    result = {"content": []}
    extracted = client._extract_tool_result(result, "exec")
    assert extracted == {}


class TestClientSync:
    """Tests using a mock subprocess for synchronous operations."""

    def _make_client_with_mock(self):
        transport = StdioTransport("rf-mcp-server")
        # Simulate a started transport
        mock_proc = MagicMock(spec=subprocess.Popen)
        mock_proc.stdin = MagicMock()
        mock_proc.stdout = MagicMock()
        mock_proc.poll.return_value = None
        transport._sync_process = mock_proc
        client = McpClient(transport)
        client._initialized = True
        return client, mock_proc

    def test_exec_sync(self):
        client, mock_proc = self._make_client_with_mock()
        response = json.dumps({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"content": [{"type": "text", "text": json.dumps({
                "output": "Linux web-01",
                "exit_code": 0,
                "duration_ms": 5.2,
            })}]},
        }).encode() + b"\n"
        mock_proc.stdout.readline.return_value = response
        result = client.exec_sync("uname -n")
        assert result.output == "Linux web-01"
        assert result.exit_code == 0

    def test_query_policy_sync_denied(self):
        client, mock_proc = self._make_client_with_mock()
        response = json.dumps({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"content": [{"type": "text", "text": json.dumps({
                "allowed": False,
                "reason": "matches deny pattern",
                "matched_rule": ".*rm.*-rf.*",
            })}]},
        }).encode() + b"\n"
        mock_proc.stdout.readline.return_value = response
        decision = client.query_policy_sync("rm -rf /")
        assert not decision.allowed
        assert decision.matched_rule == ".*rm.*-rf.*"

    def test_file_read_sync(self):
        client, mock_proc = self._make_client_with_mock()
        response = json.dumps({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"content": [{"type": "text", "text": json.dumps({
                "path": "/etc/hostname",
                "content": "web-01\n",
                "size": 7,
            })}]},
        }).encode() + b"\n"
        mock_proc.stdout.readline.return_value = response
        fc = client.file_read_sync("/etc/hostname")
        assert fc.content == "web-01\n"
        assert fc.size == 7

    def test_tool_error_sync(self):
        client, mock_proc = self._make_client_with_mock()
        response = json.dumps({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": -32000, "message": "Policy denied"},
        }).encode() + b"\n"
        mock_proc.stdout.readline.return_value = response
        with pytest.raises(McpProtocolError, match="Policy denied"):
            client.exec_sync("rm -rf /")
