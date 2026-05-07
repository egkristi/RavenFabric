/**
 * Stdio transport for MCP communication.
 *
 * Spawns an MCP server as a child process and communicates via
 * newline-delimited JSON-RPC over stdin/stdout.
 */

import { ChildProcess, spawn } from "node:child_process";
import { createInterface, Interface } from "node:readline";
import { McpTransportError } from "./error.js";
import type { JsonRpcRequest, JsonRpcResponse } from "./types.js";

export class StdioTransport {
  private readonly command: string;
  private readonly args: string[];
  private readonly env: Record<string, string> | undefined;
  private process: ChildProcess | null = null;
  private readline: Interface | null = null;
  private requestId = 0;
  private pendingRequests = new Map<
    number,
    { resolve: (resp: JsonRpcResponse) => void; reject: (err: Error) => void }
  >();

  constructor(
    command: string,
    args: string[] = [],
    env?: Record<string, string>,
  ) {
    this.command = command;
    this.args = args;
    this.env = env;
  }

  /** Start the MCP server subprocess. */
  start(): void {
    const childEnv = this.env
      ? { ...process.env, ...this.env }
      : process.env;

    try {
      this.process = spawn(this.command, this.args, {
        stdio: ["pipe", "pipe", "pipe"],
        env: childEnv as NodeJS.ProcessEnv,
      });
    } catch (err) {
      throw new McpTransportError(
        `Failed to start MCP server: ${err instanceof Error ? err.message : err}`,
      );
    }

    this.process.on("error", (err) => {
      // Reject all pending requests
      for (const [, pending] of this.pendingRequests) {
        pending.reject(new McpTransportError(`Process error: ${err.message}`));
      }
      this.pendingRequests.clear();
    });

    this.process.on("exit", (code) => {
      for (const [, pending] of this.pendingRequests) {
        pending.reject(
          new McpTransportError(`Process exited with code ${code}`),
        );
      }
      this.pendingRequests.clear();
    });

    if (!this.process.stdout) {
      throw new McpTransportError("Failed to open stdout pipe");
    }

    this.readline = createInterface({ input: this.process.stdout });
    this.readline.on("line", (line) => {
      this.handleLine(line);
    });
  }

  /** Send a JSON-RPC request and wait for the response. */
  async send(
    method: string,
    params: Record<string, unknown>,
  ): Promise<JsonRpcResponse> {
    if (!this.process?.stdin) {
      throw new McpTransportError("Transport not started. Call start() first.");
    }

    const id = ++this.requestId;
    const request: JsonRpcRequest = {
      jsonrpc: "2.0",
      method,
      params,
      id,
    };

    const promise = new Promise<JsonRpcResponse>((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
    });

    const line = JSON.stringify(request) + "\n";
    const ok = this.process.stdin.write(line);
    if (!ok) {
      await new Promise<void>((resolve) =>
        this.process!.stdin!.once("drain", resolve),
      );
    }

    return promise;
  }

  /** Close the transport and kill the subprocess. */
  close(): void {
    if (this.readline) {
      this.readline.close();
      this.readline = null;
    }
    if (this.process) {
      this.process.kill();
      this.process = null;
    }
    for (const [, pending] of this.pendingRequests) {
      pending.reject(new McpTransportError("Transport closed"));
    }
    this.pendingRequests.clear();
  }

  /** Whether the subprocess is still running. */
  get isRunning(): boolean {
    return this.process !== null && this.process.exitCode === null;
  }

  private handleLine(line: string): void {
    let data: JsonRpcResponse;
    try {
      data = JSON.parse(line) as JsonRpcResponse;
    } catch {
      // Ignore non-JSON lines (e.g. stderr leaking to stdout)
      return;
    }

    const pending = this.pendingRequests.get(data.id);
    if (pending) {
      this.pendingRequests.delete(data.id);
      pending.resolve(data);
    }
  }
}
