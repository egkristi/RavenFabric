//! External secret manager backends.
//!
//! Implements [`SecretBackend`] for HashiCorp Vault, AWS Secrets Manager,
//! Azure Key Vault, GCP Secret Manager, and a generic HTTP backend.
//!
//! # Feature flag
//!
//! This module is compiled only when the `secret-backends` Cargo feature is enabled.
//! It is on by default.
//!
//! # Security considerations
//!
//! - Backend credentials (tokens, keys) are stored in the local [`SecretStore`] rather
//!   than in plaintext config. Callers are responsible for providing credentials that
//!   have been pre-sealed.
//! - All secret values returned by backends are treated as sensitive: they are never
//!   written to trace logs.
//! - TLS certificate validation is enforced for all backends (no `danger_accept_invalid_certs`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, warn};

// ── Error type ───────────────────────────────────────────────────────────────

/// Error type returned by all [`SecretBackend`] implementations.
#[derive(Debug, Error)]
pub enum SecretBackendError {
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Secret not found at path '{0}'")]
    NotFound(String),
    #[error("Operation not supported by this backend: {0}")]
    NotSupported(String),
    #[error("JSON parse error: {0}")]
    Parse(String),
    #[error("Backend configuration error: {0}")]
    Config(String),
    #[error("Token refresh failed: {0}")]
    TokenRefresh(String),
}

// ── Trait ────────────────────────────────────────────────────────────────────

/// Trait for external secret backends.
///
/// Implementations must be [`Send`] + [`Sync`] so they can be shared across tasks.
#[async_trait]
pub trait SecretBackend: Send + Sync + std::fmt::Debug {
    /// Fetch a secret by path (backend-specific path syntax).
    async fn fetch(&self, path: &str) -> Result<String, SecretBackendError>;

    /// Write a new secret value (optional — backends that do not support write
    /// return [`SecretBackendError::NotSupported`]).
    async fn write(&self, path: &str, value: &str) -> Result<(), SecretBackendError> {
        let _ = (path, value);
        Err(SecretBackendError::NotSupported(
            "write not supported for this backend".into(),
        ))
    }

    /// Human-readable identifier for audit logging.
    fn backend_type(&self) -> &str;
}

// ─────────────────────────────────────────────────────────────────────────────
// HashiCorp Vault
// ─────────────────────────────────────────────────────────────────────────────

/// Authentication method for Vault.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VaultAuth {
    /// Static token authentication (simplest, but token does not auto-renew).
    Token { token: String },
    /// AppRole authentication — role_id + secret_id exchanged for a short-lived token.
    AppRole {
        role_id: String,
        secret_id: String,
        /// Vault mount path (default: `approle`).
        #[serde(default = "default_approle_mount")]
        mount: String,
    },
}

fn default_approle_mount() -> String {
    "approle".to_string()
}

/// Parsed configuration for [`VaultBackend`].
#[derive(Debug, Clone, Deserialize)]
pub struct VaultConfig {
    /// Vault server address, e.g. `https://vault.example.com:8200`.
    pub addr: String,
    /// Authentication method.
    pub auth: VaultAuth,
    /// KV mount path (default: `secret`).
    #[serde(default = "default_kv_mount")]
    pub kv_mount: String,
    /// KV secrets engine version (1 or 2, default: 2).
    #[serde(default = "default_kv_version")]
    pub kv_version: u8,
}

fn default_kv_mount() -> String {
    "secret".to_string()
}
fn default_kv_version() -> u8 {
    2
}

/// Cached Vault token (token value + expiry time).
#[derive(Debug)]
struct TokenCache {
    token: String,
    acquired_at: Instant,
    ttl: Duration,
}

impl TokenCache {
    fn is_valid(&self) -> bool {
        // Refresh 60 s before expiry.
        self.acquired_at.elapsed() + Duration::from_secs(60) < self.ttl
    }
}

/// HashiCorp Vault secret backend.
///
/// Supports KV v1 and KV v2 engines with Token or AppRole authentication.
#[derive(Debug)]
pub struct VaultBackend {
    config: VaultConfig,
    client: reqwest::Client,
    token_cache: Arc<Mutex<Option<TokenCache>>>,
}

impl VaultBackend {
    /// Create a new [`VaultBackend`] from the given configuration.
    pub fn new(config: VaultConfig) -> Result<Self, SecretBackendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| SecretBackendError::Config(e.to_string()))?;
        Ok(Self {
            config,
            client,
            token_cache: Arc::new(Mutex::new(None)),
        })
    }

    /// Obtain a valid Vault token (using cache if possible).
    async fn ensure_token(&self) -> Result<String, SecretBackendError> {
        let mut cache = self.token_cache.lock().await;
        if let Some(ref c) = *cache {
            if c.is_valid() {
                return Ok(c.token.clone());
            }
        }
        // Cache miss or expired — (re)authenticate.
        let token = match &self.config.auth {
            VaultAuth::Token { token } => {
                // Static tokens don't expire via this path.
                *cache = Some(TokenCache {
                    token: token.clone(),
                    acquired_at: Instant::now(),
                    ttl: Duration::from_secs(3600 * 24 * 365), // effectively never
                });
                return Ok(token.clone());
            }
            VaultAuth::AppRole {
                role_id,
                secret_id,
                mount,
            } => {
                let url = format!("{}/v1/auth/{mount}/login", self.config.addr);
                let body = serde_json::json!({
                    "role_id": role_id,
                    "secret_id": secret_id,
                });
                let resp = self
                    .client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| SecretBackendError::Auth(e.to_string()))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return Err(SecretBackendError::Auth(format!(
                        "AppRole login failed ({status}): {text}"
                    )));
                }

                let json: Value = resp
                    .json()
                    .await
                    .map_err(|e| SecretBackendError::Parse(e.to_string()))?;
                let token = json["auth"]["client_token"]
                    .as_str()
                    .ok_or_else(|| SecretBackendError::Auth("missing client_token".into()))?
                    .to_string();
                let lease_secs = json["auth"]["lease_duration"].as_u64().unwrap_or(3600);
                *cache = Some(TokenCache {
                    token: token.clone(),
                    acquired_at: Instant::now(),
                    ttl: Duration::from_secs(lease_secs),
                });
                token
            }
        };
        Ok(token)
    }

    /// Build the KV read URL for the given path.
    fn kv_read_url(&self, path: &str) -> String {
        if self.config.kv_version == 2 {
            format!(
                "{}/v1/{}/data/{}",
                self.config.addr, self.config.kv_mount, path
            )
        } else {
            format!("{}/v1/{}/{}", self.config.addr, self.config.kv_mount, path)
        }
    }

    /// Build the KV write URL for the given path.
    fn kv_write_url(&self, path: &str) -> String {
        if self.config.kv_version == 2 {
            format!(
                "{}/v1/{}/data/{}",
                self.config.addr, self.config.kv_mount, path
            )
        } else {
            format!("{}/v1/{}/{}", self.config.addr, self.config.kv_mount, path)
        }
    }

    /// Extract the secret value from a Vault KV response.
    fn extract_value(json: &Value, path: &str) -> Result<String, SecretBackendError> {
        // KV v2 wraps the data: { "data": { "data": { "key": "value" } } }
        // KV v1: { "data": { "key": "value" } }
        // We try to find the "value" key, then fall back to the first string field.
        let data = json
            .get("data")
            .and_then(|d| d.get("data"))
            .or_else(|| json.get("data"));
        if let Some(data) = data {
            if let Some(v) = data.get("value").and_then(|v| v.as_str()) {
                return Ok(v.to_string());
            }
            // Return the first string value found.
            if let Some(obj) = data.as_object() {
                for (_, v) in obj {
                    if let Some(s) = v.as_str() {
                        return Ok(s.to_string());
                    }
                }
            }
        }
        Err(SecretBackendError::NotFound(path.to_string()))
    }
}

#[async_trait]
impl SecretBackend for VaultBackend {
    async fn fetch(&self, path: &str) -> Result<String, SecretBackendError> {
        let token = self.ensure_token().await?;
        let url = self.kv_read_url(path);
        debug!(path, "fetching secret from Vault");

        let resp = self
            .client
            .get(&url)
            .header("X-Vault-Token", &token)
            .send()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if resp.status().as_u16() == 404 {
            return Err(SecretBackendError::NotFound(path.to_string()));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Http(format!(
                "Vault GET {path} failed ({status}): {text}"
            )));
        }

        let json: Value = resp
            .json()
            .await
            .map_err(|e| SecretBackendError::Parse(e.to_string()))?;
        Self::extract_value(&json, path)
    }

    async fn write(&self, path: &str, value: &str) -> Result<(), SecretBackendError> {
        let token = self.ensure_token().await?;
        let url = self.kv_write_url(path);

        let body = if self.config.kv_version == 2 {
            serde_json::json!({ "data": { "value": value } })
        } else {
            serde_json::json!({ "value": value })
        };

        let resp = self
            .client
            .post(&url)
            .header("X-Vault-Token", &token)
            .json(&body)
            .send()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Http(format!(
                "Vault PUT {path} failed ({status}): {text}"
            )));
        }
        Ok(())
    }

    fn backend_type(&self) -> &str {
        "vault"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AWS Secrets Manager
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed configuration for [`AwsSecretsManagerBackend`].
#[derive(Debug, Clone, Deserialize)]
pub struct AwsConfig {
    /// AWS region, e.g. `us-east-1`.
    pub region: String,
    /// AWS Access Key ID.
    pub access_key_id: String,
    /// AWS Secret Access Key.
    pub secret_access_key: String,
    /// Optional session token (for temporary credentials from STS/IMDSv2).
    pub session_token: Option<String>,
}

/// AWS Secrets Manager backend.
///
/// Uses AWS Signature Version 4 (SigV4) for authentication. Requires static
/// access key credentials or temporary STS credentials.
#[derive(Debug)]
pub struct AwsSecretsManagerBackend {
    config: AwsConfig,
    client: reqwest::Client,
}

impl AwsSecretsManagerBackend {
    /// Create a new [`AwsSecretsManagerBackend`].
    pub fn new(config: AwsConfig) -> Result<Self, SecretBackendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| SecretBackendError::Config(e.to_string()))?;
        Ok(Self { config, client })
    }

    /// Sign and send an AWS Secrets Manager API request (SigV4).
    async fn call_api(&self, target: &str, body_json: &Value) -> Result<Value, SecretBackendError> {
        let endpoint = format!(
            "https://secretsmanager.{}.amazonaws.com/",
            self.config.region
        );
        let body_bytes =
            serde_json::to_vec(body_json).map_err(|e| SecretBackendError::Parse(e.to_string()))?;

        // Build SigV4 authorization.
        let now = chrono::Utc::now();
        let date_time = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let service = "secretsmanager";
        let host = format!("secretsmanager.{}.amazonaws.com", self.config.region);

        let body_hash = sha256_hex(&body_bytes);
        let canonical_headers = format!(
            "content-type:application/x-amz-json-1.1\nhost:{host}\nx-amz-date:{date_time}\nx-amz-target:{target}\n"
        );
        let signed_headers = "content-type;host;x-amz-date;x-amz-target";

        // Add session token header to canonical + signed headers if present.
        let (canonical_headers, signed_headers) = if let Some(ref token) = self.config.session_token
        {
            let ch = format!("{canonical_headers}x-amz-security-token:{token}\n");
            let sh = format!("{signed_headers};x-amz-security-token");
            (ch, sh)
        } else {
            (canonical_headers, signed_headers.to_string())
        };

        let canonical_request =
            format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{body_hash}");
        let credential_scope = format!("{date}/{}/{service}/aws4_request", self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{date_time}\n{credential_scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );

        let signing_key = derive_signing_key(
            &self.config.secret_access_key,
            &date,
            &self.config.region,
            service,
        );
        let signature = hmac_sha256_hex(&signing_key, string_to_sign.as_bytes());
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.config.access_key_id
        );

        let mut req = self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/x-amz-json-1.1")
            .header("X-Amz-Date", &date_time)
            .header("X-Amz-Target", target)
            .header("Authorization", authorization)
            .body(body_bytes);

        if let Some(ref token) = self.config.session_token {
            req = req.header("X-Amz-Security-Token", token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Http(format!(
                "AWS API {target} failed ({status}): {text}"
            )));
        }

        resp.json()
            .await
            .map_err(|e| SecretBackendError::Parse(e.to_string()))
    }
}

#[async_trait]
impl SecretBackend for AwsSecretsManagerBackend {
    async fn fetch(&self, path: &str) -> Result<String, SecretBackendError> {
        debug!(path, "fetching secret from AWS Secrets Manager");
        let body = serde_json::json!({ "SecretId": path });
        let json = self
            .call_api("secretsmanager.GetSecretValue", &body)
            .await
            .map_err(|e| {
                if e.to_string().contains("ResourceNotFoundException") {
                    SecretBackendError::NotFound(path.to_string())
                } else {
                    e
                }
            })?;

        // Response has "SecretString" or "SecretBinary".
        if let Some(s) = json["SecretString"].as_str() {
            return Ok(s.to_string());
        }
        Err(SecretBackendError::NotFound(format!(
            "{path} (binary secrets not supported)"
        )))
    }

    async fn write(&self, path: &str, value: &str) -> Result<(), SecretBackendError> {
        debug!(path, "writing secret to AWS Secrets Manager");
        let body = serde_json::json!({
            "SecretId": path,
            "SecretString": value,
        });
        self.call_api("secretsmanager.PutSecretValue", &body)
            .await?;
        Ok(())
    }

    fn backend_type(&self) -> &str {
        "aws-secrets-manager"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Azure Key Vault
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed configuration for [`AzureKeyVaultBackend`].
#[derive(Debug, Clone, Deserialize)]
pub struct AzureConfig {
    /// Key Vault URL, e.g. `https://myvault.vault.azure.net`.
    pub vault_url: String,
    /// Azure AD tenant ID.
    pub tenant_id: String,
    /// Service principal client ID.
    pub client_id: String,
    /// Service principal client secret.
    pub client_secret: String,
}

/// Azure Key Vault secret backend.
///
/// Uses client credentials flow (service principal) to obtain an OAuth 2.0
/// access token, then queries the Key Vault REST API.
#[derive(Debug)]
pub struct AzureKeyVaultBackend {
    config: AzureConfig,
    client: reqwest::Client,
    token_cache: Arc<Mutex<Option<TokenCache>>>,
}

impl AzureKeyVaultBackend {
    /// Create a new [`AzureKeyVaultBackend`].
    pub fn new(config: AzureConfig) -> Result<Self, SecretBackendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| SecretBackendError::Config(e.to_string()))?;
        Ok(Self {
            config,
            client,
            token_cache: Arc::new(Mutex::new(None)),
        })
    }

    /// Obtain a valid Azure AD access token.
    async fn ensure_token(&self) -> Result<String, SecretBackendError> {
        let mut cache = self.token_cache.lock().await;
        if let Some(ref c) = *cache {
            if c.is_valid() {
                return Ok(c.token.clone());
            }
        }

        let url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.config.tenant_id
        );
        let params = [
            ("grant_type", "client_credentials"),
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("scope", "https://vault.azure.net/.default"),
        ];

        let resp = self
            .client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| SecretBackendError::Auth(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Auth(format!(
                "Azure token request failed ({status}): {text}"
            )));
        }

        let json: Value = resp
            .json()
            .await
            .map_err(|e| SecretBackendError::Parse(e.to_string()))?;
        let token = json["access_token"]
            .as_str()
            .ok_or_else(|| SecretBackendError::Auth("missing access_token".into()))?
            .to_string();
        let expires_in = json["expires_in"].as_u64().unwrap_or(3600);
        *cache = Some(TokenCache {
            token: token.clone(),
            acquired_at: Instant::now(),
            ttl: Duration::from_secs(expires_in),
        });
        Ok(token)
    }
}

#[async_trait]
impl SecretBackend for AzureKeyVaultBackend {
    async fn fetch(&self, path: &str) -> Result<String, SecretBackendError> {
        let token = self.ensure_token().await?;
        // path is the secret name; Key Vault uses /secrets/{name}/versions/{version}
        // omitting version returns the latest.
        let url = format!("{}/secrets/{}?api-version=7.4", self.config.vault_url, path);
        debug!(path, "fetching secret from Azure Key Vault");

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if resp.status().as_u16() == 404 {
            return Err(SecretBackendError::NotFound(path.to_string()));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Http(format!(
                "Azure Key Vault GET {path} failed ({status}): {text}"
            )));
        }

        let json: Value = resp
            .json()
            .await
            .map_err(|e| SecretBackendError::Parse(e.to_string()))?;
        json["value"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| SecretBackendError::NotFound(path.to_string()))
    }

    async fn write(&self, path: &str, value: &str) -> Result<(), SecretBackendError> {
        let token = self.ensure_token().await?;
        let url = format!("{}/secrets/{}?api-version=7.4", self.config.vault_url, path);
        let body = serde_json::json!({ "value": value });

        let resp = self
            .client
            .put(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Http(format!(
                "Azure Key Vault PUT {path} failed ({status}): {text}"
            )));
        }
        Ok(())
    }

    fn backend_type(&self) -> &str {
        "azure-key-vault"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GCP Secret Manager
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed configuration for [`GcpSecretManagerBackend`].
#[derive(Debug, Clone, Deserialize)]
pub struct GcpConfig {
    /// GCP project ID.
    pub project_id: String,
    /// OAuth 2.0 access token (from `gcloud auth print-access-token` or Workload Identity).
    pub access_token: String,
}

/// GCP Secret Manager backend.
///
/// Uses a pre-obtained OAuth 2.0 access token. For production use, rotate the
/// token via the Workload Identity Federation or a service account key.
#[derive(Debug)]
pub struct GcpSecretManagerBackend {
    config: GcpConfig,
    client: reqwest::Client,
}

impl GcpSecretManagerBackend {
    /// Create a new [`GcpSecretManagerBackend`].
    pub fn new(config: GcpConfig) -> Result<Self, SecretBackendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| SecretBackendError::Config(e.to_string()))?;
        Ok(Self { config, client })
    }
}

#[async_trait]
impl SecretBackend for GcpSecretManagerBackend {
    async fn fetch(&self, path: &str) -> Result<String, SecretBackendError> {
        // path can be "my-secret" (uses "latest" version) or "my-secret/versions/3".
        let (secret_name, version) = if path.contains("/versions/") {
            let parts: Vec<&str> = path.splitn(2, "/versions/").collect();
            (parts[0], parts[1].to_string())
        } else {
            (path, "latest".to_string())
        };

        let url = format!(
            "https://secretmanager.googleapis.com/v1/projects/{}/secrets/{}/versions/{}:access",
            self.config.project_id, secret_name, version
        );
        debug!(path, "fetching secret from GCP Secret Manager");

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.config.access_token)
            .send()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if resp.status().as_u16() == 404 {
            return Err(SecretBackendError::NotFound(path.to_string()));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Http(format!(
                "GCP Secret Manager GET {path} failed ({status}): {text}"
            )));
        }

        let json: Value = resp
            .json()
            .await
            .map_err(|e| SecretBackendError::Parse(e.to_string()))?;

        // GCP returns: { "payload": { "data": "<base64-encoded value>" } }
        let b64 = json["payload"]["data"]
            .as_str()
            .ok_or_else(|| SecretBackendError::NotFound(path.to_string()))?;
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| SecretBackendError::Parse(format!("base64 decode: {e}")))?;
        String::from_utf8(decoded)
            .map_err(|e| SecretBackendError::Parse(format!("UTF-8 decode: {e}")))
    }

    fn backend_type(&self) -> &str {
        "gcp-secret-manager"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Generic HTTP backend
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed configuration for [`GenericHttpBackend`].
#[derive(Debug, Clone, Deserialize)]
pub struct GenericHttpConfig {
    /// URL template with `{path}` placeholder, e.g.
    /// `https://secrets.internal.com/v1/{path}`.
    pub url_template: String,
    /// HTTP method for read requests (default: `GET`).
    #[serde(default = "default_method")]
    pub method: String,
    /// Static request headers (e.g. `Authorization: Bearer mytoken`).
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Optional dot-path into the JSON response to extract the value,
    /// e.g. `data.value`. If absent, the entire response body is used.
    pub json_path: Option<String>,
    /// URL template for write requests. If absent, writes are not supported.
    pub write_url_template: Option<String>,
    /// HTTP method for write requests (default: `PUT`).
    #[serde(default = "default_write_method")]
    pub write_method: String,
}

fn default_method() -> String {
    "GET".to_string()
}
fn default_write_method() -> String {
    "PUT".to_string()
}

/// Generic HTTP secret backend.
///
/// Queries any HTTP service that returns secret values as JSON (or plain text).
#[derive(Debug)]
pub struct GenericHttpBackend {
    config: GenericHttpConfig,
    client: reqwest::Client,
}

impl GenericHttpBackend {
    /// Create a new [`GenericHttpBackend`].
    pub fn new(config: GenericHttpConfig) -> Result<Self, SecretBackendError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| SecretBackendError::Config(e.to_string()))?;
        Ok(Self { config, client })
    }

    /// Expand a URL template, replacing `{path}` with the given path.
    fn expand_url(&self, template: &str, path: &str) -> String {
        template.replace("{path}", path)
    }

    /// Apply a dot-path to extract a value from a JSON object,
    /// e.g. `data.value` → `root["data"]["value"]`.
    fn extract_json_path(json: &Value, dot_path: &str) -> Option<String> {
        let mut current = json;
        for key in dot_path.split('.') {
            current = current.get(key)?;
        }
        current.as_str().map(|s| s.to_string())
    }
}

#[async_trait]
impl SecretBackend for GenericHttpBackend {
    async fn fetch(&self, path: &str) -> Result<String, SecretBackendError> {
        let url = self.expand_url(&self.config.url_template, path);
        debug!(path, url, "fetching secret from generic HTTP backend");

        let method = reqwest::Method::from_bytes(self.config.method.as_bytes())
            .unwrap_or(reqwest::Method::GET);
        let mut req = self.client.request(method, &url);
        for (k, v) in &self.config.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if resp.status().as_u16() == 404 {
            return Err(SecretBackendError::NotFound(path.to_string()));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Http(format!(
                "Generic HTTP GET {url} failed ({status}): {text}"
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if let Some(ref json_path) = self.config.json_path {
            // Parse as JSON and extract via dot-path.
            let json: Value = serde_json::from_str(&body)
                .map_err(|e| SecretBackendError::Parse(e.to_string()))?;
            Self::extract_json_path(&json, json_path)
                .ok_or_else(|| SecretBackendError::NotFound(path.to_string()))
        } else {
            Ok(body.trim().to_string())
        }
    }

    async fn write(&self, path: &str, value: &str) -> Result<(), SecretBackendError> {
        let template = self.config.write_url_template.as_deref().ok_or_else(|| {
            SecretBackendError::NotSupported("no write_url_template configured".into())
        })?;

        let url = self.expand_url(template, path);
        let method = reqwest::Method::from_bytes(self.config.write_method.as_bytes())
            .unwrap_or(reqwest::Method::PUT);
        let mut req = self.client.request(method, &url).body(value.to_string());
        for (k, v) in &self.config.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| SecretBackendError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SecretBackendError::Http(format!(
                "Generic HTTP write {url} failed ({status}): {text}"
            )));
        }
        Ok(())
    }

    fn backend_type(&self) -> &str {
        "generic-http"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Backend registry & sync
// ─────────────────────────────────────────────────────────────────────────────

/// A registered secret backend with optional sync configuration.
pub struct RegisteredBackend {
    /// The backend implementation.
    pub backend: Arc<dyn SecretBackend>,
    /// If non-zero, run a background sync every N seconds.
    pub sync_interval: Duration,
    /// Paths to sync on each interval (empty = on-demand only).
    pub sync_paths: Vec<String>,
}

/// Registry of named external secret backends.
///
/// Backends are registered by name and can be queried on demand. A background
/// sync task can be started to periodically refresh secrets into the local store.
pub struct SecretBackendRegistry {
    backends: HashMap<String, RegisteredBackend>,
}

impl SecretBackendRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
        }
    }

    /// Register a backend under the given name.
    pub fn register(&mut self, name: String, backend: RegisteredBackend) {
        self.backends.insert(name, backend);
    }

    /// Fetch a secret from the named backend.
    pub async fn fetch(
        &self,
        backend_name: &str,
        path: &str,
    ) -> Result<String, SecretBackendError> {
        let rb = self.backends.get(backend_name).ok_or_else(|| {
            SecretBackendError::Config(format!("backend '{backend_name}' not registered"))
        })?;
        rb.backend.fetch(path).await
    }

    /// Write a secret to the named backend.
    pub async fn write(
        &self,
        backend_name: &str,
        path: &str,
        value: &str,
    ) -> Result<(), SecretBackendError> {
        let rb = self.backends.get(backend_name).ok_or_else(|| {
            SecretBackendError::Config(format!("backend '{backend_name}' not registered"))
        })?;
        rb.backend.write(path, value).await
    }

    /// List registered backend names.
    pub fn names(&self) -> Vec<&str> {
        self.backends.keys().map(String::as_str).collect()
    }
}

impl Default for SecretBackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sync task
// ─────────────────────────────────────────────────────────────────────────────

/// Run a background sync task that periodically fetches secrets from a backend
/// and stores them in the given local store.
///
/// Returns when the task is cancelled via the provided `shutdown` signal.
pub async fn run_sync_task(
    backend: Arc<dyn SecretBackend>,
    paths: Vec<String>,
    interval: Duration,
    on_secret: Arc<dyn Fn(String, String) + Send + Sync>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    if interval.is_zero() || paths.is_empty() {
        return;
    }
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await; // skip the immediate tick
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                for path in &paths {
                    match backend.fetch(path).await {
                        Ok(value) => on_secret(path.clone(), value),
                        Err(e) => warn!(path, error = %e, "secret sync failed"),
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Build a backend from config JSON
// ─────────────────────────────────────────────────────────────────────────────

/// Construct a [`SecretBackend`] from a `backend_type` string and JSON configuration.
///
/// # Errors
///
/// Returns [`SecretBackendError::Config`] if the type is unknown or the JSON is invalid.
pub fn build_backend(
    backend_type: &str,
    config_json: &str,
) -> Result<Arc<dyn SecretBackend>, SecretBackendError> {
    match backend_type {
        "vault" => {
            let cfg: VaultConfig = serde_json::from_str(config_json)
                .map_err(|e| SecretBackendError::Config(format!("vault config: {e}")))?;
            Ok(Arc::new(VaultBackend::new(cfg)?))
        }
        "aws-secrets-manager" | "aws" => {
            let cfg: AwsConfig = serde_json::from_str(config_json)
                .map_err(|e| SecretBackendError::Config(format!("aws config: {e}")))?;
            Ok(Arc::new(AwsSecretsManagerBackend::new(cfg)?))
        }
        "azure-key-vault" | "azure" => {
            let cfg: AzureConfig = serde_json::from_str(config_json)
                .map_err(|e| SecretBackendError::Config(format!("azure config: {e}")))?;
            Ok(Arc::new(AzureKeyVaultBackend::new(cfg)?))
        }
        "gcp-secret-manager" | "gcp" => {
            let cfg: GcpConfig = serde_json::from_str(config_json)
                .map_err(|e| SecretBackendError::Config(format!("gcp config: {e}")))?;
            Ok(Arc::new(GcpSecretManagerBackend::new(cfg)?))
        }
        "generic-http" | "http" => {
            let cfg: GenericHttpConfig = serde_json::from_str(config_json)
                .map_err(|e| SecretBackendError::Config(format!("generic-http config: {e}")))?;
            Ok(Arc::new(GenericHttpBackend::new(cfg)?))
        }
        other => Err(SecretBackendError::Config(format!(
            "unknown backend type '{other}'; valid: vault, aws-secrets-manager, azure-key-vault, gcp-secret-manager, generic-http"
        ))),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SigV4 helpers (AWS)
// ─────────────────────────────────────────────────────────────────────────────

fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(data);
    hex::encode(hash)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::KeyInit;
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).expect("HMAC accepts any key size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    hex::encode(hmac_sha256(key, data))
}

fn derive_signing_key(secret_key: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_secret = format!("AWS4{secret_key}");
    let k_date = hmac_sha256(k_secret.as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── build_backend error cases ─────────────────────────────────────────────

    #[test]
    fn unknown_backend_type_returns_error() {
        let err = build_backend("foobar", "{}").unwrap_err();
        assert!(err.to_string().contains("unknown backend type"));
    }

    #[test]
    fn invalid_json_config_returns_error() {
        let err = build_backend("vault", "not-json").unwrap_err();
        assert!(matches!(err, SecretBackendError::Config(_)));
    }

    // ── Vault config parsing ──────────────────────────────────────────────────

    #[test]
    fn vault_token_config_parses() {
        let json =
            r#"{"addr":"https://vault.example.com:8200","auth":{"type":"token","token":"s.test"}}"#;
        let cfg: VaultConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.addr, "https://vault.example.com:8200");
        assert!(matches!(cfg.auth, VaultAuth::Token { .. }));
        assert_eq!(cfg.kv_version, 2);
    }

    #[test]
    fn vault_approle_config_parses() {
        let json = r#"{"addr":"https://vault.example.com","auth":{"type":"app_role","role_id":"r","secret_id":"s"}}"#;
        let cfg: VaultConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(cfg.auth, VaultAuth::AppRole { .. }));
    }

    #[test]
    fn vault_kv1_read_url() {
        let cfg = VaultConfig {
            addr: "https://vault.example.com".into(),
            auth: VaultAuth::Token { token: "t".into() },
            kv_mount: "kv".into(),
            kv_version: 1,
        };
        let backend = VaultBackend::new(cfg).unwrap();
        assert_eq!(
            backend.kv_read_url("myapp/db"),
            "https://vault.example.com/v1/kv/myapp/db"
        );
    }

    #[test]
    fn vault_kv2_read_url() {
        let cfg = VaultConfig {
            addr: "https://vault.example.com".into(),
            auth: VaultAuth::Token { token: "t".into() },
            kv_mount: "secret".into(),
            kv_version: 2,
        };
        let backend = VaultBackend::new(cfg).unwrap();
        assert_eq!(
            backend.kv_read_url("myapp/db"),
            "https://vault.example.com/v1/secret/data/myapp/db"
        );
    }

    #[test]
    fn vault_extract_value_kv2() {
        let json = serde_json::json!({
            "data": { "data": { "value": "s3cr3t" } }
        });
        let val = VaultBackend::extract_value(&json, "path").unwrap();
        assert_eq!(val, "s3cr3t");
    }

    #[test]
    fn vault_extract_value_kv1() {
        let json = serde_json::json!({
            "data": { "value": "kv1secret" }
        });
        let val = VaultBackend::extract_value(&json, "path").unwrap();
        assert_eq!(val, "kv1secret");
    }

    #[test]
    fn vault_extract_value_missing() {
        let json = serde_json::json!({ "lease_id": "" });
        let err = VaultBackend::extract_value(&json, "mypath").unwrap_err();
        assert!(matches!(err, SecretBackendError::NotFound(_)));
    }

    // ── AWS config parsing ────────────────────────────────────────────────────

    #[test]
    fn aws_config_parses() {
        let json = r#"{"region":"us-east-1","access_key_id":"AKIA123","secret_access_key":"abc","session_token":null}"#;
        let cfg: AwsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.region, "us-east-1");
        assert!(cfg.session_token.is_none());
    }

    // ── Azure config parsing ──────────────────────────────────────────────────

    #[test]
    fn azure_config_parses() {
        let json = r#"{"vault_url":"https://kv.vault.azure.net","tenant_id":"t","client_id":"c","client_secret":"s"}"#;
        let cfg: AzureConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.vault_url, "https://kv.vault.azure.net");
    }

    // ── GCP config parsing ────────────────────────────────────────────────────

    #[test]
    fn gcp_config_parses() {
        let json = r#"{"project_id":"my-project","access_token":"ya29.abc"}"#;
        let cfg: GcpConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.project_id, "my-project");
    }

    // ── GenericHttp config parsing ────────────────────────────────────────────

    #[test]
    fn generic_http_config_parses() {
        let json =
            r#"{"url_template":"https://secrets.example.com/v1/{path}","json_path":"data.value"}"#;
        let cfg: GenericHttpConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.method, "GET");
        assert_eq!(cfg.json_path.as_deref(), Some("data.value"));
    }

    #[test]
    fn generic_http_url_expansion() {
        let cfg = GenericHttpConfig {
            url_template: "https://secrets.internal.com/v1/{path}".into(),
            method: "GET".into(),
            headers: HashMap::new(),
            json_path: None,
            write_url_template: None,
            write_method: "PUT".into(),
        };
        let backend = GenericHttpBackend::new(cfg).unwrap();
        assert_eq!(
            backend.expand_url(&backend.config.url_template, "prod/db-pass"),
            "https://secrets.internal.com/v1/prod/db-pass"
        );
    }

    #[test]
    fn generic_http_json_path_extraction() {
        let json = serde_json::json!({ "data": { "value": "hidden" } });
        let v = GenericHttpBackend::extract_json_path(&json, "data.value").unwrap();
        assert_eq!(v, "hidden");
    }

    #[test]
    fn generic_http_json_path_missing() {
        let json = serde_json::json!({ "other": "stuff" });
        assert!(GenericHttpBackend::extract_json_path(&json, "data.value").is_none());
    }

    // ── Registry ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn registry_unknown_backend_returns_error() {
        let registry = SecretBackendRegistry::new();
        let err = registry
            .fetch("nonexistent", "some/path")
            .await
            .unwrap_err();
        assert!(matches!(err, SecretBackendError::Config(_)));
    }

    // ── SigV4 helpers ─────────────────────────────────────────────────────────

    #[test]
    fn sha256_hex_known_value() {
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hmac_sha256_roundtrip() {
        let key = b"secret-key";
        let data = b"test-data";
        let result = hmac_sha256(key, data);
        // Just verify it produces a 32-byte result.
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn derive_signing_key_produces_32_bytes() {
        let key = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20231215",
            "us-east-1",
            "secretsmanager",
        );
        assert_eq!(key.len(), 32);
    }

    // ── build_backend happy paths ─────────────────────────────────────────────

    #[test]
    fn build_vault_backend() {
        let json = r#"{"addr":"https://vault.example.com","auth":{"type":"token","token":"t"}}"#;
        let b = build_backend("vault", json).unwrap();
        assert_eq!(b.backend_type(), "vault");
    }

    #[test]
    fn build_aws_backend() {
        let json = r#"{"region":"eu-west-1","access_key_id":"K","secret_access_key":"S"}"#;
        let b = build_backend("aws", json).unwrap();
        assert_eq!(b.backend_type(), "aws-secrets-manager");
    }

    #[test]
    fn build_azure_backend() {
        let json = r#"{"vault_url":"https://v.vault.azure.net","tenant_id":"t","client_id":"c","client_secret":"s"}"#;
        let b = build_backend("azure", json).unwrap();
        assert_eq!(b.backend_type(), "azure-key-vault");
    }

    #[test]
    fn build_gcp_backend() {
        let json = r#"{"project_id":"proj","access_token":"tok"}"#;
        let b = build_backend("gcp", json).unwrap();
        assert_eq!(b.backend_type(), "gcp-secret-manager");
    }

    #[test]
    fn build_generic_http_backend() {
        let json = r#"{"url_template":"https://s.example.com/{path}"}"#;
        let b = build_backend("http", json).unwrap();
        assert_eq!(b.backend_type(), "generic-http");
    }
}
