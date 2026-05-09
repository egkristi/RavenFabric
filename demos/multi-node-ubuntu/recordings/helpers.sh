#!/usr/bin/env bash
# Asciinema recording helpers — shared by all recording scripts.
#
# Provides type_cmd() for simulated typing and run_cmd() for
# typing + execution with natural delays.

# Typing simulation — prints each character with a delay
type_cmd() {
    local cmd="$1"
    local delay="${2:-0.03}"
    printf '\033[1;32m$\033[0m '
    for ((i=0; i<${#cmd}; i++)); do
        printf '%s' "${cmd:$i:1}"
        sleep "$delay"
    done
    echo ""
    sleep 0.3
}

# Type a command then execute it
run_cmd() {
    type_cmd "$1"
    eval "$1"
    echo ""
    sleep "${2:-1.5}"
}

# Section header
section() {
    echo ""
    printf '\033[1;36m  %s\033[0m\n' "$1"
    echo "  $(printf '%.0s─' {1..50})"
    echo ""
    sleep 1
}

# Comment line
comment() {
    printf '\033[0;90m  # %s\033[0m\n' "$1"
    sleep 0.5
}
