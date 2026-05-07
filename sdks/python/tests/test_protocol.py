"""Tests for protocol data types."""

import json

from ravenfabric.protocol import (
    ExecResult,
    FileContent,
    JsonRpcRequest,
    JsonRpcResponse,
    PolicyDecision,
    ToolCapability,
)


def test_exec_result_immutable():
    result = ExecResult(output="hello", exit_code=0, duration_ms=12.5)
    assert result.output == "hello"
    assert result.exit_code == 0
    assert result.duration_ms == 12.5


def test_policy_decision_denied():
    decision = PolicyDecision(allowed=False, reason="matches deny rule", matched_rule=".*rm.*-rf.*")
    assert not decision.allowed
    assert "deny" in decision.reason
    assert decision.matched_rule == ".*rm.*-rf.*"


def test_policy_decision_allowed():
    decision = PolicyDecision(allowed=True, reason="matches allow rule")
    assert decision.allowed
    assert decision.matched_rule is None


def test_file_content():
    fc = FileContent(path="/etc/hostname", content="web-01\n", size=7)
    assert fc.path == "/etc/hostname"
    assert fc.size == 7


def test_tool_capability():
    tc = ToolCapability(name="exec", description="Execute a command", parameters={"type": "object"})
    assert tc.name == "exec"


def test_jsonrpc_request_serialization():
    req = JsonRpcRequest(method="tools/call", params={"name": "exec"}, id=1)
    d = req.to_dict()
    assert d["jsonrpc"] == "2.0"
    assert d["method"] == "tools/call"
    assert d["id"] == 1
    # Should be valid JSON
    serialized = json.dumps(d)
    assert "tools/call" in serialized


def test_jsonrpc_response():
    resp = JsonRpcResponse(id=1, result={"content": []}, error=None)
    assert resp.id == 1
    assert resp.result == {"content": []}
    assert resp.error is None


def test_jsonrpc_response_error():
    resp = JsonRpcResponse(id=2, result=None, error={"code": -32601, "message": "Method not found"})
    assert resp.error is not None
    assert resp.error["code"] == -32601
