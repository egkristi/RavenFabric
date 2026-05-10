#!/bin/bash
# Build macOS DMG installer for RavenFabric
set -euo pipefail

VERSION="${1:-0.1.6}"
APP_NAME="RavenFabric"
DMG_NAME="ravenfabric-${VERSION}-macos"
BUILD_DIR="target/release"
STAGING_DIR="target/dmg-staging"

echo "Building RavenFabric v${VERSION} for macOS..."

# Build release binaries
cargo build --release --bin rf-agent --bin rf-relay --bin rf

# Create staging directory
rm -rf "${STAGING_DIR}"
mkdir -p "${STAGING_DIR}/${APP_NAME}"

# Copy binaries
cp "${BUILD_DIR}/rf-agent" "${STAGING_DIR}/${APP_NAME}/"
cp "${BUILD_DIR}/rf-relay" "${STAGING_DIR}/${APP_NAME}/"
cp "${BUILD_DIR}/rf" "${STAGING_DIR}/${APP_NAME}/"

# Copy docs
cp README.md "${STAGING_DIR}/${APP_NAME}/"
cp LICENSE "${STAGING_DIR}/${APP_NAME}/"
cp packaging/config/raven.toml "${STAGING_DIR}/${APP_NAME}/raven.toml.example"

# Create install script
cat > "${STAGING_DIR}/${APP_NAME}/install.sh" << 'EOF'
#!/bin/bash
set -euo pipefail
PREFIX="${1:-/usr/local}"
echo "Installing RavenFabric to ${PREFIX}/bin..."
install -d "${PREFIX}/bin"
install -m 755 rf-agent "${PREFIX}/bin/rf-agent"
install -m 755 rf-relay "${PREFIX}/bin/rf-relay"
install -m 755 rf "${PREFIX}/bin/rf"
echo "Done. Run 'rf --help' to get started."
EOF
chmod +x "${STAGING_DIR}/${APP_NAME}/install.sh"

# Create DMG
echo "Creating DMG..."
hdiutil create -volname "${APP_NAME}" \
    -srcfolder "${STAGING_DIR}" \
    -ov -format UDZO \
    "target/${DMG_NAME}.dmg"

echo "DMG created: target/${DMG_NAME}.dmg"

# If codesign identity is available, sign it
if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
    echo "Signing DMG..."
    codesign --sign "${CODESIGN_IDENTITY}" "target/${DMG_NAME}.dmg"
    echo "DMG signed."
fi

echo "Done!"
