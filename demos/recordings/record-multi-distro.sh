#!/usr/bin/env bash
# Recording: Multi-Distro Linux Demo
# Run: asciinema rec --command "bash demos/recordings/record-multi-distro.sh" demos/recordings/multi-distro.cast --cols 100 --rows 28 --overwrite
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
export PATH="$ROOT/target/release:$PATH"
export RUST_LOG=warn
RELAY="ws://127.0.0.1:9092"

type_cmd() {
    local cmd="$1"
    printf '\033[1;32m$\033[0m '
    for ((i=0; i<${#cmd}; i++)); do
        printf '%s' "${cmd:$i:1}"
        sleep 0.03
    done
    echo ""
    sleep 0.3
}

run_cmd() {
    type_cmd "$1"
    eval "$1"
    echo ""
    sleep "${2:-2}"
}

section() {
    echo ""
    printf '\033[1;36m  %s\033[0m\n' "$1"
    echo "  $(printf '%.0s─' {1..60})"
    echo ""
    sleep 1
}

comment() {
    printf '\033[0;90m  # %s\033[0m\n' "$1"
    sleep 0.5
}

clear
section "RavenFabric — Multi-Distro Linux"

comment "Same static musl binary runs on every major Linux distribution"
comment "No runtime dependencies, no compilation, no package manager needed"
sleep 1

comment "Ubuntu 24.04 (apt/deb, glibc)"
run_cmd "rf --relay $RELAY exec --token ubuntu 'cat /etc/os-release | head -2'" 6

comment "Debian 12 (apt/deb, glibc)"
run_cmd "rf --relay $RELAY exec --token debian 'cat /etc/os-release | head -2'" 6

comment "Fedora 41 (dnf/rpm, glibc)"
run_cmd "rf --relay $RELAY exec --token fedora 'cat /etc/os-release | head -2'" 6

comment "Alpine 3.20 (apk, musl-native)"
run_cmd "rf --relay $RELAY exec --token alpine 'cat /etc/os-release | head -2'" 6

comment "Rocky Linux 9 (dnf/rpm, glibc, RHEL-compatible)"
run_cmd "rf --relay $RELAY exec --token rocky 'cat /etc/os-release | head -2'" 6

comment "Amazon Linux 2023 (dnf/rpm, glibc, AWS)"
run_cmd "rf --relay $RELAY exec --token amazon 'cat /etc/os-release | head -2'" 6

section "One binary · 9 distros · Zero dependencies"
sleep 2
