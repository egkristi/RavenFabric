"""Tests for framework integrations."""

from ravenfabric.integrations.openai import (
    OPENAI_POLICY_TOOL_DEFINITION,
    OPENAI_TOOL_DEFINITION,
    OpenAIAdapter,
)
from ravenfabric.integrations.anthropic import (
    ANTHROPIC_EXEC_TOOL,
    ANTHROPIC_POLICY_TOOL,
    AnthropicAdapter,
)
from ravenfabric.integrations.autogen import AutoGenExecutor
from ravenfabric.integrations.langchain import LangChainTool
from ravenfabric.integrations.crewai import CrewAITool
from ravenfabric.transport import StdioTransport


def test_openai_tool_definition_structure():
    assert OPENAI_TOOL_DEFINITION["type"] == "function"
    func = OPENAI_TOOL_DEFINITION["function"]
    assert func["name"] == "ravenfabric_exec"
    assert "command" in func["parameters"]["properties"]
    assert "command" in func["parameters"]["required"]


def test_openai_policy_tool_definition():
    assert OPENAI_POLICY_TOOL_DEFINITION["type"] == "function"
    func = OPENAI_POLICY_TOOL_DEFINITION["function"]
    assert func["name"] == "ravenfabric_query_policy"


def test_openai_adapter_tool_definitions():
    transport = StdioTransport("rf-mcp-server")
    adapter = OpenAIAdapter(transport)
    defs = adapter.tool_definitions
    assert len(defs) == 2
    names = [d["function"]["name"] for d in defs]
    assert "ravenfabric_exec" in names
    assert "ravenfabric_query_policy" in names


def test_openai_adapter_unknown_function():
    transport = StdioTransport("rf-mcp-server")
    adapter = OpenAIAdapter(transport)
    adapter._initialized = True  # skip actual init for unit test
    # Mock the client to avoid needing a server
    # For unknown function, it returns a message directly
    result = adapter.handle_function_call("unknown_func", {})
    assert "Unknown function" in result


def test_anthropic_tool_definition_structure():
    assert ANTHROPIC_EXEC_TOOL["name"] == "ravenfabric_exec"
    assert "input_schema" in ANTHROPIC_EXEC_TOOL
    schema = ANTHROPIC_EXEC_TOOL["input_schema"]
    assert schema["type"] == "object"
    assert "command" in schema["properties"]


def test_anthropic_policy_tool_definition():
    assert ANTHROPIC_POLICY_TOOL["name"] == "ravenfabric_query_policy"


def test_anthropic_adapter_tool_definitions():
    transport = StdioTransport("rf-mcp-server")
    adapter = AnthropicAdapter(transport)
    defs = adapter.tool_definitions
    assert len(defs) == 2
    names = [d["name"] for d in defs]
    assert "ravenfabric_exec" in names
    assert "ravenfabric_query_policy" in names


def test_anthropic_adapter_unknown_tool():
    transport = StdioTransport("rf-mcp-server")
    adapter = AnthropicAdapter(transport)
    adapter._initialized = True
    result = adapter.handle_tool_use("unknown_tool", {})
    assert result["type"] == "text"
    assert "Unknown tool" in result["text"]


def test_langchain_tool_schema():
    transport = StdioTransport("rf-mcp-server")
    tool = LangChainTool(transport)
    schema = tool.args_schema
    assert schema["type"] == "object"
    assert "command" in schema["properties"]


def test_crewai_tool_schema():
    transport = StdioTransport("rf-mcp-server")
    tool = CrewAITool(transport)
    schema = tool.args_schema
    assert schema["type"] == "object"
    assert "command" in schema["properties"]


def test_autogen_executor_config():
    transport = StdioTransport("rf-mcp-server")
    executor = AutoGenExecutor(transport)
    config = executor.code_execution_config
    assert config["executor"] == "ravenfabric"
    assert config["use_docker"] is False
