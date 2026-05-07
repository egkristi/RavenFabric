/** Result of a command execution. */
export interface ExecResult {
  output: string;
  exitCode: number;
  durationMs: number;
}

/** Result of a policy query. */
export interface PolicyDecision {
  allowed: boolean;
  reason: string;
  matchedRule?: string;
}

/** Content of a file read operation. */
export interface FileContent {
  path: string;
  content: string;
  size: number;
}

/** Description of an available MCP tool. */
export interface ToolCapability {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
}

/** JSON-RPC 2.0 request structure. */
export interface JsonRpcRequest {
  jsonrpc: "2.0";
  method: string;
  params: Record<string, unknown>;
  id: number;
}

/** JSON-RPC 2.0 response structure. */
export interface JsonRpcResponse {
  jsonrpc: "2.0";
  id: number;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}
