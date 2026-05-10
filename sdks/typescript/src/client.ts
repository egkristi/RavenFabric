/**
 * MCP client for RavenFabric.
 *
 * Provides a type-safe async interface for communicating with a
 * RavenFabric MCP server via the Model Context Protocol.
 */

import { McpError, McpProtocolError, McpTimeoutError } from "./error.js";
import { StdioTransport } from "./transport.js";
import type {
  ExecResult,
  FileContent,
  JsonRpcResponse,
  PolicyDecision,
  ToolCapability,
} from "./types.js";

export class McpClient {
  private readonly transport: StdioTransport;
  private readonly timeout: number;
  private initialized = false;

  /**
   * Create a new MCP client.
   * @param transport - The transport to use for communication.
   * @param timeout - Request timeout in milliseconds (default: 30000).
   */
  constructor(transport: StdioTransport, timeout = 30000) {
    this.transport = transport;
    this.timeout = timeout;
  }

  /** Perform the MCP initialization handshake. */
  async initialize(): Promise<void> {
    this.transport.start();
    const resp = await this.call("initialize", {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: "ravenfabric-typescript", version: "0.1.6" },
    });
    if (resp.error) {
      throw new McpProtocolError(
        `Initialization failed: ${resp.error.message}`,
      );
    }
    // Send initialized notification (no response expected, but still send)
    await this.transport.send("notifications/initialized", {});
    this.initialized = true;
  }

  /** Execute a command on the remote agent. */
  async exec(
    command: string,
    options?: { timeout?: number; reason?: string },
  ): Promise<ExecResult> {
    this.checkInitialized();
    const params: Record<string, unknown> = { command };
    if (options?.timeout !== undefined) params.timeout = options.timeout;
    if (options?.reason !== undefined) params.reason = options.reason;

    const result = await this.callTool("exec", params);
    return {
      output: (result.output as string) ?? "",
      exitCode: (result.exit_code as number) ?? -1,
      durationMs: (result.duration_ms as number) ?? 0,
    };
  }

  /** Check whether a command would be allowed by policy. */
  async queryPolicy(command: string): Promise<PolicyDecision> {
    this.checkInitialized();
    const result = await this.callTool("query_policy", { command });
    return {
      allowed: (result.allowed as boolean) ?? false,
      reason: (result.reason as string) ?? "",
      matchedRule: result.matched_rule as string | undefined,
    };
  }

  /** Read a file from the remote agent. */
  async fileRead(path: string): Promise<FileContent> {
    this.checkInitialized();
    const result = await this.callTool("file_read", { path });
    return {
      path: (result.path as string) ?? path,
      content: (result.content as string) ?? "",
      size: (result.size as number) ?? 0,
    };
  }

  /** Write content to a file on the remote agent. */
  async fileWrite(path: string, content: string): Promise<void> {
    this.checkInitialized();
    await this.callTool("file_write", { path, content });
  }

  /** List available MCP tools on the server. */
  async listTools(): Promise<ToolCapability[]> {
    this.checkInitialized();
    const resp = await this.call("tools/list", {});
    if (resp.error) {
      throw new McpProtocolError(`listTools failed: ${resp.error.message}`);
    }
    const result = resp.result as { tools?: Array<Record<string, unknown>> };
    const tools = result?.tools ?? [];
    return tools.map((t) => ({
      name: t.name as string,
      description: (t.description as string) ?? "",
      parameters: (t.inputSchema as Record<string, unknown>) ?? {},
    }));
  }

  /** Request human approval for a command. */
  async requestApproval(command: string, reason: string): Promise<string> {
    this.checkInitialized();
    const result = await this.callTool("request_approval", {
      command,
      reason,
    });
    return (result.approval_id as string) ?? "";
  }

  /** Close the transport connection. */
  close(): void {
    this.transport.close();
    this.initialized = false;
  }

  // --- Internal helpers ---

  private checkInitialized(): void {
    if (!this.initialized) {
      throw new McpError("Client not initialized. Call initialize() first.");
    }
  }

  private async call(
    method: string,
    params: Record<string, unknown>,
  ): Promise<JsonRpcResponse> {
    const timeoutPromise = new Promise<never>((_, reject) => {
      setTimeout(
        () =>
          reject(
            new McpTimeoutError(
              `Request '${method}' timed out after ${this.timeout}ms`,
            ),
          ),
        this.timeout,
      );
    });
    return Promise.race([this.transport.send(method, params), timeoutPromise]);
  }

  private async callTool(
    toolName: string,
    args: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const resp = await this.call("tools/call", {
      name: toolName,
      arguments: args,
    });
    if (resp.error) {
      throw new McpProtocolError(
        `Tool '${toolName}' failed: ${resp.error.message}`,
      );
    }
    return this.extractToolResult(resp.result, toolName);
  }

  private extractToolResult(
    result: unknown,
    toolName: string,
  ): Record<string, unknown> {
    if (!result) {
      throw new McpProtocolError(`Empty result from tool '${toolName}'`);
    }
    const r = result as { content?: Array<{ type: string; text?: string }> };
    const content = r.content ?? [];
    for (const block of content) {
      if (block.type === "text" && block.text) {
        try {
          return JSON.parse(block.text) as Record<string, unknown>;
        } catch {
          return { output: block.text };
        }
      }
    }
    return {};
  }
}
