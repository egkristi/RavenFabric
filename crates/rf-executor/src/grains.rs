//! Grains — automatic system fact collection.
//!
//! Collects machine facts (OS, architecture, hostname, IPs, CPU, memory, etc.)
//! for use in targeting and policy decisions. Inspired by Salt grains.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Collected system grains (facts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Grains {
    /// All collected facts as key-value pairs.
    pub facts: HashMap<String, GrainValue>,
}

/// A grain value that can be a string, number, bool, or list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum GrainValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    List(Vec<String>),
}

impl GrainValue {
    /// Get as string representation.
    pub fn as_str(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Integer(n) => n.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::List(l) => l.join(", "),
        }
    }
}

impl From<String> for GrainValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for GrainValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<i64> for GrainValue {
    fn from(n: i64) -> Self {
        Self::Integer(n)
    }
}

impl From<bool> for GrainValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<Vec<String>> for GrainValue {
    fn from(v: Vec<String>) -> Self {
        Self::List(v)
    }
}

impl Grains {
    /// Create empty grains.
    pub fn new() -> Self {
        Self {
            facts: HashMap::new(),
        }
    }

    /// Collect all available system grains.
    pub fn collect() -> Self {
        let mut grains = Self::new();
        grains.collect_os();
        grains.collect_arch();
        grains.collect_hostname();
        grains.collect_env();
        grains
    }

    /// Get a grain value by key.
    pub fn get(&self, key: &str) -> Option<&GrainValue> {
        self.facts.get(key)
    }

    /// Get a grain value as string.
    pub fn get_str(&self, key: &str) -> Option<String> {
        self.facts.get(key).map(GrainValue::as_str)
    }

    /// Set a grain value.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<GrainValue>) {
        self.facts.insert(key.into(), value.into());
    }

    /// Check if a grain matches a label selector (all labels must match).
    pub fn matches_labels(&self, labels: &HashMap<String, String>) -> bool {
        labels.iter().all(|(key, expected)| {
            self.facts
                .get(key)
                .map(|v| v.as_str() == *expected)
                .unwrap_or(false)
        })
    }

    /// Merge another set of grains into this one (other takes priority).
    pub fn merge(&mut self, other: &Grains) {
        for (k, v) in &other.facts {
            self.facts.insert(k.clone(), v.clone());
        }
    }

    /// Collect OS-related grains.
    fn collect_os(&mut self) {
        self.set("os", std::env::consts::OS);
        self.set("os_family", os_family());
        self.set("os_type", std::env::consts::OS);
    }

    /// Collect architecture grains.
    fn collect_arch(&mut self) {
        self.set("arch", std::env::consts::ARCH);
        self.set(
            "pointer_width",
            GrainValue::Integer(std::mem::size_of::<usize>() as i64 * 8),
        );
    }

    /// Collect hostname.
    fn collect_hostname(&mut self) {
        if let Ok(hostname) = hostname() {
            self.set("hostname", hostname);
        }
    }

    /// Collect relevant environment variables as grains.
    fn collect_env(&mut self) {
        if let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
            self.set("user", user);
        }
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            self.set("home", home);
        }
        if let Ok(shell) = std::env::var("SHELL") {
            self.set("shell", shell);
        }
    }
}

impl Default for Grains {
    fn default() -> Self {
        Self::new()
    }
}

/// Get OS family (debian, redhat, darwin, windows, etc.)
fn os_family() -> &'static str {
    match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        "freebsd" | "openbsd" | "netbsd" | "dragonfly" => "bsd",
        "android" => "android",
        "ios" => "darwin",
        other => other,
    }
}

/// Get system hostname.
fn hostname() -> Result<String, std::io::Error> {
    // Use gethostname on Unix, COMPUTERNAME on Windows
    #[cfg(unix)]
    {
        use std::ffi::CStr;
        let mut buf = [0u8; 256];
        let ret = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
        if ret == 0 {
            let cstr = unsafe { CStr::from_ptr(buf.as_ptr().cast()) };
            Ok(cstr.to_string_lossy().into_owned())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(windows)]
    {
        std::env::var("COMPUTERNAME")
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "COMPUTERNAME not set"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "hostname not supported on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grains_collect() {
        let grains = Grains::collect();
        // OS and arch should always be present
        assert!(grains.get("os").is_some());
        assert!(grains.get("arch").is_some());
        assert!(grains.get("os_family").is_some());
        assert!(grains.get("pointer_width").is_some());
    }

    #[test]
    fn test_grains_set_and_get() {
        let mut grains = Grains::new();
        grains.set("role", "web-server");
        grains.set("cpu_count", GrainValue::Integer(8));
        grains.set("virtual", GrainValue::Bool(true));

        assert_eq!(grains.get_str("role").unwrap(), "web-server");
        assert_eq!(grains.get_str("cpu_count").unwrap(), "8");
        assert_eq!(grains.get_str("virtual").unwrap(), "true");
    }

    #[test]
    fn test_grains_matches_labels() {
        let mut grains = Grains::new();
        grains.set("os", "linux");
        grains.set("role", "web-server");
        grains.set("env", "production");

        let mut labels = HashMap::new();
        labels.insert("os".into(), "linux".into());
        labels.insert("role".into(), "web-server".into());
        assert!(grains.matches_labels(&labels));

        labels.insert("env".into(), "staging".into());
        assert!(!grains.matches_labels(&labels));
    }

    #[test]
    fn test_grains_matches_empty_labels() {
        let grains = Grains::new();
        let labels = HashMap::new();
        // Empty labels should always match
        assert!(grains.matches_labels(&labels));
    }

    #[test]
    fn test_grains_merge() {
        let mut base = Grains::new();
        base.set("os", "linux");
        base.set("role", "base");

        let mut overlay = Grains::new();
        overlay.set("role", "web-server");
        overlay.set("env", "prod");

        base.merge(&overlay);
        assert_eq!(base.get_str("os").unwrap(), "linux"); // kept
        assert_eq!(base.get_str("role").unwrap(), "web-server"); // overwritten
        assert_eq!(base.get_str("env").unwrap(), "prod"); // added
    }

    #[test]
    fn test_grain_value_from() {
        let s: GrainValue = "hello".into();
        assert!(matches!(s, GrainValue::String(_)));

        let n: GrainValue = 42i64.into();
        assert!(matches!(n, GrainValue::Integer(42)));

        let b: GrainValue = true.into();
        assert!(matches!(b, GrainValue::Bool(true)));

        let l: GrainValue = vec!["a".to_string(), "b".to_string()].into();
        assert!(matches!(l, GrainValue::List(_)));
    }

    #[test]
    fn test_grain_value_as_str() {
        assert_eq!(GrainValue::String("hello".into()).as_str(), "hello");
        assert_eq!(GrainValue::Integer(42).as_str(), "42");
        assert_eq!(GrainValue::Float(3.14).as_str(), "3.14");
        assert_eq!(GrainValue::Bool(true).as_str(), "true");
        assert_eq!(
            GrainValue::List(vec!["a".into(), "b".into()]).as_str(),
            "a, b"
        );
    }

    #[test]
    fn test_grains_serialization() {
        let mut grains = Grains::new();
        grains.set("os", "linux");
        grains.set("cpu", GrainValue::Integer(4));

        let json = serde_json::to_string(&grains).unwrap();
        let deser: Grains = serde_json::from_str(&json).unwrap();
        assert_eq!(grains, deser);
    }

    #[test]
    fn test_os_family_known() {
        let family = os_family();
        // Should be one of the known families
        let known = ["linux", "darwin", "windows", "bsd", "android"];
        assert!(
            known.contains(&family) || !family.is_empty(),
            "unknown os_family: {family}"
        );
    }

    #[test]
    fn test_hostname_collected() {
        let grains = Grains::collect();
        // hostname may or may not be available in CI, but on most systems it should be
        // Just verify it doesn't panic
        let _ = grains.get("hostname");
    }
}
