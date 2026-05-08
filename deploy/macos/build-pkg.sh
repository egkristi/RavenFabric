#!/bin/bash
# Build macOS .pkg installer for RavenFabric
# Requires: macOS with pkgbuild and productbuild
# Optional: Apple Developer ID Installer certificate for signing
set -euo pipefail

VERSION="${1:-0.1.3}"
APP_NAME="RavenFabric"
PKG_ID="io.ravenfabric.agent"
INSTALL_PREFIX="/usr/local"
BUILD_DIR="target/release"
STAGING_DIR="target/pkg-staging"
SCRIPTS_DIR="target/pkg-scripts"
PKG_OUTPUT="target/${APP_NAME}-${VERSION}.pkg"

echo "Building RavenFabric v${VERSION} macOS .pkg installer..."

# Build release binaries (universal if on Apple Silicon with both targets)
if [[ "$(uname -m)" == "arm64" ]] && rustup target list --installed | grep -q x86_64-apple-darwin; then
    echo "Building universal binary (arm64 + x86_64)..."
    cargo build --release --bin rf-agent --bin rf-relay --bin rf --target aarch64-apple-darwin
    cargo build --release --bin rf-agent --bin rf-relay --bin rf --target x86_64-apple-darwin
    mkdir -p "${BUILD_DIR}"
    for bin in rf-agent rf-relay rf; do
        lipo -create \
            "target/aarch64-apple-darwin/release/${bin}" \
            "target/x86_64-apple-darwin/release/${bin}" \
            -output "${BUILD_DIR}/${bin}"
    done
else
    echo "Building native binary..."
    cargo build --release --bin rf-agent --bin rf-relay --bin rf
fi

# Create staging directory (mirrors install location)
rm -rf "${STAGING_DIR}" "${SCRIPTS_DIR}"
mkdir -p "${STAGING_DIR}${INSTALL_PREFIX}/bin"
mkdir -p "${STAGING_DIR}/etc/ravenfabric"
mkdir -p "${STAGING_DIR}/Library/LaunchDaemons"
mkdir -p "${SCRIPTS_DIR}"

# Copy binaries
install -m 755 "${BUILD_DIR}/rf-agent" "${STAGING_DIR}${INSTALL_PREFIX}/bin/"
install -m 755 "${BUILD_DIR}/rf-relay" "${STAGING_DIR}${INSTALL_PREFIX}/bin/"
install -m 755 "${BUILD_DIR}/rf" "${STAGING_DIR}${INSTALL_PREFIX}/bin/"

# Copy example config
if [[ -f "deploy/raven.toml.example" ]]; then
    install -m 644 "deploy/raven.toml.example" "${STAGING_DIR}/etc/ravenfabric/raven.toml.example"
fi

# Copy launchd plist
if [[ -f "deploy/io.ravenfabric.agent.plist" ]]; then
    install -m 644 "deploy/io.ravenfabric.agent.plist" "${STAGING_DIR}/Library/LaunchDaemons/"
fi

# Create postinstall script
cat > "${SCRIPTS_DIR}/postinstall" << 'POSTINSTALL'
#!/bin/bash
# Post-installation script for RavenFabric

# Create config directory if it doesn't exist
mkdir -p /etc/ravenfabric

# Copy example config if no config exists
if [[ ! -f /etc/ravenfabric/raven.toml ]]; then
    if [[ -f /etc/ravenfabric/raven.toml.example ]]; then
        cp /etc/ravenfabric/raven.toml.example /etc/ravenfabric/raven.toml
        echo "Created default config at /etc/ravenfabric/raven.toml"
    fi
fi

# Generate agent key if it doesn't exist
if [[ ! -f /etc/ravenfabric/agent.key ]]; then
    /usr/local/bin/rf-agent --generate-key /etc/ravenfabric/agent.key 2>/dev/null || true
fi

# Set permissions
chmod 700 /etc/ravenfabric
chmod 600 /etc/ravenfabric/*.key 2>/dev/null || true

echo "RavenFabric installed successfully."
echo "  Agent:  /usr/local/bin/rf-agent"
echo "  Relay:  /usr/local/bin/rf-relay"
echo "  CLI:    /usr/local/bin/rf"
echo ""
echo "To start the agent as a service:"
echo "  sudo launchctl load /Library/LaunchDaemons/io.ravenfabric.agent.plist"

exit 0
POSTINSTALL
chmod +x "${SCRIPTS_DIR}/postinstall"

# Create preinstall script
cat > "${SCRIPTS_DIR}/preinstall" << 'PREINSTALL'
#!/bin/bash
# Pre-installation script for RavenFabric

# Stop existing service if running
if launchctl list | grep -q io.ravenfabric.agent; then
    echo "Stopping existing RavenFabric agent..."
    sudo launchctl unload /Library/LaunchDaemons/io.ravenfabric.agent.plist 2>/dev/null || true
fi

exit 0
PREINSTALL
chmod +x "${SCRIPTS_DIR}/preinstall"

# Build component package
echo "Building component package..."
pkgbuild \
    --root "${STAGING_DIR}" \
    --identifier "${PKG_ID}" \
    --version "${VERSION}" \
    --scripts "${SCRIPTS_DIR}" \
    --install-location "/" \
    "target/${APP_NAME}-component.pkg"

# Create distribution XML for productbuild
cat > "target/distribution.xml" << DIST
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>RavenFabric v${VERSION}</title>
    <organization>${PKG_ID}</organization>
    <license file="LICENSE"/>
    <readme file="README.md"/>
    <welcome file="deploy/macos/welcome.html"/>
    <domains enable_localSystem="true"/>
    <options customize="never" require-scripts="true" rootVolumeOnly="true"/>
    <choices-outline>
        <line choice="default">
            <line choice="${PKG_ID}"/>
        </line>
    </choices-outline>
    <choice id="default"/>
    <choice id="${PKG_ID}" visible="false">
        <pkg-ref id="${PKG_ID}"/>
    </choice>
    <pkg-ref id="${PKG_ID}" version="${VERSION}" onConclusion="none">${APP_NAME}-component.pkg</pkg-ref>
</installer-gui-script>
DIST

# Create welcome HTML
mkdir -p deploy/macos
cat > "deploy/macos/welcome.html" << 'WELCOME'
<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><style>
body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; padding: 20px; }
h1 { color: #333; } p { color: #555; line-height: 1.6; }
code { background: #f0f0f0; padding: 2px 6px; border-radius: 3px; }
</style></head>
<body>
<h1>RavenFabric</h1>
<p>Secure remote execution and mesh networking agent.</p>
<p>This installer will place the following in <code>/usr/local/bin</code>:</p>
<ul>
<li><code>rf-agent</code> — The RavenFabric agent daemon</li>
<li><code>rf-relay</code> — The relay broker</li>
<li><code>rf</code> — The CLI client</li>
</ul>
<p>Configuration will be placed in <code>/etc/ravenfabric/</code>.</p>
<p>A launchd service plist will be installed for background agent operation.</p>
</body>
</html>
WELCOME

# Build product archive
echo "Building product archive..."
productbuild \
    --distribution "target/distribution.xml" \
    --package-path "target" \
    --resources "." \
    "${PKG_OUTPUT}"

# Sign if identity is available
SIGNING_IDENTITY="${SIGNING_IDENTITY:-}"
if [[ -n "${SIGNING_IDENTITY}" ]]; then
    echo "Signing package with identity: ${SIGNING_IDENTITY}"
    productsign \
        --sign "${SIGNING_IDENTITY}" \
        "${PKG_OUTPUT}" \
        "${PKG_OUTPUT}.signed"
    mv "${PKG_OUTPUT}.signed" "${PKG_OUTPUT}"
    echo "Package signed successfully."
else
    echo "No SIGNING_IDENTITY set — package is unsigned."
    echo "To sign: SIGNING_IDENTITY='Developer ID Installer: ...' $0 ${VERSION}"
fi

# Report
PKG_SIZE=$(du -h "${PKG_OUTPUT}" | cut -f1)
echo ""
echo "macOS .pkg installer built successfully:"
echo "  Output: ${PKG_OUTPUT}"
echo "  Size:   ${PKG_SIZE}"
echo "  ID:     ${PKG_ID}"

# Cleanup
rm -rf "${STAGING_DIR}" "${SCRIPTS_DIR}" "target/${APP_NAME}-component.pkg" "target/distribution.xml"
