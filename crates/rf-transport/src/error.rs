use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("no available transport driver")]
    NoDriver,

    #[error("connection failed: {0}")]
    Connection(String),

    #[error("driver {driver} unavailable: {reason}")]
    Unavailable { driver: String, reason: String },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
