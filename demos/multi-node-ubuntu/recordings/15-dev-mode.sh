#!/usr/bin/env bash
# Recording: Dev Mode (Zero-Setup)
# Run inside: asciinema rec --command "bash recordings/15-dev-mode.sh"
source "$(dirname "$0")/helpers.sh"

clear
section "RavenFabric — Dev Mode (Zero-Setup)"

comment "One command to start a complete RavenFabric environment"
comment "No Docker, no config files, no key exchange"
sleep 1

comment "Start dev mode (relay + agent in one process):"
type_cmd "rf dev"
sleep 3

comment "In another terminal, execute commands:"
run_cmd "rf exec --token dev 'hostname'"
sleep 3

comment "Run multi-line scripts:"
run_cmd "rf exec --token dev 'uname -a && uptime && whoami'"
sleep 3

comment "Stream output in real time:"
type_cmd "rf exec --token dev --stream 'for i in 1 2 3; do echo \$i; sleep 1; done'"
sleep 3

comment "Custom port: rf dev --port 8080"
comment "Custom bind: rf dev --bind 0.0.0.0"
sleep 2

comment "Stop with Ctrl+C — clean shutdown"
sleep 2

section "Done"
