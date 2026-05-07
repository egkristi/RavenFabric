import { describe, it, expect } from "vitest";
import { McpClient } from "../src/client.js";
import { McpError, McpProtocolError } from "../src/error.js";
import { StdioTransport } from "../src/transport.js";

describe("McpClient", () => {
  it("throws when not initialized", async () => {
    const transport = new StdioTransport("rf-mcp-server");
    const client = new McpClient(transport);
    await expect(client.exec("ls")).rejects.toThrow(McpError);
    await expect(client.exec("ls")).rejects.toThrow("not initialized");
  });

  it("extractToolResult parses JSON text content", () => {
    const transport = new StdioTransport("rf-mcp-server");
    const client = new McpClient(transport);

    // Access private method via any for testing
    const result = (client as any).extractToolResult(
      {
        content: [
          {
            type: "text",
            text: JSON.stringify({ output: "hello", exit_code: 0, duration_ms: 5 }),
          },
        ],
      },
      "exec",
    );
    expect(result.output).toBe("hello");
    expect(result.exit_code).toBe(0);
  });

  it("extractToolResult handles plain text", () => {
    const transport = new StdioTransport("rf-mcp-server");
    const client = new McpClient(transport);

    const result = (client as any).extractToolResult(
      { content: [{ type: "text", text: "plain output" }] },
      "exec",
    );
    expect(result.output).toBe("plain output");
  });

  it("extractToolResult throws on empty result", () => {
    const transport = new StdioTransport("rf-mcp-server");
    const client = new McpClient(transport);

    expect(() => (client as any).extractToolResult(null, "exec")).toThrow(
      McpProtocolError,
    );
  });

  it("extractToolResult returns empty on no content", () => {
    const transport = new StdioTransport("rf-mcp-server");
    const client = new McpClient(transport);

    const result = (client as any).extractToolResult({ content: [] }, "exec");
    expect(result).toEqual({});
  });
});
