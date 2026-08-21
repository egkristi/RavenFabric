#!/bin/sh
# RavenFabric install script — POSIX-compatible (dash, bash, BusyBox ash)
# Usage: curl -fsSL https://ravenfabric.io/install.sh | sh
# Or:    curl -fsSL https://get.ravenfabric.io | sh

set -eu

REPO="egkristi/RavenFabric-Published"
INSTALL_DIR="${RAVENFABRIC_INSTALL_DIR:-/usr/local/bin}"
tmpdir=""

# Cleanup on exit (handle tmpdir being unset)
cleanup() {
    if [ -n "${tmpdir:-}" ] && [ -d "${tmpdir:-}" ]; then
        rm -rf "${tmpdir}" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# Colors (disabled if not a terminal)
if [ -t 1 ]; then
    BOLD='\033[1m'; DIM='\033[2m'; GOLD='\033[33m'
    GREEN='\033[32m'; RED='\033[31m'; RESET='\033[0m'
else
    BOLD=''; DIM=''; GOLD=''; GREEN=''; RED=''; RESET=''
fi

info() { printf "%b=>%b %s\\n" "${BOLD}${GOLD}" "${RESET}" "$1"; }
ok()   { printf "%b✓%b %s\\n" "${GREEN}" "${RESET}" "$1"; }
err()  { printf "%b✗%b %s\\n" "${RED}" "${RESET}" "$1" >&2; }
die()  { err "$1"; exit 1; }

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
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/'
    else
        die "Neither curl nor wget found. Install one and retry."
    fi
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    else
        wget -q "$1" -O "$2"
    fi
}

main() {
    echo ""
    printf "%bRavenFabric Installer%b\\n" "${BOLD}" "${RESET}"
    printf "%bSecurity-first distributed execution engine%b\\n" "${DIM}" "${RESET}"
    echo ""

    os="$(detect_os)"
    arch="$(detect_arch)"

    version="${RAVENFABRIC_VERSION:-}"
    if [ -z "$version" ]; then
        version="$(latest_version)"
    fi

    if [ -z "$version" ]; then
        die "Could not determine latest version. Set RAVENFABRIC_VERSION manually."
    fi

    # Prefer musl on Linux for maximum portability
    suffix=""
    if [ "$os" = "linux" ]; then
        suffix="-musl"
    fi

    base_url="https://github.com/${REPO}/releases/download/v${version}"
    artifact="ravenfabric-${os}-${arch}${suffix}"

    info "OS: ${os}, Arch: ${arch}, Version: v${version}"

    tmpdir="$(mktemp -d)" || die "Cannot create temp directory"

    # Download each binary — no bash arrays, POSIX iteration
    for bin in agent relay cli; do
        name="${artifact}-${bin}"
        info "Downloading ${name}..."
        download "${base_url}/${name}" "${tmpdir}/${name}"
        chmod +x "${tmpdir}/${name}"
    done

    info "Installing to ${INSTALL_DIR}/ (may require sudo)..."

    use_sudo=""
    if [ ! -w "$INSTALL_DIR" ]; then
        if command -v sudo >/dev/null 2>&1; then
            use_sudo="sudo"
        else
            die "Cannot write to ${INSTALL_DIR} and sudo is not available."
        fi
    fi

    ${use_sudo} mkdir -p "$INSTALL_DIR"
    ${use_sudo} cp "${tmpdir}/${artifact}-agent" "${INSTALL_DIR}/rf-agent"
    ${use_sudo} cp "${tmpdir}/${artifact}-relay" "${INSTALL_DIR}/rf-relay"
    ${use_sudo} cp "${tmpdir}/${artifact}-cli"   "${INSTALL_DIR}/rf"

    echo ""
    ok "rf-agent installed to ${INSTALL_DIR}/rf-agent"
    ok "rf-relay installed to ${INSTALL_DIR}/rf-relay"
    ok "rf       installed to ${INSTALL_DIR}/rf"
    echo ""
    printf "%bRun 'rf --help' to get started.%b\\n" "${DIM}" "${RESET}"
    printf "%bDocs: https://ravenfabric.io/docs/%b\\n" "${DIM}" "${RESET}"
    echo ""
}

main "$@"
