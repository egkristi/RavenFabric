//! Platform-specific agent configuration types.
//!
//! Defines build targets, platform feature gates, and mobile-specific
//! considerations for cross-platform deployment.

use serde::{Deserialize, Serialize};

/// Target platform for agent compilation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// Linux x86_64 (musl static).
    LinuxAmd64,
    /// Linux aarch64 (musl static).
    LinuxArm64,
    /// Linux armv7 (Raspberry Pi 3/4/Zero 2W).
    LinuxArmv7,
    /// Linux riscv64.
    LinuxRiscv64,
    /// macOS x86_64.
    MacosAmd64,
    /// macOS aarch64 (Apple Silicon).
    MacosArm64,
    /// Windows x86_64.
    WindowsAmd64,
    /// FreeBSD x86_64.
    FreebsdAmd64,
    /// Android aarch64.
    AndroidArm64,
    /// Android armv7.
    AndroidArmv7,
    /// iOS aarch64.
    IosArm64,
    /// OpenWrt (MIPS).
    OpenWrtMips,
    /// OpenWrt (ARM).
    OpenWrtArm,
    /// WASM/WASI.
    WasmWasi,
    /// ESP32 (no_std subset).
    Esp32,
}

/// Feature profile for constrained deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureProfile {
    /// Full feature set (default for desktop/server).
    Full,
    /// Minimal: no TUN, no sysinfo, no QUIC.
    Minimal,
    /// Embedded: no_std compatible, no heap allocator.
    Embedded,
    /// Mobile: Doze-aware, battery-efficient, background-safe.
    Mobile,
}

/// Android-specific agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidConfig {
    /// Run as foreground service (required for persistent connections).
    pub foreground_service: bool,
    /// Doze-aware: use AlarmManager for reconnect in Doze mode.
    pub doze_aware: bool,
    /// Minimum reconnect interval during Doze (seconds).
    pub doze_reconnect_secs: u32,
    /// Wake lock type for execution.
    pub wake_lock: WakeLockType,
    /// Network type preference.
    pub preferred_network: AndroidNetworkType,
    /// Battery optimization exemption requested.
    pub battery_unrestricted: bool,
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            foreground_service: true,
            doze_aware: true,
            doze_reconnect_secs: 900,
            wake_lock: WakeLockType::Partial,
            preferred_network: AndroidNetworkType::Any,
            battery_unrestricted: false,
        }
    }
}

/// Android wake lock types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeLockType {
    /// Partial wake lock (CPU only).
    Partial,
    /// No wake lock (rely on foreground service).
    None,
}

/// Android network type preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidNetworkType {
    /// Any available network.
    Any,
    /// WiFi only.
    Wifi,
    /// Cellular only.
    Cellular,
    /// Unmetered networks only.
    Unmetered,
}

/// iOS-specific agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosConfig {
    /// Run as Network Extension.
    pub network_extension: bool,
    /// Background modes enabled.
    pub background_modes: Vec<IosBackgroundMode>,
    /// Push notification for wake (silent push).
    pub push_wake: bool,
    /// Maximum background execution time (seconds, iOS limit ~30s).
    pub max_background_secs: u32,
}

impl Default for IosConfig {
    fn default() -> Self {
        Self {
            network_extension: true,
            background_modes: vec![IosBackgroundMode::NetworkExtension],
            push_wake: true,
            max_background_secs: 30,
        }
    }
}

/// iOS background execution modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IosBackgroundMode {
    /// Network Extension (VPN/content filter).
    NetworkExtension,
    /// Background fetch.
    BackgroundFetch,
    /// Background processing.
    BackgroundProcessing,
    /// Remote notifications (silent push).
    RemoteNotification,
}

/// Resource constraints for embedded/constrained targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConstraints {
    /// Maximum heap memory usage (bytes). 0 = unlimited.
    pub max_memory_bytes: u64,
    /// Maximum number of async tasks.
    pub max_tasks: u16,
    /// Single-threaded runtime (for < 256KB RAM devices).
    pub single_threaded: bool,
    /// Disable crypto operations that require large stack.
    pub minimal_crypto: bool,
    /// Target binary size limit (bytes). 0 = unlimited.
    pub max_binary_size: u64,
}

impl Default for ResourceConstraints {
    fn default() -> Self {
        Self {
            max_memory_bytes: 10 * 1024 * 1024, // 10 MB
            max_tasks: 64,
            single_threaded: false,
            minimal_crypto: false,
            max_binary_size: 15 * 1024 * 1024, // 15 MB
        }
    }
}

/// Cross-compilation target triple.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileTarget {
    /// Rust target triple.
    pub triple: String,
    /// Platform enum value.
    pub platform: Platform,
    /// Feature profile.
    pub profile: FeatureProfile,
    /// Resource constraints.
    pub constraints: ResourceConstraints,
    /// Linker to use (e.g., "aarch64-linux-musl-gcc").
    pub linker: Option<String>,
    /// Extra rustflags.
    pub rustflags: Vec<String>,
}

/// Get the compile target for the current platform.
pub fn current_platform() -> Platform {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Platform::LinuxAmd64
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        Platform::LinuxArm64
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Platform::MacosAmd64
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Platform::MacosArm64
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Platform::WindowsAmd64
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        Platform::LinuxAmd64 // fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_serde() {
        let platforms = [
            Platform::LinuxAmd64,
            Platform::AndroidArm64,
            Platform::IosArm64,
            Platform::WasmWasi,
            Platform::Esp32,
        ];
        for p in &platforms {
            let json = serde_json::to_string(p).unwrap();
            let parsed: Platform = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, p);
        }
    }

    #[test]
    fn test_android_config_default() {
        let config = AndroidConfig::default();
        assert!(config.foreground_service);
        assert!(config.doze_aware);
        assert_eq!(config.doze_reconnect_secs, 900);
    }

    #[test]
    fn test_ios_config_default() {
        let config = IosConfig::default();
        assert!(config.network_extension);
        assert!(config.push_wake);
        assert_eq!(config.max_background_secs, 30);
    }

    #[test]
    fn test_resource_constraints_default() {
        let constraints = ResourceConstraints::default();
        assert_eq!(constraints.max_memory_bytes, 10 * 1024 * 1024);
        assert!(!constraints.single_threaded);
    }

    #[test]
    fn test_current_platform() {
        let p = current_platform();
        // Just verify it returns something without panicking.
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.is_empty());
    }

    #[test]
    fn test_compile_target() {
        let target = CompileTarget {
            triple: "aarch64-linux-android".into(),
            platform: Platform::AndroidArm64,
            profile: FeatureProfile::Mobile,
            constraints: ResourceConstraints::default(),
            linker: Some("aarch64-linux-android21-clang".into()),
            rustflags: vec!["-C".into(), "link-arg=-landroid".into()],
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains("android"));
        assert!(json.contains("mobile"));
    }

    #[test]
    fn test_feature_profile_serde() {
        let profiles = [
            FeatureProfile::Full,
            FeatureProfile::Minimal,
            FeatureProfile::Embedded,
            FeatureProfile::Mobile,
        ];
        for p in &profiles {
            let json = serde_json::to_string(p).unwrap();
            let parsed: FeatureProfile = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, p);
        }
    }
}
