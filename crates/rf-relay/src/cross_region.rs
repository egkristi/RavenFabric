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
#[derive(Debug, Clone)]
pub struct ForwardConfig {
    /// Whether forwarding is enabled at all.  Default: `false`.
    pub allow_forwarding: bool,
    /// Allowlist of target relay base URLs (e.g. `wss://eu2.relay.example.com:9090`).
    /// An empty list means *all* targets are permitted (only relevant when
    /// `allow_forwarding` is `true`).
    pub forward_allowlist: Vec<String>,
    /// Enable compatibility mode for cross-platform relay connections.
    /// Adds a small yield between forwarded messages to prevent race conditions
    /// on certain platform combinations (e.g., macOS→Linux via snow-0.10.0).
    pub compat_mode: bool,
    /// Maximum number of relay hops a forwarding request may traverse before it
    /// is rejected. Prevents A→B→A amplification loops (ROADMAP F11).
    /// `0` disables the hop check (unbounded). Default: `2`.
    pub max_forward_hops: u32,
}

impl Default for ForwardConfig {
    fn default() -> Self {
        Self {
            allow_forwarding: false,
            forward_allowlist: Vec::new(),
            compat_mode: false,
            max_forward_hops: 2,
        }
    }
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

/// Parse an optional hop count from a forwarding token.
///
/// The token format `FORWARD:<url>|<hops>|<inner>` carries a decimal hop count;
/// the legacy `FORWARD:<url>|<inner>` form (without hop count) is also accepted
/// and treated as hop 0 (unlimited at the parser level — the relay enforces a
/// ceiling via `max_forward_hops`). Returns `Some((url, hops, inner))` for the
/// three-segment form and `Some((url, 0, inner))` for the legacy two-segment
/// form, or `None` if the token is not a forwarding token.
pub fn parse_forward_token_with_hops(token: &str) -> Option<(&str, u32, &str)> {
    let rest = token.strip_prefix("FORWARD:")?;
    let parts: Vec<&str> = rest.split('|').collect();
    match parts.as_slice() {
        [target_url, hops, inner] => {
            if target_url.is_empty() || inner.is_empty() {
                return None;
            }
            let hops = hops.parse::<u32>().ok()?;
            Some((target_url, hops, inner))
        }
        [target_url, inner] => {
            if target_url.is_empty() || inner.is_empty() {
                return None;
            }
            Some((target_url, 0, inner))
        }
        _ => None,
    }
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
    bridge_to_remote_relay_inner(local_ws, target_relay_url, inner_token, cancel, false).await
}

/// Internal bridge with optional compat mode for cross-platform relay issues.
///
/// Generic over the local stream type so it can bridge both plain TCP and TLS
/// streams (used by `handle_connection_inner` for the native-TLS path).
pub(crate) async fn bridge_to_remote_relay_inner<S>(
    local_ws: tokio_tungstenite::WebSocketStream<S>,
    target_relay_url: &str,
    inner_token: &str,
    cancel: tokio_util::sync::CancellationToken,
    compat_mode: bool,
) -> anyhow::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
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
                        if compat_mode {
                            tokio::task::yield_now().await;
                        }
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
                        if compat_mode {
                            tokio::task::yield_now().await;
                        }
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
    fn test_parse_forward_token_with_hops_three_segment() {
        let token = "FORWARD:wss://eu2.relay.example.com:9090|1|my-agent-token";
        assert_eq!(
            parse_forward_token_with_hops(token),
            Some(("wss://eu2.relay.example.com:9090", 1, "my-agent-token"))
        );
    }

    #[test]
    fn test_parse_forward_token_with_hops_legacy_two_segment() {
        // Legacy FORWARD:<url>|<inner> form is treated as hop 0.
        let token = "FORWARD:wss://eu2.relay.example.com:9090|my-agent-token";
        assert_eq!(
            parse_forward_token_with_hops(token),
            Some(("wss://eu2.relay.example.com:9090", 0, "my-agent-token"))
        );
    }

    #[test]
    fn test_parse_forward_token_with_hops_invalid_hop() {
        assert!(
            parse_forward_token_with_hops("FORWARD:wss://relay.example.com:9090|abc|inner")
                .is_none()
        );
    }

    #[test]
    fn test_parse_forward_token_with_hops_no_prefix() {
        assert!(parse_forward_token_with_hops("ordinary-token").is_none());
    }

    #[test]
    fn test_max_forward_hops_default_is_two() {
        assert_eq!(ForwardConfig::default().max_forward_hops, 2);
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
            compat_mode: false,
            max_forward_hops: 2,
        };
        assert!(cfg.is_target_allowed("wss://anything.example.com:9090"));
    }

    #[test]
    fn test_forward_config_allowlist() {
        let cfg = ForwardConfig {
            allow_forwarding: true,
            forward_allowlist: vec!["wss://trusted.example.com:9090".into()],
            compat_mode: false,
            max_forward_hops: 2,
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
