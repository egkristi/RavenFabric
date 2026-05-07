/** Error types for RavenFabric MCP client. */

export class McpError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "McpError";
  }
}

export class McpTransportError extends McpError {
  constructor(message: string) {
    super(message);
    this.name = "McpTransportError";
  }
}

export class McpTimeoutError extends McpError {
  constructor(message: string) {
    super(message);
    this.name = "McpTimeoutError";
  }
}

export class McpProtocolError extends McpError {
  constructor(message: string) {
    super(message);
    this.name = "McpProtocolError";
  }
}
