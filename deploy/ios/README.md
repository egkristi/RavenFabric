# iOS Build Guide

## Cross-compilation

### Prerequisites

```bash
# Add iOS targets
rustup target add aarch64-apple-ios
rustup target add aarch64-apple-ios-sim  # For simulator

# Xcode must be installed for iOS SDK
xcode-select --install
```

### Build

```bash
# Build for physical iOS devices (ARM64)
cargo build --release --target aarch64-apple-ios --bin rf-agent

# Build for iOS Simulator
cargo build --release --target aarch64-apple-ios-sim --bin rf-agent
```

### Network Extension Integration

RavenFabric on iOS runs as a Network Extension (Packet Tunnel Provider):

1. The Rust binary is compiled as a static library (`staticlib`)
2. A thin Swift wrapper calls into it via C FFI
3. Runs with proper background entitlements (no app suspension)

### Entitlements Required

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.developer.networking.networkextension</key>
    <array>
        <string>packet-tunnel-provider</string>
    </array>
    <key>com.apple.developer.networking.vpn.api</key>
    <array>
        <string>allow-vpn</string>
    </array>
</dict>
</plist>
```

### Cargo Configuration

See `.cargo/config.toml` for iOS linker settings.

### Memory & Power

- Idle: ~4-6 MB RSS
- iOS Network Extensions have generous memory limits (~50 MB)
- Reconnect on wake: agent handles suspend/resume cycle
- Uses `NEPacketTunnelProvider` lifecycle callbacks

### Distribution

- **TestFlight**: For beta testing (requires Apple Developer Program)
- **App Store**: For production (requires Apple review)
- Both require an Xcode project with the Network Extension target
