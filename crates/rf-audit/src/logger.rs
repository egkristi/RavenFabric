use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::types::AuditEntry;

/// Trait for audit loggers.
pub trait AuditLogger: Send + Sync {
    fn log(&self, entry: AuditEntry);
}

/// File-based JSON-lines audit logger.
pub struct FileAuditLogger {
    file: Mutex<std::fs::File>,
}

impl FileAuditLogger {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }
}

impl AuditLogger for FileAuditLogger {
    fn log(&self, entry: AuditEntry) {
        if let Ok(json) = serde_json::to_string(&entry) {
            if let Ok(mut file) = self.file.lock() {
                let _ = writeln!(file, "{}", json);
            }
        }
    }
}
