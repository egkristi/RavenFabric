# Multi-stage build for RavenFabric
# Produces small static binaries (agent, relay, cli)

# --- Build stage ---
FROM rust:1.85-alpine AS builder

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
    mkdir -p crates/rf-cli/src && echo "fn main() {}" > crates/rf-cli/src/main.rs

# Build dependencies only (cached layer)
RUN cargo build --release 2>/dev/null || true

# Copy real source code
COPY crates/ crates/

# Touch source files to invalidate cache for actual code
RUN find crates -name "*.rs" -exec touch {} +

# Build all binaries
RUN cargo build --release -p rf-agent -p rf-relay -p rf-cli

# --- Agent image (scratch — no OS, just the binary) ---
FROM scratch AS agent
COPY --from=builder /app/target/release/rf-agent /rf-agent
ENTRYPOINT ["/rf-agent"]

# --- Relay image ---
FROM scratch AS relay
COPY --from=builder /app/target/release/rf-relay /rf-relay
EXPOSE 9090
ENTRYPOINT ["/rf-relay"]
