//! RavenFabric ingress gateway.
//!
//! Accepts inbound HTTP requests and routes them via Noise XX–authenticated
//! RPC connections to registered agent upstreams.  All routing decisions are
//! logged through the audit pipeline.

pub mod auth;
pub mod rate_limit;
pub mod router;
pub mod server;

pub use server::run_ingress;
