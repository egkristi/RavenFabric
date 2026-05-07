# Multi-stage build for RavenFabric
# Produces small static binaries (agent, relay, cli, mcp-server)

# --- Build stage ---
FROM rust:1.88-alpine AS builder

WORKDIR /app

RUN apk add --no-cache musl-dev gcc

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/rf-crypto/Cargo.toml crates/rf-crypto/Cargo.toml
COPY crates/rf-transport/Cargo.toml crates/rf-transport/Cargo.toml
COPY crates/rf-rpc/Cargo.toml crates/rf-rpc/Cargo.toml
COPY crates/rf-audit/Cargo.toml crates/rf-audit/Cargo.toml
COPY crates/rf-policy/Cargo.toml crates/rf-policy/Cargo.toml
COPY crates/rf-executor/Cargo.toml crates/rf-executor/Cargo.toml
COPY crates/rf-bootstrap/Cargo.toml crates/rf-bootstrap/Cargo.toml
COPY crates/rf-relay/Cargo.toml crates/rf-relay/Cargo.toml
COPY crates/rf-agent/Cargo.toml crates/rf-agent/Cargo.toml
COPY crates/rf-cli/Cargo.toml crates/rf-cli/Cargo.toml
COPY crates/rf-mcp-server/Cargo.toml crates/rf-mcp-server/Cargo.toml
COPY crates/rf-mcp-client/Cargo.toml crates/rf-mcp-client/Cargo.toml
COPY crates/rf-integration-tests/Cargo.toml crates/rf-integration-tests/Cargo.toml

# Create dummy source files for dependency caching
RUN mkdir -p crates/rf-crypto/src && echo "pub fn dummy() {}" > crates/rf-crypto/src/lib.rs && \
    mkdir -p crates/rf-transport/src && echo "pub fn dummy() {}" > crates/rf-transport/src/lib.rs && \
    mkdir -p crates/rf-rpc/src && echo "pub fn dummy() {}" > crates/rf-rpc/src/lib.rs && \
    mkdir -p crates/rf-audit/src && echo "pub fn dummy() {}" > crates/rf-audit/src/lib.rs && \
    mkdir -p crates/rf-policy/src && echo "pub fn dummy() {}" > crates/rf-policy/src/lib.rs && \
    mkdir -p crates/rf-executor/src && echo "pub fn dummy() {}" > crates/rf-executor/src/lib.rs && \
    mkdir -p crates/rf-bootstrap/src && echo "pub fn dummy() {}" > crates/rf-bootstrap/src/lib.rs && \
    mkdir -p crates/rf-relay/src && echo "fn main() {}" > crates/rf-relay/src/main.rs && \
    mkdir -p crates/rf-agent/src && echo "fn main() {}" > crates/rf-agent/src/main.rs && \
    mkdir -p crates/rf-cli/src && echo "fn main() {}" > crates/rf-cli/src/main.rs && \
    mkdir -p crates/rf-mcp-server/src && echo "fn main() {}" > crates/rf-mcp-server/src/main.rs && \
    mkdir -p crates/rf-mcp-client/src && echo "pub fn dummy() {}" > crates/rf-mcp-client/src/lib.rs && \
    mkdir -p crates/rf-integration-tests/src && echo "pub fn dummy() {}" > crates/rf-integration-tests/src/lib.rs

# Build dependencies only (cached layer)
RUN cargo build --release 2>/dev/null || true

# Copy real source code
COPY crates/ crates/

# Touch source files to invalidate cache for actual code
RUN find crates -name "*.rs" -exec touch {} +

# Build all binaries
RUN cargo build --release -p rf-agent -p rf-relay -p rf-cli -p rf-mcp-server

# --- Agent image (scratch — no OS, just the binary) ---
FROM scratch AS agent
COPY --from=builder /app/target/release/rf-agent /rf-agent
ENTRYPOINT ["/rf-agent"]

# --- Relay image ---
FROM scratch AS relay
COPY --from=builder /app/target/release/rf-relay /rf-relay
EXPOSE 9090
ENTRYPOINT ["/rf-relay"]

# --- CLI image (alpine-based for shell access) ---
FROM alpine:3.21 AS cli
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/rf /usr/bin/rf
ENTRYPOINT ["rf"]

# --- MCP Server image ---
FROM scratch AS mcp-server
COPY --from=builder /app/target/release/rf-mcp-server /rf-mcp-server
ENTRYPOINT ["/rf-mcp-server"]
