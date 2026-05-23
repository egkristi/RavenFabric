//! Cross-region relay forwarding.
//!
//! Enables an agent on relay-A to reach an agent registered on relay-B by
//! having relay-A open a forwarded WebSocket connection to relay-B and bridge
//! the two encrypted streams.
//!
//! ## Security model
//!
//! - Forwarding is **disabled by default**; the relay must opt-in with
//!   `allow_forwarding = true`.
//! - An optional `forward_allowlist` restricts which peer-relay URLs are
//!   permitted.  When the list is empty and forwarding is enabled, any URL is
//!   accepted (useful in fully-trusted private meshes).
//! - The relay **never decrypts** the forwarded payload.  End-to-end NoiseXX
//!   confidentiality between agent and client is preserved across relay hops.
//! - Every forwarding event is logged with source IP, target relay URL, and
//!   a SHA-256 hash of the inner meet token.
//!
//! ## Token format
//!
//! A forwarding request is triggered when the meet token begins with the
//! ASCII prefix `FORWARD:`:
//!
//! ```text
//! FORWARD:<target_relay_url>|<inner_meet_token>
//! ```
//!
//! Example:
//! ```text
//! FORWARD:wss://eu2.relay.example.com:9090|agent-foo-secret
//! ```
//!
//! The inner `<inner_meet_token>` is the normal meet token used to pair with
//! the agent on the remote relay.

use futures_util::{SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

/// Configuration governing cross-region relay forwarding.
#[derive(Debug, Clone, Default)]
pub struct ForwardConfig {
    /// Whether forwarding is enabled at all.  Default: `false`.
    pub allow_forwarding: bool,
    /// Allowlist of target relay base URLs (e.g. `wss://eu2.relay.example.com:9090`).
    /// An empty list means *all* targets are permitted (only relevant when
    /// `allow_forwarding` is `true`).
    pub forward_allowlist: Vec<String>,
}

impl ForwardConfig {
    /// Returns `true` if forwarding to `target_url` is permitted.
    pub fn is_target_allowed(&self, target_url: &str) -> bool {
        if !self.allow_forwarding {
            return false;
        }
        if self.forward_allowlist.is_empty() {
            return true;
        }
        self.forward_allowlist
            .iter()
            .any(|allowed| allowed == target_url)
    }
}

/// Parse a forwarding meet token.
///
/// Returns `Some((target_relay_url, inner_token))` when the token matches the
/// `FORWARD:<url>|<inner>` format, otherwise `None`.
pub fn parse_forward_token(token: &str) -> Option<(&str, &str)> {
    let rest = token.strip_prefix("FORWARD:")?;
    let (target_url, inner_token) = rest.split_once('|')?;
    if target_url.is_empty() || inner_token.is_empty() {
        return None;
    }
    Some((target_url, inner_token))
}

/// Hash a meet token for audit logging (SHA-256, hex-encoded).
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}

/// Bridge an already-open local WebSocket (`local_ws`) to a remote relay at
/// `target_relay_url` by connecting as the first peer with `inner_token`.
///
/// Traffic is forwarded bidirectionally without decryption until either side
/// closes the connection.
pub async fn bridge_to_remote_relay(
    local_ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    target_relay_url: &str,
    inner_token: &str,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    info!(
        "opening cross-region forward to {} (token hash {})",
        target_relay_url,
        hash_token(inner_token)
    );

    // Connect to the peer relay.
    let (mut remote_ws, _resp) = tokio_tungstenite::connect_async(target_relay_url)
        .await
        .map_err(|e| {
            warn!(
                "failed to connect to peer relay {}: {}",
                target_relay_url, e
            );
            e
        })?;

    // Send the inner meet token to pair with the waiting agent on the remote relay.
    remote_ws
        .send(Message::Text(inner_token.to_string().into()))
        .await?;

    info!("cross-region link established to {}", target_relay_url);

    let (mut local_sink, mut local_source) = local_ws.split();
    let (mut remote_sink, mut remote_source) = remote_ws.split();

    // Bidirectional forwarding — encrypted frames pass through unchanged.
    tokio::select! {
        () = cancel.cancelled() => {
            info!("cross-region forward cancelled");
        }
        // local → remote
        _ = async {
            while let Some(msg) = local_source.next().await {
                match msg {
                    Ok(msg @ Message::Binary(_)) => {
                        if remote_sink.send(msg).await.is_err() { break; }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        } => {}
        // remote → local
        _ = async {
            while let Some(msg) = remote_source.next().await {
                match msg {
                    Ok(msg @ Message::Binary(_)) => {
                        if local_sink.send(msg).await.is_err() { break; }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        } => {}
    }

    info!("cross-region forward closed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_forward_token_valid() {
        let token = "FORWARD:wss://eu2.relay.example.com:9090|my-agent-token";
        let result = parse_forward_token(token);
        assert_eq!(
            result,
            Some(("wss://eu2.relay.example.com:9090", "my-agent-token"))
        );
    }

    #[test]
    fn test_parse_forward_token_no_prefix() {
        assert!(parse_forward_token("ordinary-token").is_none());
    }

    #[test]
    fn test_parse_forward_token_missing_separator() {
        assert!(parse_forward_token("FORWARD:wss://relay.example.com:9090").is_none());
    }

    #[test]
    fn test_parse_forward_token_empty_inner() {
        assert!(parse_forward_token("FORWARD:wss://relay.example.com:9090|").is_none());
    }

    #[test]
    fn test_parse_forward_token_empty_url() {
        assert!(parse_forward_token("FORWARD:|inner-token").is_none());
    }

    #[test]
    fn test_forward_config_deny_by_default() {
        let cfg = ForwardConfig::default();
        assert!(!cfg.is_target_allowed("wss://relay.example.com:9090"));
    }

    #[test]
    fn test_forward_config_allow_all() {
        let cfg = ForwardConfig {
            allow_forwarding: true,
            forward_allowlist: vec![],
        };
        assert!(cfg.is_target_allowed("wss://anything.example.com:9090"));
    }

    #[test]
    fn test_forward_config_allowlist() {
        let cfg = ForwardConfig {
            allow_forwarding: true,
            forward_allowlist: vec!["wss://trusted.example.com:9090".into()],
        };
        assert!(cfg.is_target_allowed("wss://trusted.example.com:9090"));
        assert!(!cfg.is_target_allowed("wss://untrusted.example.com:9090"));
    }

    #[test]
    fn test_hash_token_deterministic() {
        let h1 = hash_token("my-token");
        let h2 = hash_token("my-token");
        assert_eq!(h1, h2);
        assert_ne!(h1, hash_token("other-token"));
        // SHA-256 hex is 64 chars
        assert_eq!(h1.len(), 64);
    }
}
