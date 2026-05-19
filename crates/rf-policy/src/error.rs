/// Errors from policy loading and validation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PolicyError {
    #[error("I/O error reading policy file: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("invalid regex pattern '{pattern}': {source}")]
    InvalidRegex {
        pattern: String,
        source: regex::Error,
    },
    #[error("invalid CIDR '{cidr}': {reason}")]
    InvalidCidr { cidr: String, reason: String },
    #[error("invalid port specification: {spec}")]
    InvalidPortSpec { spec: String },
    #[error("invalid network rule: {reason}")]
    InvalidNetworkRule { reason: String },
    #[error("policy validation error: {0}")]
    Validation(String),
}
