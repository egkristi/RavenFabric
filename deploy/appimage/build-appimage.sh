#!/bin/bash
# Build AppImage for RavenFabric
# Requires: linuxdeploy, cargo (Rust toolchain)
set -euo pipefail

APP_NAME="RavenFabric"
VERSION="${1:-0.1.6}"
ARCH="${2:-x86_64}"

APPDIR="${APP_NAME}.AppDir"

echo "Building RavenFabric ${VERSION} AppImage for ${ARCH}..."

# Build release binaries
cargo build --release -p rf-agent -p rf-relay -p rf-cli

# Create AppDir structure
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/scalable/apps"

# Copy binaries
cp target/release/rf-agent "${APPDIR}/usr/bin/"
cp target/release/rf-relay "${APPDIR}/usr/bin/"
cp target/release/rf "${APPDIR}/usr/bin/"

# Create desktop file
cat > "${APPDIR}/usr/share/applications/io.ravenfabric.Agent.desktop" << 'EOF'
[Desktop Entry]
Type=Application
Name=RavenFabric Agent
Comment=Secure remote execution and mesh networking agent
Exec=rf-agent
Icon=ravenfabric
Categories=Network;System;
Terminal=true
NoDisplay=true
EOF

cp "${APPDIR}/usr/share/applications/io.ravenfabric.Agent.desktop" "${APPDIR}/io.ravenfabric.Agent.desktop"

# Copy icon
cp website/assets/favicon.svg "${APPDIR}/usr/share/icons/hicolor/scalable/apps/ravenfabric.svg"
cp website/assets/favicon.svg "${APPDIR}/ravenfabric.svg"

# Create AppRun
cat > "${APPDIR}/AppRun" << 'APPRUN'
#!/bin/bash
SELF="$(readlink -f "$0")"
APPDIR="${SELF%/*}"
export PATH="${APPDIR}/usr/bin:${PATH}"

BASENAME="$(basename "$0")"
case "$BASENAME" in
  rf|rf-agent|rf-relay)
    exec "${APPDIR}/usr/bin/${BASENAME}" "$@"
    ;;
  *)
    # Default: show help
    echo "RavenFabric — Secure remote execution and mesh networking"
    echo ""
    echo "Available commands:"
    echo "  rf          CLI client"
    echo "  rf-agent    Agent daemon"
    echo "  rf-relay    Relay broker"
    echo ""
    echo "Symlink this AppImage to 'rf', 'rf-agent', or 'rf-relay' to run directly."
    exec "${APPDIR}/usr/bin/rf" "$@"
    ;;
esac
APPRUN
chmod +x "${APPDIR}/AppRun"

# Build AppImage using appimagetool if available
if command -v appimagetool &>/dev/null; then
  ARCH="${ARCH}" appimagetool "${APPDIR}" "${APP_NAME}-${VERSION}-${ARCH}.AppImage"
  echo "Created: ${APP_NAME}-${VERSION}-${ARCH}.AppImage"
else
  echo "appimagetool not found. AppDir created at: ${APPDIR}/"
  echo "Install appimagetool from https://github.com/AppImage/appimagetool"
  echo "Then run: ARCH=${ARCH} appimagetool ${APPDIR} ${APP_NAME}-${VERSION}-${ARCH}.AppImage"
fi
