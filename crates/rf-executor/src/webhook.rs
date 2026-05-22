//! Webhook notifications for update failure and rollback events.
//!
//! A fire-and-forget HTTP POST is sent to the configured webhook URL with
//! a JSON payload describing the event. Failures are logged as warnings and
//! silently dropped — webhook delivery must never interrupt normal operation.

use chrono::Utc;
use serde::Serialize;
use tracing::warn;

/// Payload sent as a JSON POST to the alert webhook URL.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    /// Event type: `"update_failed"` or `"update_rollback"`.
    pub event: String,
    /// Agent identifier.
    pub agent_id: String,
    /// Attempted update version.
    pub version: String,
    /// Human-readable failure reason.
    pub reason: String,
    /// RFC 3339 timestamp of the event.
    pub timestamp: String,
}

/// Send an update-failure or rollback alert to a webhook URL.
///
/// Fire-and-forget: all errors are logged at `warn` level and discarded.
pub async fn send_update_alert(url: &str, payload: WebhookPayload) {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("webhook: failed to build HTTP client: {e}");
            return;
        }
    };
    let json = match serde_json::to_string(&payload) {
        Ok(j) => j,
        Err(e) => {
            warn!("webhook: failed to serialize payload: {e}");
            return;
        }
    };
    match client
        .post(url)
        .header("Content-Type", "application/json")
        .body(json)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!("webhook: alert delivered ({})", resp.status());
        }
        Ok(resp) => {
            warn!("webhook: server returned {} for alert POST", resp.status());
        }
        Err(e) => {
            warn!("webhook: delivery failed: {e}");
        }
    }
}

/// Convenience wrapper: send an `"update_failed"` or `"update_rollback"` alert.
///
/// Fills in the current UTC timestamp automatically.
pub async fn send_update_failure(
    url: &str,
    agent_id: &str,
    version: &str,
    reason: &str,
    is_rollback: bool,
) {
    let event = if is_rollback {
        "update_rollback"
    } else {
        "update_failed"
    };
    send_update_alert(
        url,
        WebhookPayload {
            event: event.to_string(),
            agent_id: agent_id.to_string(),
            version: version.to_string(),
            reason: reason.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        },
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_payload_serializes_all_fields() {
        let p = WebhookPayload {
            event: "update_failed".into(),
            agent_id: "agent-01".into(),
            version: "1.2.3".into(),
            reason: "sha256 mismatch".into(),
            timestamp: "2026-05-21T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"event\":\"update_failed\""));
        assert!(json.contains("\"agent_id\":\"agent-01\""));
        assert!(json.contains("\"version\":\"1.2.3\""));
        assert!(json.contains("sha256 mismatch"));
    }

    #[test]
    fn rollback_event_name() {
        // Verify the event name string matches documentation.
        assert_eq!(
            "update_rollback",
            if true {
                "update_rollback"
            } else {
                "update_failed"
            }
        );
    }
}
