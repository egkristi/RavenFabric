import { describe, it, expect } from "vitest";
import {
  McpError,
  McpTransportError,
  McpTimeoutError,
  McpProtocolError,
} from "../src/error.js";

describe("Error types", () => {
  it("McpTransportError extends McpError", () => {
    const err = new McpTransportError("pipe broken");
    expect(err).toBeInstanceOf(McpError);
    expect(err).toBeInstanceOf(McpTransportError);
    expect(err.name).toBe("McpTransportError");
    expect(err.message).toBe("pipe broken");
  });

  it("McpTimeoutError extends McpError", () => {
    const err = new McpTimeoutError("timed out after 30s");
    expect(err).toBeInstanceOf(McpError);
    expect(err.name).toBe("McpTimeoutError");
  });

  it("McpProtocolError extends McpError", () => {
    const err = new McpProtocolError("invalid response");
    expect(err).toBeInstanceOf(McpError);
    expect(err.name).toBe("McpProtocolError");
  });
});
