# RavenFabric Python MCP Client SDK

A Python client for communicating with RavenFabric MCP servers via the
Model Context Protocol (MCP). Provides async/await and synchronous interfaces
for secure remote command execution, policy querying, and file operations.

## Installation

```bash
pip install ravenfabric
```

## Quick Start

```python
import asyncio
from ravenfabric import McpClient, StdioTransport

async def main():
    transport = StdioTransport("rf-mcp-server", ["--policy", "policy.yaml"])
    client = McpClient(transport)
    await client.initialize()

    # Execute a command
    result = await client.exec("ls -la")
    print(result.output)

    # Query policy before executing
    decision = await client.query_policy("rm -rf /")
    assert not decision.allowed

asyncio.run(main())
```

## Synchronous Usage

```python
from ravenfabric import McpClient, StdioTransport

transport = StdioTransport("rf-mcp-server", ["--policy", "policy.yaml"])
client = McpClient(transport)
client.initialize_sync()

result = client.exec_sync("uname -a")
print(result.output)
```

## API Reference

### `StdioTransport(command, args, env=None)`

Spawns the MCP server as a subprocess with JSON-RPC over stdio.

### `McpClient(transport, timeout=30.0)`

Main client class. All methods have both async (`await client.exec(...)`)
and sync (`client.exec_sync(...)`) variants.

**Methods:**

| Method | Description |
|--------|-------------|
| `initialize()` | Perform MCP handshake |
| `exec(command, timeout=None, reason=None)` | Execute a command |
| `query_policy(command)` | Check if a command would be allowed |
| `file_read(path)` | Read a file |
| `file_write(path, content)` | Write a file |
| `list_tools()` | List available MCP tools |
| `request_approval(command, reason)` | Request human approval |

### Response Types

- `ExecResult(output, exit_code, duration_ms)`
- `PolicyDecision(allowed, reason, matched_rule)`
- `FileContent(path, content, size)`
- `ToolCapability(name, description, parameters)`

## Framework Integration

### LangChain

```python
from ravenfabric.integrations import LangChainTool

tool = LangChainTool(transport)
# Use as a LangChain tool in your agent chain
```

### CrewAI

```python
from ravenfabric.integrations import CrewAITool

tool = CrewAITool(transport)
# Use as a CrewAI agent tool
```

## License

MIT
