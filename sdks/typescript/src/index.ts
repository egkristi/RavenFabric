/**
 * RavenFabric MCP Client SDK for TypeScript/JavaScript.
 *
 * Provides a type-safe client for communicating with RavenFabric MCP servers
 * via the Model Context Protocol over stdio transport.
 *
 * @packageDocumentation
 */

export { McpClient } from "./client.js";
export { StdioTransport } from "./transport.js";
export {
  McpError,
  McpTransportError,
  McpTimeoutError,
  McpProtocolError,
} from "./error.js";
export type {
  ExecResult,
  PolicyDecision,
  FileContent,
  ToolCapability,
} from "./types.js";
