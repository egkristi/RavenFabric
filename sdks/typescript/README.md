# RavenFabric TypeScript MCP Client SDK

A TypeScript/JavaScript client for communicating with RavenFabric MCP servers
via the Model Context Protocol. Provides a fully typed async interface for
secure remote command execution, policy querying, and file operations.

## Installation

```bash
npm install ravenfabric
```

## Quick Start

```typescript
import { McpClient, StdioTransport } from "ravenfabric";

const transport = new StdioTransport("rf-mcp-server", ["--policy", "policy.yaml"]);
const client = new McpClient(transport);
await client.initialize();

// Execute a command
const result = await client.exec("ls -la");
console.log(result.output);

// Query policy before executing
const decision = await client.queryPolicy("rm -rf /");
console.log(decision.allowed); // false

// Clean up
client.close();
```

## API Reference

### `new StdioTransport(command, args?, env?)`

Spawns the MCP server as a subprocess with JSON-RPC over stdio.

### `new McpClient(transport, timeout?)`

Main client class. All methods are async.

**Methods:**

| Method | Description |
|--------|-------------|
| `initialize()` | Perform MCP handshake |
| `exec(command, options?)` | Execute a command |
| `queryPolicy(command)` | Check if a command would be allowed |
| `fileRead(path)` | Read a file |
| `fileWrite(path, content)` | Write a file |
| `listTools()` | List available MCP tools |
| `requestApproval(command, reason)` | Request human approval |
| `close()` | Close the connection |

### Response Types

```typescript
interface ExecResult {
  output: string;
  exitCode: number;
  durationMs: number;
}

interface PolicyDecision {
  allowed: boolean;
  reason: string;
  matchedRule?: string;
}

interface FileContent {
  path: string;
  content: string;
  size: number;
}

interface ToolCapability {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
}
```

## Error Handling

```typescript
import { McpError, McpTimeoutError, McpProtocolError, McpTransportError } from "ravenfabric";

try {
  await client.exec("dangerous-command");
} catch (err) {
  if (err instanceof McpTimeoutError) {
    console.error("Command timed out");
  } else if (err instanceof McpProtocolError) {
    console.error("Policy denied:", err.message);
  }
}
```

## License

MIT
