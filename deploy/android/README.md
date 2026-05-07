# Android Build Guide

## Cross-compilation with Android NDK

### Prerequisites

```bash
# Install Android NDK (via Android Studio or standalone)
# Set NDK path
export ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/27.0.12077973

# Add Rust targets
rustup target add aarch64-linux-android
rustup target add armv7-linux-androideabi
```

### Build

```bash
# Set up cargo config for Android targets
# (see .cargo/config.toml in this directory)

# Build for ARM64 (most modern Android devices)
cargo build --release --target aarch64-linux-android --bin rf-agent

# Build for ARMv7 (older devices)
cargo build --release --target armv7-linux-androideabi --bin rf-agent
```

### Termux Installation

RavenFabric runs natively in Termux without modification:

```bash
# In Termux on Android:
pkg install rust
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build --release --bin rf-agent --bin rf
cp target/release/rf-agent ~/.local/bin/
cp target/release/rf ~/.local/bin/

# Run agent
rf-agent --config ~/.config/ravenfabric/raven.toml
```

### F-Droid / APK Distribution

For distributing as an Android APK (foreground service):

1. The agent runs as a native binary inside the APK's lib directory
2. A thin Java/Kotlin wrapper starts it as a foreground service
3. No JNI required — communicates via stdin/stdout or localhost socket

See `AndroidManifest.xml` for the minimal service wrapper.

### Binary Size (Release, stripped)

| Target | Binary | Size |
|--------|--------|------|
| aarch64-linux-android | rf-agent | ~8 MB |
| armv7-linux-androideabi | rf-agent | ~7 MB |

### Memory Usage

- Idle: ~4-6 MB RSS
- Active (executing command): ~8-12 MB RSS
- Well within Android background service limits
