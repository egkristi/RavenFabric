import { describe, it, expect } from "vitest";
import { StdioTransport } from "../src/transport.js";
import { McpTransportError } from "../src/error.js";

describe("StdioTransport", () => {
  it("initializes without starting", () => {
    const transport = new StdioTransport("rf-mcp-server", ["--policy", "test.yaml"]);
    expect(transport.isRunning).toBe(false);
  });

  it("throws when sending without starting", async () => {
    const transport = new StdioTransport("rf-mcp-server");
    await expect(transport.send("test", {})).rejects.toThrow(McpTransportError);
    await expect(transport.send("test", {})).rejects.toThrow("not started");
  });

  it("close is idempotent", () => {
    const transport = new StdioTransport("rf-mcp-server");
    transport.close();
    transport.close();
    expect(transport.isRunning).toBe(false);
  });

  it("accepts environment variables", () => {
    const transport = new StdioTransport("rf-mcp-server", [], { RUST_LOG: "debug" });
    expect(transport.isRunning).toBe(false);
  });
});
