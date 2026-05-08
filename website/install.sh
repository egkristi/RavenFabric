#!/bin/bash
# RavenFabric install script
# Usage: curl -fsSL https://ravenfabric.io/install.sh | sh
# Or:    curl -fsSL https://get.ravenfabric.io | sh (alias)

set -euo pipefail

REPO="egkristi/RavenFabric-Published"
INSTALL_DIR="${RAVENFABRIC_INSTALL_DIR:-/usr/local/bin}"

# Colors (disabled if not a terminal)
if [ -t 1 ]; then
    BOLD="\033[1m"
    DIM="\033[2m"
    GOLD="\033[33m"
    GREEN="\033[32m"
    RED="\033[31m"
    RESET="\033[0m"
else
    BOLD="" DIM="" GOLD="" GREEN="" RED="" RESET=""
fi

info()  { echo -e "${BOLD}${GOLD}=>${RESET} $1"; }
ok()    { echo -e "${GREEN}✓${RESET} $1"; }
err()   { echo -e "${RED}✗${RESET} $1" >&2; }
die()   { err "$1"; exit 1; }

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "darwin" ;;
        *)       die "Unsupported OS: $(uname -s). Use cargo install instead." ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)   echo "amd64" ;;
        aarch64|arm64)  echo "arm64" ;;
        armv7l)         echo "armv7" ;;
        *)              die "Unsupported architecture: $(uname -m)" ;;
    esac
}

latest_version() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/'
    else
        die "Neither curl nor wget found. Install one and retry."
    fi
}

download() {
    local url="$1" dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$dest"
    else
        wget -q "$url" -O "$dest"
    fi
}

main() {
    echo ""
    echo -e "${BOLD}RavenFabric Installer${RESET}"
    echo -e "${DIM}Security-first distributed execution engine${RESET}"
    echo ""

    local os arch version
    os="$(detect_os)"
    arch="$(detect_arch)"
    version="${RAVENFABRIC_VERSION:-$(latest_version)}"

    [ -z "$version" ] && die "Could not determine latest version. Set RAVENFABRIC_VERSION manually."

    # Prefer musl on Linux for maximum portability
    local suffix=""
    if [ "$os" = "linux" ]; then
        suffix="-musl"
    fi

    local base_url="https://github.com/${REPO}/releases/download/v${version}"
    local artifact="ravenfabric-${os}-${arch}${suffix}"

    info "OS: ${os}, Arch: ${arch}, Version: v${version}"

    local tmpdir
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    local binaries=("agent" "relay" "cli")
    for bin in "${binaries[@]}"; do
        local ext=""
        local name="${artifact}-${bin}"
        info "Downloading ${name}..."
        download "${base_url}/${name}" "${tmpdir}/${name}"
        chmod +x "${tmpdir}/${name}"
    done

    info "Installing to ${INSTALL_DIR}/ (may require sudo)..."

    local use_sudo=""
    if [ ! -w "$INSTALL_DIR" ]; then
        if command -v sudo >/dev/null 2>&1; then
            use_sudo="sudo"
        else
            die "Cannot write to ${INSTALL_DIR} and sudo is not available."
        fi
    fi

    $use_sudo mkdir -p "$INSTALL_DIR"
    $use_sudo cp "${tmpdir}/${artifact}-agent" "${INSTALL_DIR}/rf-agent"
    $use_sudo cp "${tmpdir}/${artifact}-relay" "${INSTALL_DIR}/rf-relay"
    $use_sudo cp "${tmpdir}/${artifact}-cli"   "${INSTALL_DIR}/rf"

    echo ""
    ok "rf-agent installed to ${INSTALL_DIR}/rf-agent"
    ok "rf-relay installed to ${INSTALL_DIR}/rf-relay"
    ok "rf       installed to ${INSTALL_DIR}/rf"
    echo ""
    echo -e "${DIM}Run 'rf --help' to get started.${RESET}"
    echo -e "${DIM}Docs: https://ravenfabric.io/docs/${RESET}"
    echo ""
}

main "$@"
