/// Errors from policy loading and validation.
#[derive(Debug, thiserror::Error)]
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
    #[error("policy validation error: {0}")]
    Validation(String),
}
