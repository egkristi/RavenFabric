# Changelog

All notable changes to RavenFabric will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Kubernetes operator CRDs** — Added installable CRD manifests for `RavenAgent`, `RavenPolicy`, `RavenRelay`, and `RavenMesh` (group `ravenfabric.io`), wired into the Helm chart (`crds/` directory + `crds.enabled` toggle). OpenAPI v3 schemas enforce required fields and printer columns. Verified in the `ravenfabric` K8s namespace: CRs create/list correctly and schema validation rejects a `RavenAgent` missing `spec.id`.

## [1.0.0-rc.13] — 2026-08-22

### Added

- **`rf-controller` binary (Web dashboard + REST API)** — New management-plane binary that serves the embedded Web UI dashboard and REST API. Adds `POST /api/v1/agents/heartbeat` for agent registration/heartbeat ingestion (mutating `ApiDispatcher`), and fixes `/healthz` routing so the liveness endpoint returns `{"status":"healthy"}`. Ships with a hardened `deploy/rf-controller.service` systemd unit and `docs/src/guide/controller.md`.

## [1.0.0-rc.12] — 2026-08-22

### Added

- **CLI relay failover (Relay HA)** — `rf --relay` now accepts a comma-separated list of relay URLs. On connection or Noise XX handshake failure, the CLI fails over to the next relay in order instead of erroring out. This completes controller-side relay high-availability, complementing the agent-side failover added in v1.0.0-beta.4. Applies to all commands routed through `dial_agent` (exec, shell, forward, playbook, status, cp, proxy, secret).

## [1.0.0-rc.11] — 2026-08-22

### Fixed

- **Relay health prober deadlock (root cause of relay handshake hang)** — The agent's background relay health prober was dialing with the agent's real meet token and performing a full Noise XX handshake. Since both the agent and the prober are Noise responders, the relay paired them together — deadlocking both and stealing the pairing from the CLI initiator. The prober now uses a distinct `__health_probe__<token>` meet token and skips the handshake entirely (a probe has no initiator peer, so a handshake would always time out). Relay exec now completes end-to-end.
- **Version consistency** — Reconciled version strings across the workspace (`rc.9`), inter-crate dependencies (`rc.6`), deploy manifests, SDKs, website, and documentation to a single `rc.11` release.

## [1.0.0-rc.10] — 2026-07-02

### Fixed

- **Relay yield** — Relay now always yields after mpsc send to prevent a single connection from monopolizing the event loop.
- **Exit code propagation** — `rf exec` now returns the remote command's exit code to the shell.
- **Secret store error clarity** — Added a `--seal-key-path` hint to the "secret store not configured" error.

## [1.0.0-rc.9] — 2026-07-02

### Changed

- **Kubernetes testing** — Additional K8s integration test scenarios and manifests.

## [1.0.0-rc.8] — 2026-07-02

### Fixed

- **Noise XX handshake flush** — Added `flush()` after each handshake message and reduced the handshake timeout from 30s to 10s to fix relay handshake timeouts.

## [1.0.0-rc.7] — 2026-07-02

### Fixed

- **Noise XX handshake retry** — Added a 3-attempt retry loop with `flush()` after each message to fix relay timeouts.

## [1.0.0-rc.6] — 2026-07-02

### Fixed

- **`rf cp` agent→local pull-stream hang** — File transfers from agent to local (pull direction) would hang for any file size >0 bytes. Root cause: `SecureChannel::send()` never flushed the underlying transport, so the last data frame(s) remained buffered and never reached the client. Added `SecureChannel::flush()` public method and call it after the last frame in `handle_file_pull_stream()`. Also added a 30-second idle timeout to the client's receive loop as a safety net.

### Added

- **Full feature overview asciinema demo** — New Demo 11 on the website demos page (`/demos/#full-overview`) with an asciinema embed covering all 19 sections of the RavenFabric demo script. CSP updated across `website/index.html`, `website/demos/index.html`, and `website/_headers` to allow asciinema.org embeds. "Watch demo" button added to the main page hero CTA.

## [1.0.0-rc.5] — 2026-07-02

### Fixed

- **Streaming exec hang in `rf dev` mode** — After any exec command completed, the dev agent's `chan.recv()` would hang indefinitely on a half-closed TCP connection, preventing the agent from reconnecting for subsequent commands. Added a 5-second read timeout to `connect_dev_agent()` so the agent properly detects disconnection and reconnects to the relay.

## [1.0.0-rc.4] — 2026-06-30

### Changed

- **Dependency maintenance** — Merged 3 Dependabot PRs: codecov/codecov-action v6→v7, maxminddb 0.27.3→0.28.1, serde_json 1.0.149→1.0.150.
- **cryptoki skipped** — PR #123 (cryptoki 0.6→0.12) deferred due to breaking API changes in the HSM module. Tracked as GitHub Issue.

## [1.0.0-rc.3] — 2026-06-30

### Changed

- **Dependency maintenance** — Merged 4 Dependabot PRs: wasmtime v36→v46, yamux v0.13→v0.14, actions/checkout v6→v7, vite v7.3.3→v7.3.6. Applied rusqlite v0.39→v0.40 update.

## [1.0.0-rc.2] — 2026-06-30

### Fixed

- **rf-mcp-client keyword** — `"model-context-protocol"` (23 chars) shortened to `"mcp-protocol"` (12 chars) to comply with crates.io 20-char keyword limit.
- **rf-ingress missing from publish list** — Added `rf-ingress` to the `publish-crates` job in `.github/workflows/release.yml`.

### Added

- **Release workflow configured for crates.io** — Publish workflow added for all 13 crates (rf-audit, rf-crypto, rf-bootstrap, rf-transport, rf-policy, rf-rpc, rf-executor, rf-mcp-client, rf-mcp-server, rf-relay, rf-agent, rf-cli, rf-ingress) at v1.0.0-rc.1.

## [1.0.0-rc.1] — 2026-06-30

### Added

- **Release Candidate** — All features through v1.3 implemented (Secure Access Layer, Fleet Operations, Enterprise & Compliance). 14 crates, ~75,170 LOC, 1,429 tests, 0 clippy warnings.
- **Version bump** — 1.0.0-beta.6 → 1.0.0-rc.1 across all Cargo.toml, docs, website, SDKs, and deploy files.

### Changed

- **Dependency maintenance** — `cargo update` applied 81 compatible dependency updates across the workspace, including chrono, hyper, quinn, rustls, serde_json, wasmtime, and zeroize.

## [1.0.0-beta.5] — 2026-06-30

### Added

- **Regulatory compliance documentation** — GDPR (Articles 5, 17, 32, 33, 35), PCI-DSS v4.0 (Requirements 1–12), and SOC 2 (Trust Services Criteria CC1–CC9, A1, C1, PI1) compliance mappings documented in `docs/compliance/frameworks/`.
- **Data retention/deletion API** — `FileAuditLogger::purge_entries_before(cutoff)` removes entries older than a timestamp; `delete_entries_by_filter(filter)` removes entries matching AND-combined criteria (older_than, action, caller_key, decision, request_id_contains). HMAC chain is preserved via `rewrite_chain()`.
- **`AuditError::NoEntriesMatched`** — New error variant returned when no audit entries match deletion criteria.
- **6 new tests** — Coverage for purge, filter, and combined deletion scenarios with HMAC chain integrity verification.

### Changed

- **Compliance matrix updated** — `docs/compliance/README.md` and mdBook copy now include GDPR, PCI-DSS, and SOC 2 entries.

## [1.0.0-beta.4] — 2026-06-30

### Added

- **Relay HA failover** — Agent now supports multiple relay URLs via `[[transport.relay_clusters]]` configuration. On connection failure, the agent automatically fails over to the next healthy relay in the cluster instead of retrying the same URL. A background health prober measures RTT to all configured relays every 5 minutes for observability.
- **`relay_url` field in `AgentInfo`** — Controller now tracks which relay each agent is connected through, enabling clients to discover agent→relay mappings for HA-aware routing.

## [1.0.0-beta.3] — 2026-06-30

### Changed

- **Cached `sysinfo::System` in health check probes** — `check_process_alive()` now uses a `OnceLock<Mutex<System>>` to avoid re-reading `/proc` on every process health check, reducing memory churn from repeated `sysinfo::System::new()` allocations.

### Fixed

- **`check_process_alive()` memory leak** — Previously created a new `sysinfo::System` instance on every call, each enumerating all processes. Now cached and reused.

## [1.0.0-beta.2] — 2026-06-29

### Changed

- **Reduced WebSocket duplex buffer** — Lowered from 256 KB to 64 KB, saving 192 KB per connection. Noise XX handshake messages are ~200 bytes, so 64 KB provides ample headroom for asymmetric relay latency without wasting memory.

### Fixed

- **`--constrained` help text accuracy** — Updated to describe only what it actually does (audit buffer reduction), removing misleading claim about transport buffer sizes.

## [1.0.0-beta.1] — 2026-06-29

### Added

- **`--export-hmac-key` flag to `rf-agent`** — Derives an HMAC-SHA256 key from the agent identity key via HKDF-SHA256 and prints it in hex. Enables external log verification without exposing the full identity private key.
- **`rf audit derive-key` subcommand to `rf-cli`** — Same HKDF-SHA256 key derivation for the CLI, enabling operators to derive the HMAC key from an agent key file for offline audit verification.
- **`BufferedAuditCollector` wrapping in `rf-agent`** — The `FileAuditLogger` is now wrapped in a `BufferedAuditCollector` (4096-entry ring buffer, 5-second flush interval) to bound memory growth under high-throughput audit workloads.
- **`StaticKey::private_bytes()` made public** — The `rf-crypto` crate now exposes the private key bytes publicly, enabling external tools (agent, CLI) to derive HMAC keys without duplicating key material access logic.
- **`--constrained` flag to `rf-agent`** — Enables memory-constrained mode for IoT / low-RAM devices. Reduces audit buffer from 4096 to 512 entries, dedup window from 1024 to 256, and flushes every 2 seconds. Also configurable via `constrained = true` in `raven.toml`.
- **`CollectorConfig::constrained()` preset** — New constructor on `BufferedAuditCollector` config providing conservative buffer sizes for constrained environments (~2 MB RSS savings).
- **Reduced WebSocket duplex buffer** — Lowered from 1 MB to 256 KB, saving ~768 KB per connection with no functional impact.

### Fixed

- **Cross-platform Noise XX handshake** — Added `--compat-mode` flag (from v0.25.4) resolves macOS→Linux handshake failures via relay.
- **Release date in ROADMAP.md** — Corrected from 2026-07-27 to actual release date 2026-06-29.

## [0.25.4] — 2026-06-29

### Added

- **`--compat-mode` flag** — Added to `rf-agent`, `rf-relay`, and `rf-cli` binaries. Enables relaxed Noise XX handshake timing for cross-platform compatibility (e.g., macOS→Linux via relay).
- **`--version` flag to rf-agent** — `rf-agent --version` now prints version info via clap's built-in `#[command(version)]`.
- **`--reason` flag to `rf exec`** — CLI now accepts an optional `--reason` string that is threaded through to the executor and included in audit log entries.
- **Handshake metrics counters** — `handshakes_completed` and `handshake_latency_us` Prometheus counters are now incremented on every successful Noise XX handshake in the agent.
- **Active connections tracking** — `active_connections` Prometheus counter is incremented on connection open and decremented on connection close via a RAII `ConnectionTracker` guard.
- **Playbook documentation** — Full YAML schema, rollout strategies (parallel, sequential, rolling, canary), failure policies, and examples in `docs/src/guide/fleet-orchestration.md`.

### Fixed

- **`rf cp` chunking for files >65535B** — `MAX_FRAME_PAYLOAD` corrected from 65535 to 65519 (65535 - 16-byte ChaChaPoly MAC tag). All chunk constants in CLI and agent updated to match. The snow crate enforces `plaintext.len() + TAGLEN <= MAXMSGLEN`, so 65535-byte plaintext always failed encryption.
- **Clippy `too_many_arguments` warnings** — Added `#[allow(clippy::too_many_arguments)]` to `audit()`, `handle_execute()`, and `exec_command()`.

## [0.25.3] — 2026-06-27

### Added

- **`rf policy lint` command** — Validates policy YAML with 6 check categories: dangerous patterns (bash in allow list), overly broad regex, missing deny rules, filesystem allow/deny overlaps, HTTP allow without deny, missing resource limits.
- **`rf audit verify` command** — Checks HMAC-SHA256 chain continuity across the audit log, reports tampered entries, and shows chain gaps.
- **HMAC chain integrity for audit log** — Every `AuditEntry` now carries `prev_hash` and `hmac` fields. `FileAuditLogger` computes HMAC-SHA256 over canonical field representation on every write. `verify_audit_chain()` validates continuity.
- **Secret store wiring in agent** — Added `seal_key_path` to agent config, CLI args, and `ResolvedConfig`. `SecretStore` initialized on agent startup and wired to both Executor instances via `.with_secrets()`. New `seal_key_path` field in `raven.toml.example`.

### Fixed

- **Playbook YAML schema** — All 6 playbook files updated from map syntax to correct YAML tag syntax (`!agents [...]`, `!canary { ... }`, etc.) for serde_yaml externally-tagged enums.
- **File chunking in `rf cp`** — Fixed 3 locations where chunk size was 65536 instead of 65535 (wire protocol max frame payload).
- **AuditEntry constructor completeness** — All 32+ `AuditEntry` constructors across 5 crates updated with `prev_hash` and `hmac` fields.
- **Clippy warnings** — Fixed `needless_borrow` and `format!` variable warnings in `rf-cli` and `rf-ingress`.

## [0.25.2] — 2026-06-27

### Added

- **RavenFabric-specific Prometheus metrics** — `RavenFabricMetricsCollector` with 6 shared atomic counters: commands_allowed, commands_denied, audit_entries, active_connections, handshakes_completed, handshake_latency_us. Integrated into the `/metrics` HTTP endpoint alongside system metrics.
- **Audit log staleness detection** — `StalenessConfig` with configurable `max_idle_secs` (default 300) and `dedup_window_secs` (default 600). `AlertEngine::check_staleness()` fires an alert if no audit activity occurs within the idle window. `record_activity()` auto-called on every `evaluate()`.
- **Policy rules for non-exec actions** — Built-in coding assistant template now allows `port-forward`, `remote-forward`, `socks5-forward` (localhost only), and `proxy` actions.
- **MCP server systemd unit** — `rf-mcp-server.service` with security hardening (DynamicUser, ProtectSystem=strict, PrivateTmp, NoNewPrivileges), listens on `127.0.0.1:8080` HTTP+SSE.

### Fixed

- **Relay mode in agent systemd config** — `ExecStart` now includes `--relay ws://localhost:9090` so the agent connects to the local relay on startup.
- **Cross-platform Noise XX handshake** — Improved `Error::Input` handling from snow 0.10.0 with diagnostic logging, larger payload buffer (65535+256 bytes), and specific `HandshakeInput` error variant for better debugging of macOS→Linux relay handshake failures.
- **Restricted `bash` in policy allow list** — Deny patterns now cover bare `bash`, `sh`, `/bin/bash`, `/usr/bin/bash` in addition to the existing `python`/`perl`/`ruby` restrictions.

## [0.25.1] — 2026-05-24

### Fixed

- Dockerfile: pinned builder from `rust:1.88-alpine` (Alpine 3.20, 1 critical + 14 high CVEs) to `rust:1.88-alpine3.22` and CLI stage from `alpine:3.21` to `alpine:3.22` to use the latest Alpine release with fewer unpatched vulnerabilities.
- `.vscode/settings.json`: disabled `yaml.schemaStore.enable` to prevent the YAML extension from auto-matching RavenFabric playbook files against Docker network / other unrelated schemas (caused ~50 false-positive "missing property" errors). Added explicit Kubernetes JSON schema for demo Kubernetes manifests and Helm charts.
- WinGet manifest (`deploy/winget/RavenFabric.RavenFabric.yaml`): added required `PackageLocale: en-US` field; updated to v0.25.1; replaced empty `InstallerSha256` with zero-filled placeholder that satisfies the manifest schema pattern.
- F-Droid metadata (`deploy/fdroid/metadata/io.ravenfabric.agent.yml`): removed `subdir: .` (not a valid value in current schema; root directory is the default when `subdir` is absent).
- `release.yml`: GitHub Actions `secrets` context is not available in job-level `if` conditions — replaced invalid `if: ${{ secrets.CRATES_IO_TOKEN != '' }}` with a runtime shell guard inside the step. Added `|| ''` fallback to all custom secrets (`CRATES_IO_TOKEN`, `HOMEBREW_TAP_TOKEN`, `PUBLISH_BIN_TAP_TOKEN`) so expressions always resolve and VS Code "Context access might be invalid" diagnostics are suppressed.
- `ci.yml`: added `|| ''` fallback to `CODECOV_TOKEN` secret reference to suppress the "Context access might be invalid" VS Code diagnostic.


### Fixed

- `RelayCluster` in `rf-transport` used `#[cfg_attr(feature = "serde", ...)]` but `serde` is an unconditional dependency — removed invalid cfg_attr, derive directly.
- `ForwardConfig` manual `impl Default` replaced with `#[derive(Default)]` (clippy `derivable_impls`).
- `relay_clusters` / `RelayClusterConfig` fields in `rf-agent` were deserialized but never read — wired into `load_config()` for region-aware relay selection at startup.
- CI Coverage job: added `libtss2-dev` apt package so `tss-esapi-sys` builds with `--all-features`.
- CI Cross-compile job: switched to `cargo install cross --locked` (crates.io) to avoid GitHub git auth failures.
- Snap manifest deprecated `architectures:` key → migrated to `platforms:` for `base: core24`.
- WinGet manifest `$schema` pointed to `defaultLocale.1.6.0` but file is `ManifestType: singleton` — fixed to `singleton.1.6.0`.
- `rf-crypto` no_std: `pub mod hsm` / `pub mod tpm` gated with `#[cfg(feature)]` to fix 20-error no_std build failure.
- Dockerfile: added `apk upgrade --no-cache` to clear Alpine CVEs.
- ROADMAP.md: added `text` language specifier to fenced code block (MD040).
- Added `.vscode/settings.json` to suppress false-positive schema warnings on RavenFabric playbook YAML files.

### Added

- README: Global Fleet section — relay cluster config, cross-region forwarding protocol (`FORWARD:<url>|<inner>`), deny-by-default `ForwardConfig`, region-aware orchestration.

## [0.24.0] — 2026-05-23

### Added

- **Region-aware orchestration** — `[agent] region = "eu-west"` in `raven.toml`; region propagates through `RpcResult::StatusInfo` wire protocol; `AgentInfo` registry stores region; `AgentRegistry::select_by_region()` for case-insensitive fleet filtering; `rf status` CLI prints `Region:` line when reported
- **Regional relay clusters** — `RelayCluster` config type groups relays by region with continent, country, and lat/lon metadata; `RelaySelector::from_clusters()` factory; `RelaySelector::best_in_region()` picks the best relay within the same region/continent; `[[transport.relay_clusters]]` TOML config block
- **Cross-region routing** — `FORWARD:<target_relay_url>|<inner_token>` forwarding protocol enables agents on different relays to communicate; `ForwardConfig` (deny-by-default, optional `forward_allowlist`); `run_relay_full()` with `ForwardConfig`; `bridge_to_remote_relay()` for bidirectional encrypted stream bridging without relay decrypting payload
- **maxminddb 0.27.3 upgrade** — updated GeoIP lookups to use new `LookupResult.decode::<T>()` API

### Security

- Cross-region forwarding disabled by default; requires explicit `allow_forwarding = true` in relay config
- Forwarding allowlist enforced before opening any remote relay connection
- Inner token hashed via SHA-256 in audit log entries; relay never decrypts end-to-end Noise payload

### Total

~72,767 LOC, 1,432 tests across 14 crates.

## [0.23.0] — 2026-05-22

### Added

- **HSM/PKCS#11 key provider** (`HsmKeyProvider` in `rf-crypto`, feature `hsm`) — X25519 identity key stored on hardware security module (YubiHSM2, AWS CloudHSM, SoftHSM2, etc.) with non-extractable private key; DH operations delegated to HSM via dedicated worker thread (`SyncSender` channel, fully `Send + Sync` without `unsafe`)
- **`HsmSnowResolver` + `HsmSnowDh`** — custom snow `CryptoResolver` that uses the HSM for Noise XX static key DH and software `x25519-dalek` for ephemeral keys; FIPS mode support (hard error on HSM unavailability)
- **Graceful fallback** — `HsmKeyProvider::open_with_fallback()` falls back to file-based `StaticKey` when HSM is unreachable (FIPS mode blocks this)
- **TPM 2.0 key storage** (`TpmKeyStore` + `TpmAttestation` in `rf-crypto`, feature `tpm`) — seal/unseal identity key to current PCR bank, TPM2_Quote attestation with freshness nonce, measured boot PCR verification
- **`SealedKeyBlob`** — serialisable blob (JSON/msgpack) for persisting TPM-sealed keys alongside agent configuration
- **GeoIP database integration** (`rf-relay::geoip`, feature `geoip`) — MaxMind GeoLite2/GeoIP2 database reader, `Region` struct with coordinates, Haversine great-circle distance
- **Region-aware relay selection** (`rf-transport::relay_select`) — `RelaySelector` with four strategies: nearest-by-geo, lowest-RTT, latency-weighted geo-distance score (`0.7×RTT + 0.3×distance`), and continental affinity (`multi_relay_affinity()`)
- `RelayEndpoint` builder type with continent, country, coordinates, RTT, and weight metadata
- 34 new tests across `rf-crypto` (HSM/TPM) and `rf-transport` (relay selection)

## [0.22.0] — 2026-05-22

### Added

- **Staged rollout coordinator** (`RolloutCoordinator` in `rf-executor`) — canary (1 agent) → percentage → fleet stages with automatic batching
- **Health-check gates** — rollout only advances to the next stage when all updated agents pass `RolloutHealthCheck`
- **Rollout pause/abort** — controller can halt a rollout mid-flight and resume or abandon it
- **`RolloutHealthCheck` RPC action** — per-agent health verification post-update (uptime, version reporting)
- **`SetAlertWebhook` RPC action** — configure a webhook URL for update failure alerts
- **Update failure webhook alerts** — JSON POST on download failure or rollback with event, agent_id, version, reason, and timestamp
- **`RolloutStrategy` and `RolloutStage` enums** in `rf-rpc` — serializable, msgpack roundtrip tested
- 18 new tests (8 rollout coordinator, 2 webhook, 8 rf-rpc roundtrip)

### Total

~69,907 LOC, 1,386 tests across 14 crates.

## [0.21.0] — 2026-05-21

### Added

- **Multi-agent load balancing** — `rf-ingress` routing table now supports multiple
  agents registered under the same route (subdomain + path prefix combination).
  Requests are distributed round-robin across all healthy candidates.
- **Sticky sessions** — Optional session affinity by caller identity (hashed API
  key or `"anonymous"`). Affinity is maintained for 1 hour (TTL refreshed on
  each hit) and evicted automatically when the pinned agent deregisters.
- **Version pinning** — New `PinVersion` / `UnpinVersion` RPC actions allow a
  controller to pin a specific agent to a version and prevent auto-updates.
- **Update windows** — New `SetUpdateWindow` RPC action configures a daily
  maintenance window in `"HH:MM-HH:MM"` format (24h, midnight-crossing supported).
  `UpdateAgent` enforces the window — updates outside it are rejected with an
  audit entry.
- **`GetVersionInfo` RPC action** — Returns current version, pinned version (if
  any), and configured update window for an individual agent. Enables per-agent
  fleet version visibility from the controller.
- **Fleet coordination roundtrip tests** — 7 new msgpack roundtrip tests for all
  new `Action` and `RpcResult` variants.
- **Ingress load-balancer tests** — 4 new tests: round-robin distribution,
  sticky session affinity, per-caller isolation, deregister evicts sticky pins.

### Changed

- `RoutingTable::resolve()` now delegates to the new
  `resolve_with_affinity(host, path, caller_identity)` method. Backward-compatible.
- `handle_update_agent` checks version pin and update window before downloading
  the new binary, returning `UpdateFailed` with an audit entry if blocked.

### Total

~69,055 LOC, 1,368 tests across 14 crates.

## [0.20.0] — 2026-05-23

### Added

- **Ingress audit logging** — `rf-ingress` now emits structured JSON-lines audit
  entries for every proxy request (rate-limit deny, auth deny, no-route deny, and
  success). Configurable via `IngressConfig::audit_path`; defaults to no-op when
  not set.
- **Agent auto-update mechanism** — New `rf-executor::updater` module implements
  HTTPS-only binary download with SHA-256 integrity verification, atomic binary
  swap (`.new` → target), backup/rollback (`.bak`), and process restart via
  `exec()` on Unix or spawn+exit on Windows.
- **`CheckUpdate` and `UpdateAgent` RPC actions** — New wire-protocol actions
  allow a controller to trigger update checks and push a new agent binary version.
  Responses: `UpdateAvailable`, `UpdateNotAvailable`, `UpdateApplied`,
  `UpdateFailed`. Optional `ed25519_sig` field for future signature verification.
- **Update audit entries** — Every `check-update` and `update-agent` RPC action
  produces a structured audit entry recording decision, version, URL, and outcome.

### Changed

- Total: ~68,459 LOC, 1,357 tests across 14 crates.

## [0.19.0] — 2026-05-22

### Added

- **rf-ingress HTTP gateway** — New `rf-ingress` binary (14th crate in the
  workspace). Accepts inbound HTTP requests, authenticates them via `X-RF-Key`
  API key header, applies per-IP sliding-window rate limiting (configurable RPM),
  resolves the target agent from a live routing table, and reverse-proxies the
  request to the registered local upstream URL. Routing supports subdomain
  matching, path-prefix matching, and single-agent catch-all mode. All forwarded
  requests strip hop-by-hop headers. A `/health` endpoint bypasses authentication.
  New crate modules: `server`, `router`, `auth`, `rate_limit`.
- **ReverseProxy RPC action** — New `Action::ReverseProxy` and
  `RpcResult::ReverseProxyResponse` wire types in rf-rpc. Enables the ingress
  server to forward HTTP requests to agent-local upstreams via the authenticated
  RPC channel. The rf-executor `handle_reverse_proxy` handler applies HTTP
  method+path policy checks, enforces configurable timeout and response-size
  limits, and emits structured audit log entries for every forwarded request.
- **IngressRegister RPC action** — New `Action::IngressRegister` and
  `RpcResult::IngressRegistered` wire types for the agent-to-ingress registration
  handshake. Flows through the normal policy/audit pipeline.
- **reqwest now unconditional in rf-executor** — Previously gated behind the
  `secret-backends` feature; now always available to support `ReverseProxy`.

### Changed

- **Version**: Bumped to 0.19.0 across all crates and deploy manifests
- **Total tests**: 1,343 (up from 1,328) — +4 rf-rpc roundtrips, +11 rf-ingress
  (router, auth, rate_limit, server unit tests)
- **LOC**: ~67,887 (up from ~66,857)
- ROADMAP items marked complete: rf-ingress HTTP gateway, ReverseProxy RPC,
  IngressRegister RPC

### Added

- **Delta/incremental sync for `rf cp`** — New `--delta` flag on `rf cp` enables
  rolling-checksum-based sync: the CLI queries the remote file's per-block
  Adler-32 + SHA-256 fingerprints (`FileDeltaQuery` RPC action), computes local
  block hashes inline, and transmits only the changed blocks (`FileDeltaPatch`
  RPC action). Unchanged blocks are reused from the file already on the agent.
  The agent verifies the full-file SHA-256 checksum after reconstruction and
  performs an atomic write via temp-file + rename. If the remote file is missing,
  `--delta` automatically falls back to a full push. New types: `BlockInfo`,
  `DeltaPatch`; new RPC actions: `FileDeltaQuery`, `FileDeltaPatch`; new RPC
  results: `FileDeltaIndex`, `FileDeltaApplied`. 13 new tests (6 in rf-rpc
  roundtrips, 7 in rf-executor handlers).

### Changed

- **Version**: Bumped to 0.18.0 across all crates and deploy manifests
- **Total tests**: 1,328 (up from 1,316) — +6 rf-rpc roundtrips, +7 rf-executor delta sync
- **LOC**: ~66,857 (up from ~65,914)
- ROADMAP item marked complete: Delta/incremental sync for `rf cp`

## [0.17.0] — 2026-05-21

### Added

- **Fleet-wide secret push** — New `SealSecret` RPC action pushes a plaintext secret value over the Noise-encrypted channel and seals it on the agent. Supports zero-downtime rotation via `grace_period_secs`: when > 0 and the secret already exists, the old value remains valid during roll-over. Returns `SecretSealed { name, value_hash, rotated }` — the value itself is never written to audit logs.
- **Secret enumeration** — New `ListSecrets` RPC action returns the sorted names of all secrets currently held in the agent's secret store (`SecretsList { names }`). Values are never returned.
- **`rf secret` CLI commands** — `rf secret push --token <tok> --name <name> --value <val> [--grace-period <secs>]` seals a secret on a remote agent. `rf secret list --token <tok>` lists secret names. Both commands use the existing Noise-authenticated channel.
- **External secret backends** — `rf-executor::secret_backends` provides pluggable secret manager integrations: HashiCorp Vault (AppRole and Token auth, KV v1/v2), AWS Secrets Manager (SigV4 signing, optional session token), Azure Key Vault (client credentials OAuth2), GCP Secret Manager (pre-obtained access token, base64-decoded payload), and a Generic HTTP backend (configurable URL template, JSON path extraction, custom headers). `SecretBackend` trait (`fetch`, `write`, `backend_type`) is `async`, `Send + Sync`, and dyn-compatible via `async-trait`. `SecretBackendRegistry` stores named backends. `build_backend()` factory from JSON config. Background sync task for periodic secret refresh (source-of-truth pull mode). New RPC actions: `ConfigureSecretBackend` and `FetchFromBackend`. New RPC results: `SecretBackendConfigured` and `SecretFetched`. 25 new tests.
- **CI test timeout** — `.github/workflows/ci.yml` Test job now has `timeout-minutes: 45` to prevent indefinite hangs.
- **Comprehensive documentation guides** — 9 new how-to guides covering all major features: Secret Management, File Transfer, Port Forwarding, Desired-State Convergence, Fleet Orchestration, Mesh VPN & DTN, SIEM Integration, Anomaly Detection, Post-Quantum Keys. Updated `SUMMARY.md` navigation and `reference/cli.md` with `rf secret push/list` and `rf cp` command references.

### Fixed

- **`BufferedAuditCollector` worker race condition** — Fixed a "notify before wait" race where `Drop` could send `notify_all()` before the worker thread entered `cvar.wait_timeout`, causing `w.join()` to block for the full `flush_interval` (up to 60s in tests). Worker now checks the stop flag before waiting; if already set, it drains the buffer and exits immediately. Both `Drop` (best-effort) and `flush_and_stop()` (explicit drain) work correctly.
- **`DatadogAuditLogger` test deadlock** — Added `with_intake_url()` override to `DatadogConfig` bypassing the `https://http-intake.logs.{site}/api/v2/logs` URL template. Tests that used `with_site("127.0.0.1:port")` would produce an unresolvable hostname (`http-intake.logs.127.0.0.1`), causing `TcpStream::connect_timeout` to fail silently and a test listener thread to block on `listener.accept()` indefinitely.

### Changed

- **Version**: Bumped to 0.17.0 across all crates and deploy manifests
- **Total tests**: 1,316 (up from 1,283) — +5 rf-executor (SealSecret/ListSecrets handlers), +5 rf-rpc (new action/result types), +25 rf-executor (secret backends), +4 rf-rpc (ConfigureSecretBackend/FetchFromBackend roundtrips)
- **LOC**: ~65,914 (up from ~63,769)
- ROADMAP items marked complete: SealSecret/ListSecrets, rf secret CLI, Vault/AWS/Azure/GCP/Generic HTTP secret backends, sync mode, CI timeout

## [0.15.0] — 2026-05-21

### Added

- **Streaming file transfer (`FilePushStream` / `FilePullStream`)** — `rf cp` now uses a streaming protocol instead of per-chunk RPC round-trips. Upload: one negotiation request (`FilePushStream`) → `FileStreamReady` → raw `SecureChannel` frames → `FileStreamDone`. Download: one negotiation request (`FilePullStream`) → `FileStreamReady { total_size, checksum }` → raw frames until `total_size` bytes received. Eliminates N round-trips for N chunks — a single negotiation followed by a continuous byte stream. Full policy enforcement, SHA-256 checksum verification, atomic rename on the agent side, and complete audit logging. 4 new roundtrip tests in `rf-rpc`.

### Changed

- **Version**: Bumped to 0.15.0 across all crates and deploy manifests
- **Total tests**: 1,283 (up from 1,279) — +4 rf-rpc (stream action/result roundtrip serialization)
- **LOC**: ~63,769 (up from ~63,229)

### Fixed

## [0.14.0] — 2026-05-21

### Added

- **Policy-gated MCP tool endpoints (RBAC `allowed_tools`)** — `CallerProfile` now accepts an optional `allowed_tools: Vec<String>` field. When set, `tools/list` returns only the permitted subset, and `tools/call` rejects any tool not in the list with a structured error. Empty list (default) preserves existing behaviour — all tools available. `tool_list_capabilities` now reports `caller`, `allowed_tools`, and `tool_restriction_active` so AI agents can discover their effective permissions. New `list_tools_filtered(allowed: &[String])` + `all_tool_names()` helpers in `tools.rs`. 11 new tests covering filtered listing, call rejection, call allowance, and capabilities reporting.

### Changed

- **Version**: Bumped to 0.14.0 across all crates and deploy manifests
- **Total tests**: 1,279 (up from 1,268) — +11 rf-mcp-server (RBAC tool gating)
- **rf-mcp-server LOC**: ~3,400+ (up from ~3,300)

### Fixed

- Dependabot ignore rule added for `sysinfo >= 0.39` (requires rustc 1.95; MSRV is 1.88)

## [0.13.0] — 2026-05-20

### Added

- **Concurrent proxy tunnels (dedicated-connection model)** — `rf-cli` `proxy` command now opens a dedicated agent connection per incoming local TCP connection, enabling truly concurrent tunnels without head-of-line blocking. Each local connection spawns a task that: dials the relay, performs Noise XX handshake, sends `Action::ProxyOpen { target, idle_timeout_secs, max_duration_secs }`, waits for `RpcResult::ProxyReady`, then enters raw bidirectional forwarding mode using two concurrent tasks (`tcp_r → chan.send` / `chan.recv → tcp_w`). HTTP-aware mode also upgraded: each HTTP request creates its own dedicated agent connection. `Action::ProxyOpen` and `RpcResult::ProxyReady` added to `rf-rpc::types`. Agent intercepts `ProxyOpen` before the executor, establishes a raw forwarding loop with `run_proxy_tunnel`, and audits open/close with bytes transferred.
- **Secret rotation TTL/hooks** — `rf-crypto::secrets::RotationConfig` struct: configurable `ttl: Duration`, `hook: Option<String>` (shell command whose stdout becomes new secret value), `grace_period: Duration` (old value remains valid during overlap window), `health_check: Option<String>` (must exit 0 before old value is retired). New `SecretStore` methods: `seal_with_rotation()`, `rotate()`, `needs_rotation()`, `unseal_with_grace()`, `set_rotation_config()`, `rotation_config()`. Grace-period fallback integrated into `resolve_template()` so in-flight template expansions survive zero-downtime rotation. `RotationConfig::is_expired()`, `in_grace_period()`, `ttl_remaining_secs()` helpers. 8 new tests.
- **`Action::RotateSecret` RPC action** — manually trigger rotation for a named secret. If the secret has a rotation hook, the hook is run and its stdout is sealed as the new value. Optional health-check command (`RF_NEW_SECRET` env var) must exit 0 before the rotation is committed. Responds with `RpcResult::Rotated { name, new_value_hash, ttl_secs, grace_period_secs }`. Full audit trail.
- **`Action::SetSecretRotation` RPC action** — attach or replace the rotation policy for an existing sealed secret without re-sealing the value. Responds with `RpcResult::RotationConfigured { name, ttl_secs }`. Full audit trail.

### Changed

- **Version**: Bumped to 0.13.0 across all crates and deploy manifests
- **Total tests**: 1,268 (up from 1,257) — +8 rf-crypto (rotation), +2 rf-executor (rotation handlers), +1 rf-rpc (ProxyOpen roundtrip)
- **LOC**: ~62,819 (up from ~61,878)
- ROADMAP items marked complete: Concurrent tunnels, Secret rotation TTL/hooks, Rotation hooks, Grace period, Health-check after rotation

### Added

- **Elasticsearch/OpenSearch audit destination** — new `ElasticsearchAuditLogger` in `rf-audit::elasticsearch`. Indexes each `AuditEntry` into an Elasticsearch or OpenSearch cluster via the Bulk API (`POST /_bulk`). Uses NDJSON format: each event is two lines (action metadata + document body). Auth options: `ElasticAuth::None`, `ElasticAuth::Basic { username, password }` (HTTP Basic with inline Base64 encoder), and `ElasticAuth::ApiKey(key)` (`Authorization: ApiKey <key>` header). Configurable index name (default: `"ravenfabric"`), batch size (default: 1), and HTTPS default port (9243 for Elastic Cloud, 9200 for HTTP). Batch queue with `Drop` flush. 11 new tests.
- **Datadog log forwarding audit destination** — new `DatadogAuditLogger` in `rf-audit::datadog`. Forwards each `AuditEntry` to the Datadog Logs Intake API (`POST /api/v2/logs`). Authenticates via `DD-API-KEY` header. Log entries include `ddsource` (`"ravenfabric"`), `ddtags` (`service:ravenfabric` + custom tags), `hostname`, `service`, and `message` (JSON-serialized `AuditEntry`). Configurable site (`datadoghq.com`, `datadoghq.eu`, `us3.datadoghq.com`, etc.), service name, hostname, custom tags, and batch size (default: 10). Batch queue with `Drop` flush. 8 new tests.
- **Buffered audit collector with deduplication** — new `BufferedAuditCollector<L>` in `rf-audit::collector`. Generic wrapper around any `AuditLogger` adding: bounded in-memory ring buffer (configurable capacity, default 4,096); background flush thread draining buffer at configurable interval (default 5s); sliding-window deduplication by `request_id` (default window: 1,024 entries); age-based retention (default: 24h, entries older than `max_age` are silently discarded before forwarding). When buffer is full, oldest entry is evicted with a `warn` log. `flush_and_stop()` drains all buffered events and joins the background thread. `CollectorConfig` builder with `with_flush_interval()`, `with_dedup_window()`, and `with_max_age()`. 6 new tests.

### Changed

- **Version**: Bumped to 0.12.0 across all crates and deploy manifests
- **Total tests**: 1,257 (up from 1,232) — +25 rf-audit (11 elasticsearch + 8 datadog + 6 collector)
- **LOC**: ~61,878 (up from ~60,548)
- `rf-audit::lib` now exports `collector`, `datadog`, and `elasticsearch` modules

## [0.11.0] — 2026-05-20

### Added

- **LEEF (Log Event Extended Format) audit formatter** — new `LeefAuditLogger<L>` in `rf-audit::leef`. Generic wrapper around any `AuditLogger`. Supports LEEF 1.0 and 2.0 via `LeefVersion` enum (default: V2). LEEF 1.0 header: `LEEF:1.0|Vendor|Product|Version|EventID|attrs`; LEEF 2.0 adds a Label field: `LEEF:2.0|Vendor|Product|Version|EventID|Label|attrs`. Tab-delimited attributes: `devTime`, `requestId`, `act`, `outcome`, `sev`, `src`, `duration`, `matchedRule`, and optionally `cmd`, `exitCode`, `reason`. Proper LEEF escaping (`|`, `\`, `=`) in both header and attribute values. `new()` constructor (defaults to V2) and `with_version()` to select format. IBM QRadar compatible. 9 new tests.
- **OCSF (Open Cybersecurity Schema Framework) audit formatter** — new `OcsfAuditLogger<L>` in `rf-audit::ocsf`. Converts each `AuditEntry` to OCSF 1.1.0 schema, class_uid 6003 (Application Activity). Types: `OcsfMetadata`, `OcsfProduct`, `OcsfActor`, `OcsfUser`, `OcsfEvent`. Maps audit decisions to OCSF severity codes (0–6) and status codes (0=Unknown, 1=Success, 2=Failure). Public functions: `to_ocsf_event()` and `format_ocsf()`. Serialization errors logged at `warn` level, never surfaced as errors. 12 new tests.
- **Splunk HEC audit destination** — new `SplunkHecAuditLogger` in `rf-audit::splunk`. Sends audit entries to a Splunk HTTP Event Collector endpoint with token-based authentication. `SplunkHecConfig`: configurable `url`, `token`, `index` (default `"main"`), `batch_size` (default 1). `HecPayload`: `time` (f64 Unix epoch), `source` (`"ravenfabric"`), `sourcetype` (`"rf:audit"`), `index`, `event`. Batch queue (`Arc<Mutex<Vec<String>>>`): accumulates serialized events and flushes at `batch_size`. `Drop` impl: flushes any remaining buffered events on drop; send failures logged at `warn`. 8 new tests.

### Changed

- **Version**: Bumped to 0.11.0 across all crates and deploy manifests
- **Total tests**: 1,232 (up from 1,203) — +29 rf-audit (9 leef + 12 ocsf + 8 splunk)
- **LOC**: ~60,548 (up from ~59,448)
- `rf-audit::lib` now exports `leef`, `ocsf`, and `splunk` modules

## [0.10.0] — 2026-05-20

### Added

- **Syslog RFC 5424 audit destination** — new `SyslogAuditLogger` in `rf-audit::syslog`. Implements `AuditLogger` and sends each entry as an RFC 5424 syslog message to a remote server via UDP or TCP. UDP variant reuses a single bound socket (fire-and-forget, no error propagation). TCP variant maintains a persistent connection with 5-second timeouts and reconnects on drop; uses RFC 6587 octet-counting framing. Facility (`SyslogFacility`: Kernel, User, Daemon, Auth, Local0–Local7) and severity (`SyslogSeverity`: Emergency through Debug) are configurable. Priority = `facility * 8 + severity`. Structured data includes `request_id`, `decision`, `matched_rule`, `caller_key`, `duration_ms`. Delivery failures are logged at `warn` level, never surfaced as errors. New constructors: `SyslogAuditLogger::udp()` and `SyslogAuditLogger::tcp()`. 6 new tests.
- **CEF (Common Event Format) audit wrapper** — new `CefAuditLogger<L>` in `rf-audit::cef`. Generic wrapper around any `AuditLogger`. Converts each `AuditEntry` to a CEF-formatted line (`CEF:0|RavenFabric|RavenFabric|version|class_id|name|severity|extension`) before forwarding to the inner logger. CEF severity maps from audit decision (`denied→8`, `error→7`, `allowed→3`, other→`5`). Extension fields include `rt` (epoch ms), `requestId`, `act`, `outcome`, `dvcpid`, `reason`, `duration`, `cs1Label=matchedRule`, `cs1`, `exitCode`, `suser`. Proper CEF escaping: `|` and `\` in header fields; `=`, `\`, `\n`, `\r` in extension values. Compatible with ArcSight, Splunk, IBM QRadar, and other SIEM systems. 8 new tests.
- **Rotation audit trail** — `TrustStore` now maintains an in-memory append-only rotation log (`Vec<RotationEvent>`). New types: `RotationEventType` (`Rotate` / `Revoke`) and `RotationEvent` (`timestamp`, `agent_id`, `event_type`, `old_key_hash`, `new_key_hash`, `version`). `rotate_key()` appends a `Rotate` event with the hex digest of both old and new keys, plus the new version number. `revoke_immediate()` appends a `Revoke` event with the old key hash and current version. New `TrustStore::rotation_history()` method returns the full audit trail. 3 new tests: `test_rotation_audit_trail_rotate`, `test_rotation_audit_trail_revoke`, `test_rotation_audit_trail_multiple_events`.

### Changed

- **Version**: Bumped to 0.10.0 across all crates and deploy manifests
- **Total tests**: 1,203 (up from 1,186) — +14 rf-audit (syslog + CEF), +3 rf-bootstrap (rotation trail)
- **LOC**: ~59,448 (up from ~58,611)
- `rf-audit::lib` now exports `cef` and `syslog` modules

## [0.9.0] — 2026-05-20

### Added

- **zstd compression for file transfer** — `FilePush` and `FilePull` RPC actions now accept an optional `compress: bool` field. When enabled, the client sends compressed chunks (FilePush) or the agent returns compressed chunks (FilePull) using zstd level-3 compression. The `FileChunk` response includes a `compressed` flag so the receiver knows whether to decompress. Transparent and policy-neutral — compression is negotiated per-request. 2 new executor tests: `test_file_push_compress`, `test_file_pull_compress`.
- **Secret versioning in TrustStore** — `TrustedAgent` now tracks `version: u32` (starts at 1, incremented on each key rotation), `key_history: Vec<String>` (all previous public keys, oldest first), `revoked: bool`, and `revoked_at: Option<String>` (RFC 3339 timestamp). New `rotate_key(old, new)` method atomically increments version and archives the old key.
- **Emergency revocation** — new `TrustStore::revoke_immediate(public_key)` method immediately marks an agent as revoked (`revoked=true`, `revoked_at=<now>`). `is_trusted()` returns `false` for revoked agents instantly without removing the entry from the store (preserved for audit). 4 new bootstrap tests: `test_is_trusted_false_when_revoked`, `test_revoke_immediate_sets_fields`, `test_revoke_immediate_unknown_key`, `test_rotate_key`, `test_key_history_after_rotation`.

### Changed

- `TrustStore::is_trusted()` now returns `false` for revoked agents (previously only checked key presence).

## [0.8.0] — 2026-05-20

### Added

- **Generic webhook audit log forwarding** — new `WebhookAuditLogger` in `rf-audit::logger`. Wraps any existing `AuditLogger` (e.g., `FileAuditLogger`) and forwards each audit entry to a remote HTTP endpoint via an asynchronous fire-and-forget HTTP POST. Payload is a JSON-serialized `AuditEntry` (same format as JSON-lines files). Supports `http://host:port/path` scheme. Connection and write failures are logged at `warn` level and never surface as errors to the caller. 5 new tests: `test_webhook_audit_logger_delivers_entry` (TCP listener verifies HTTP POST), `test_webhook_audit_logger_delegates_to_inner` (file logger also receives entry), `test_parse_webhook_url_with_port_and_path`, `test_parse_webhook_url_default_port`, `test_parse_webhook_url_invalid`.

### Changed

- **Version**: Bumped to 0.8.0 across all crates and deploy manifests
- **Total tests**: 1,179 (up from 1,174)
- **rf-audit**: +5 tests (32 total); `WebhookAuditLogger`, `parse_webhook_url()`, `post_audit_webhook()` added to `logger` module

## [0.7.0] — 2026-05-20

### Added

- **Alert webhook destinations** — `AlertRule::with_webhook(url)` builder method. When an alert fires (and is not suppressed by deduplication), an asynchronous HTTP POST is dispatched to the configured URL with a JSON payload containing `rule`, `action`, `decision`, `request_id`, `matched_rule`, `command`, and `timestamp`. Delivery is fire-and-forget; failures are logged at `warn` level. Supports `http://host:port/path` scheme. 4 new tests in rf-audit: `test_webhook_url_configured`, `test_no_webhook_by_default`, `test_webhook_delivered_on_alert` (binds a TCP listener and verifies the HTTP POST arrives), `test_webhook_not_fired_when_deduped` (verifies suppressed alerts produce no network traffic).

### Changed

- **Version**: Bumped to 0.7.0 across all crates and deploy manifests
- **Total tests**: 1,174 (up from 1,170)
- **rf-audit**: +4 tests (27 total); `AlertRule` gains `webhook_url` field and `with_webhook()` builder

## [0.6.0] — 2026-05-20

### Added

- **HTTP rate limiting per destination** — `maxHttpRequestsPerWindow` and `httpRateLimitWindowSecs` fields in policy `resources` section. When set, limits the number of `HttpForward` requests to each upstream target within the configured window. Tracked per-destination in a sliding-window timestamp queue (`VecDeque<Instant>`) stored in the executor. Returns `RpcResult::Denied` with `rule: resources.maxHttpRequestsPerWindow` when exceeded. 0 (default) means unlimited. 2 new tests in rf-executor, 2 new tests in rf-policy.
- **Bandwidth throttling for file transfer** — `maxTransferBytesPerSec` field in policy `resources` section (default 0 = unlimited). Enforced on both `FilePush` (per chunk, after write) and `FilePull` (per chunk, after read). Uses elapsed-time pacing: if chunk transfer was faster than the configured rate, sleeps the remainder of the expected duration. 2 new tests in rf-policy, 2 new tests in rf-executor.

### Changed

- **Version**: Bumped to 0.6.0 across all crates and deploy manifests
- **Total tests**: 1,170 (up from 1,164)
- **rf-policy**: +2 tests (140 total); 3 new fields in `RpcPolicy`/`ResourceSpec`
- **rf-executor**: +4 tests (173 total); rate-limit state in `Executor`, throttle logic in `handle_file_push`/`handle_file_pull`

## [0.5.0] — 2026-05-20

### Added

- **Header injection/stripping policy** — `http.headers.require` and `http.headers.forbid` lists in policy YAML. Required headers must be present on every HTTP request; forbidden headers must not appear. Enforced by `check_http_headers()` in rf-policy, called from `handle_http_forward()` in rf-executor before connecting to the upstream. Case-insensitive header name matching. 6 new tests in rf-policy.
- **Real-time alert rules** — `AlertRule` and `AlertEngine` in new `rf-audit::alert` module. Rules match audit entries by pattern (regex applied to `action`, `decision`, and `command`). Matching rules emit `tracing::warn!` structured alerts. Alert deduplication: same rule+action pair suppressed within a configurable window (`dedup_window_secs`). 9 new tests in rf-audit.
- **Glob pattern expansion for `rf cp`** — `rf cp "/var/log/*.gz" agent:/backup/` expands glob patterns locally before uploading. Multiple matched files are pushed individually; destination directory is derived per-file. Powered by the `glob` crate. Invalid patterns and zero-match globs return a clear error.

### Changed

- **Version**: Bumped to 0.5.0 across all crates and deploy manifests
- **Total tests**: 1,164 (up from 1,149)
- **rf-audit**: +9 tests (23 total); added `glob` dependency to workspace

## [0.4.0] — 2026-05-16

### Added

- **MCP tool: `rf_http_request`** — AI agents can call private APIs through RavenFabric with full policy enforcement. Supports GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS. Structured JSON response with `status_code`, `headers`, `body` (auto-parsed as JSON when `Content-Type: application/json`), and `latency_ms`. Method validated against allowlist. 4 new tests in rf-mcp-server.
- **File transfer size limits** — `maxFileSizeBytes` field in policy `resources` section (default 100 MB). Enforced on both `FilePush` (checked per chunk: `offset + chunk_size`) and `FilePull` (checked against file metadata before reading). Returns `RpcResult::Denied` with `rule: resources.maxFileSizeBytes` when exceeded. 2 new tests in rf-executor, 2 new tests in rf-policy.

### Changed

- **Version**: Bumped to 0.4.0 across all crates and deploy manifests
- **Total tests**: 1,149 (up from 1,141)

## [0.3.0] — 2026-05-14

### Added

- **HTTP-aware proxy mode** — `rf proxy --http` enables per-request HTTP policy enforcement. Agent parses method, path, headers via `httparse` before forwarding to upstream. New `HttpForward` RPC action and `HttpResponse` result type. HTTP policy rules in YAML (`http.allow`/`http.deny` by method + path regex). Request/response body size limits (`maxRequestBodyBytes`/`maxResponseBodyBytes`). Per-request audit logging with method, path, status code, latency. Header injection detection prevents CRLF attacks. 10 new tests.
- **Proxy idle timeout + max duration** — Configurable per-connection limits prevent resource exhaustion from abandoned tunnels. Policy defaults (`proxyIdleTimeoutSeconds: 300`, `proxyMaxDurationSeconds: 3600`) with per-request override via `--idle-timeout` and `--max-duration` CLI flags. Effective values returned in `ProxyConnected` response. 2 new tests.
- **Network policy rules for proxy targets** — `check_network_target()` enforces CIDR/hostname/port allow/deny rules on TCP proxy connections. Immutable deny blocks link-local/metadata addresses (169.254.0.0/16, fe80::/10). IPv4, IPv6 (bracket notation), port ranges, hostname globs (`*.internal.com`), deny-by-default. 14 new tests.
- **MCP tool: `rf_file_transfer`** — AI agents can copy files on-agent with full policy enforcement, integrity verification (SHA-256), and audit logging. 9th MCP tool.
- **Recursive directory transfer** — `rf cp -r` flag for uploading entire directory trees to agents with automatic file enumeration
- **Bulk file transfer** — `FilePush` and `FilePull` RPC actions with chunked transfer, SHA-256 integrity verification, atomic writes (temp + rename), resumable offset tracking, and full policy enforcement
- **TCP proxy tunneling** — `Proxy` RPC action with policy-checked TCP connect, `rf proxy` CLI command for local listener tunneling through agent to remote targets
- **`rf cp` CLI command** — familiar scp-like syntax (`rf cp local agent:/path` and `rf cp agent:/path local`) with progress reporting and checksum verification
- **Release: rf-mcp-server in binaries** — MCP server now included in release artifacts for all platforms (was previously source-only). Install script downloads all 4 binaries: rf-agent, rf-relay, rf, rf-mcp-server
- **v0.3.0 features shipped** — QUIC, WireGuard direct, STUN hole-punching, interactive shell, port-forwarding, playbooks, full mesh VPN, MagicDNS, sealed secrets, Reticulum, Tor, serial drivers — all confirmed code-complete and tested

### Fixed

- **install.sh**: Fixed "unbound variable" error when piped to bash (`curl | bash`). The EXIT trap referenced a local variable (`tmpdir`) that was out of scope after `main()` returned. Moved to global cleanup function
- **Release workflow**: Added `rf-mcp-server` build step and packaging — previously only rf-agent, rf-relay, and rf (CLI) were included in release artifacts

### Changed

- **Version**: Bumped to 0.3.0 across all crates, deploy manifests, SDKs, and documentation

## [Unreleased]

### Changed

- **website**: Migrated hosting from GitHub Pages to Cloudflare Pages (builds directly from GitHub). Security headers (`_headers`) now served natively. HSTS preload active. Removed `website/CNAME` and `.github/workflows/pages.yml`

### Added

- **Security audit tests**: 17 new integration tests covering key zeroization, OTP replay prevention, policy bypass, wire protocol rejection, and codec stability (`security_audit.rs`)
- **Wire protocol stability doc**: Formal stability guarantees for wire format, handshake sequence, and RPC serialization (`docs/src/reference/wire-protocol-stability.md`)
- **API stability markers**: `#[non_exhaustive]` on core public enums (`CryptoError`, `RpcError`, `PolicyError`, `Action`, `RpcResult`) — prevents downstream breakage when new variants are added

### Security

- **wasmtime**: Upgraded from v29 to v36.0.9, resolving all 16 security advisories (GHSA-q8hx-mm92-4wbg, GHSA-4w5q-m7x3-bxgf, GHSA-v39r-r8gw-p945, GHSA-5wvc-xrjx-h2xq, GHSA-5jmc-43q8-x28q, GHSA-7mpv-9xg9-5jx4, GHSA-75hq-h9g9-4gjr, GHSA-5wgq-hcmq-3rf7, GHSA-5j3r-j6x2-23x2, GHSA-cx96-5vf6-8x3f, GHSA-34ch-7c68-q6x6, GHSA-rj3g-829c-8jpc, GHSA-pp24-53gm-jr4j, GHSA-4x44-w425-m2p3, GHSA-jcr4-92f4-r3jm, GHSA-w2mj-m73j-q22c). v36.0.9 is optimal: MSRV-compatible (requires Rust 1.86, project uses 1.88) and covers all backported fix ranges

### Fixed

- **deps**: Migrated rf-executor, rf-policy, rf-rpc from `sha2 = "0.10"` to workspace `sha2 = "0.11"` — eliminates direct dependency version mismatch (#99)
- **code quality**: Replaced 2 production `unwrap()` calls in rf-transport (overlay.rs, quic.rs) with `expect()` + justification
- **docs**: Fixed inaccurate test counts in README (rf-transport 551→542, rf-rpc 120→112, rf-mcp-client 15→14) and copilot-instructions

### Changed

- **ci**: Replaced `cargo-tarpaulin` with `cargo-llvm-cov` for more accurate coverage measurement
- **deps**: Bumped `criterion` from 0.5.1 to 0.8.2
- **deps**: Bumped `toml` from 0.8.23 to 1.1.2
- **deps**: Bumped `rusqlite` from 0.35.0 to 0.39.0
- **ci**: Bumped `docker/login-action` from 3 to 4
- **ci**: Bumped `docker/build-push-action` from 6 to 7
- **ci**: Bumped `docker/setup-qemu-action` from 3 to 4
- **ci**: Bumped `actions/upload-pages-artifact` from 3 to 5
- **ci**: Bumped `actions/deploy-pages` from 4 to 5
- **security**: Updated SECURITY.md supported versions (0.3.x is now supported, 0.2.x deprecated)

### Added

- **Direct connection mode**: Agent `--listen` flag starts a WebSocket server for point-to-point connections (like sshd). CLI `--connect` flag dials the agent directly, bypassing the relay. Same Noise XX encryption, policy enforcement, and audit logging as relay mode
- **Demos**: Direct connection demo (`demos/direct-connection/`) — 4 scenarios: direct exec, system info, policy denial, audit trail
- **Demos**: MCP/AI Agent demo (`demos/mcp-agent/`) — 6 scenarios: policy discovery, command execution, policy denial, human approval, audit trail, file operations
- **Demos**: Resilience demo (`demos/resilience/`) — 5 scenarios: agent reconnect, relay restart recovery, network partition, graceful degradation, exponential backoff visualization (4 Docker containers)
- **Demos**: Controller/Web UI demo (`demos/controller/`) — 5 scenarios: agent list, health check, remote execution via HTTP API, fleet dashboard, policy view (3 Docker containers)

### Changed

- **Docs**: README demos section expanded from 6 to 9 demos with subsections for each
- **Docs**: ROADMAP demo consolidation items resolved — multi-distro-linux and kubernetes-cnpg scenarios confirmed to contain environment-specific content (not duplicates)

## [0.2.0] — 2026-05-10

### Added

- **Web UI**: HTTP server for the embedded dashboard — binds `TcpListener`, serves dashboard HTML at `/`, routes `/api/*` through `ApiDispatcher`, bearer token auth, security headers (X-Frame-Options, X-Content-Type-Options, Cache-Control), request size limiting (1 MB), 6 tests
- **WASM Plugins**: Plugin execution runtime via `wasmtime` behind `wasm-plugins` feature flag — `WasmRuntime` loads and executes WASM modules with fuel metering, memory isolation, and the `alloc/process/result_len` host interface. Stub runtime without feature returns clear error. 2 new tests
- **SPIFFE Identity**: Full workload identity implementation — `SpiffeIdentity::new()`, `parse()`, `validate()`, `path_matches()` with wildcard support, `TrustBundle` with domain verification, `SpiffeError` type, 8 new tests
- **Controller**: HTTP server serves controller API endpoints (agent list, health check) over the network — previously in-memory only

### Changed

- **Versions**: All packaging manifests, SDKs, website, demos, and docs updated to v0.2.0
- **Stats**: 13 crates, ~53,900 LOC, 1,094 tests, 0 clippy warnings

## [0.1.6] — 2026-05-10

### Added

- **CI**: Binary size gate — new CI job builds release binaries and fails if any exceed 15 MB, enforcing the deployment size constraint documented in the architecture

### Changed

- **Versions**: All packaging manifests, SDKs, website, demos, and docs updated from v0.1.5 to v0.1.6

## [0.1.5] — 2026-05-10

### Added

- **Security**: Policy enforcement for all RPC handlers — shell open, port forwarding (local/remote/SOCKS5), health check (command probes), and log tail now go through deny-by-default policy checks
- **Security**: Audit logging for all RPC actions — every handler (metrics, status, read, write, list, signal, background exec, shell open/input/close, port forward start/close, remote forward, SOCKS5 forward, health check, tail log) now produces a structured audit entry with request ID, action, decision, matched rule, and duration
- **Security**: Policy check for `signal` action — `kill -<signal> <pid>` is now checked against command policy before execution
- **Security**: Shell session recording — every PTY session records all input/output in asciicast v2 format (NDJSON) and emits the full recording to the audit log on session close for replay-grade compliance traceability
- **Tests**: Shell session recording test — verifies open/input/close lifecycle, audit entries for each phase, asciicast v2 recording in audit with header validation
- **Tests**: Audit JSON round-trip test — verifies all audit entries serialize to valid JSON with required fields (timestamp, request_id, action, decision, matched_rule, caller_key, duration_ms)
- **Tests**: Strengthened audit assertions — existing tests now verify `action`, `command`, `request_id`, `caller_key`, and `matched_rule` fields (previously only checked `decision`)
- **Demos**: Desired-State Convergence demo — 7 scenarios covering drift detection, auto-remediation, report-only mode, grains-based targeting, event triggers, and version constraints (`demos/desired-state/`)
- **Tests**: 18 desired-state showcase integration tests — YAML parsing, 4 resource types (packages/files/services/sysctl), convergence modes, grains label matching, event bus triggers, full lifecycle (`crates/rf-integration-tests/tests/desired_state_showcase.rs`)
- **Demos**: Transport Showcase demo — 5 transports (WebSocket, QUIC, UNIX Socket, Stdio Pipe, Memory) with end-to-end encrypted command execution over each, proving transport interchangeability (`demos/transport-showcase/`)
- **Tests**: 5 transport showcase integration tests — each performs Noise XX handshake + SecureChannel + RPC execution over a different transport driver (`crates/rf-integration-tests/tests/transport_showcase.rs`)
- **Demos**: Multi-Node Ubuntu demo — 2-agent Docker setup with relay, setup/teardown scripts, 11 scenario scripts (`demos/multi-node-ubuntu/`)
- **Demos**: Multi-Distro Linux demo — 9 Linux distributions (Ubuntu, Debian, Fedora, Rocky, Manjaro, openSUSE, Alpine, Amazon Linux, Void) with setup/verify/teardown (`demos/multi-distro-linux/`)
- **Demos**: Kubernetes + CloudNativePG demo — 2-instance CNPG PostgreSQL cluster with rf-agent sidecar, Gatekeeper exemption handling, auto-detect host IP (`demos/kubernetes-cnpg/`)
- **Demos**: Asciinema recording scripts and animated SVG exports for all 3 demos (`demos/recordings/`)
- **Demos**: Policy Denial scenario for all 3 demos — restrictive policy, allowed/denied command tests, audit log inspection (`scenarios/12-policy-denial.sh`, `scenarios/policy-denial.sh`)
- **Demos**: Audit Trail scenario for all 3 demos — structured JSON-lines audit log inspection, per-agent entry counts, cross-distro format consistency (`scenarios/13-audit-trail.sh`, `scenarios/audit-trail.sh`)
- **Demos**: Port Forwarding scenario for all 3 demos — local/reverse/SOCKS5 forwarding through encrypted tunnels, cross-distro tunneling, K8s PostgreSQL port forwarding (`scenarios/14-port-forwarding.sh`, `scenarios/port-forwarding.sh`)
- **Demos**: Dev Mode (Zero-Setup) scenario for all 3 demos — single-command dev environment, relay + agent in one process, zero config (`scenarios/15-dev-mode.sh`, `scenarios/dev-mode.sh`)
- **Demos**: Fleet Orchestration scenario for all 3 demos — multi-agent playbooks with parallel/sequential/rolling/canary strategies, automatic rollback (`scenarios/16-fleet-orchestration.sh`, `scenarios/fleet-orchestration.sh`)
- **Demos**: Human Approval scenario for all 3 demos — human-in-the-loop approval gate for AI-controlled agents via MCP, operator approve/deny workflow, defense in depth (`scenarios/17-human-approval.sh`, `scenarios/human-approval.sh`)
- **MCP Server**: Approval enforcement in `tool_exec` — commands matching `--approval-pattern` regex patterns are blocked until a human operator approves via `approve()`/`deny()` API
- **MCP Server**: `--require-approval` mandatory mode — when enabled, ALL mutating operations (`rf_exec`, `rf_file_write`) require a valid approval, making bypass impossible regardless of patterns
- **MCP Server**: Approval enforcement in `tool_file_write` — file write operations require approval when `--require-approval` is set, using `write:<path>` as the command binding
- **MCP Server**: SHA-256 command hash verification — approved command is cryptographically bound to the approval, preventing command substitution attacks
- **MCP Server**: One-time-use enforcement — each approval can only be consumed once, subsequent attempts return DENIED
- **MCP Server**: RBAC enforcement fix — executor now shares the same policy Arc as the server, so caller profile policies are actually enforced during command execution (previously executor used a separate, never-updated policy)
- **MCP Server**: 30-minute TTL on approvals — expired approvals automatically return DENIED
- **MCP Server**: `approval_id` parameter added to `rf_exec` tool for passing approved approval IDs
- **Website**: Live demos page at `/demos/` with animated terminal recordings, architecture diagrams, setup instructions
- **Website**: Policy Denial section added to each demo on the website demos page
- **Website**: "Demos" navigation link added to main site, blog pages, and footer
- **Blog**: "Demo 1: Multi-Node Ubuntu" post — walkthrough of all 17 scenarios, from remote execution to human approval for AI agents
- **CLI**: `--stream` and `--background` execution flags for streaming and fire-and-forget modes

### Fixed

- **Agent/Relay/CLI**: `RUST_LOG` environment variable now correctly controls log filtering — `RUST_LOG=warn` properly suppresses INFO/DEBUG lines (#95)
- **CLI**: Added `close_notify()` after `exec` and `status` commands — agent now detects session end and reconnects cleanly instead of hanging indefinitely
- **K8s Demo**: Deployment uses `strategy: Recreate` with `terminationGracePeriodSeconds: 3` and SIGTERM→SIGINT trap to prevent dual-pod relay pairing race condition
- **Versions**: All packaging manifests, SDKs, website, Web UI, and docs updated from v0.1.4 to v0.1.5
- **Docs**: Python SDK documentation URL now points to public `ravenfabric.io/docs/` instead of private repo
- **Docs**: Contributing guide SECURITY.md link changed from absolute GitHub URL to relative path
- **Transport**: QUIC test no longer flaky — uses OS-assigned port directly instead of rebinding, eliminating address-already-in-use race condition (#96)

## [0.1.4] — 2026-05-08

### Added

- **Packaging**: macOS `.pkg` installer build script (`deploy/macos/build-pkg.sh`) — universal binary support, launchd integration, pre/post install scripts
- **Packaging**: openSUSE OBS spec file (`deploy/obs/ravenfabric.spec`) — RPM packaging for zypper/OBS, systemd integration, dedicated user/group
- **Packaging**: F-Droid metadata (`deploy/fdroid/`) — full app listing with descriptions, changelog, build recipe for aarch64 Android
- **CLI**: Added `--version` flag to `rf` CLI (reads version from Cargo.toml via clap)
- **Homebrew**: Fixed formula to use pre-built binaries from `RavenFabric-Published` with real SHA256 hashes

### Fixed

- **Website**: Removed all links to private `egkristi/RavenFabric` repo from public website
- **Website**: Hero CTA changed from "View on GitHub" to "Download Latest Release" (→ RavenFabric-Published)
- **Website**: Badges now use static shields.io (version, language, license) + Latest Release from Published repo
- **Website**: Removed GitHub card sidebar, nav GitHub button, footer repo links
- **Website**: FAQ updated — source access pending legal review, available on request
- **Docs**: Removed GitHub repo/edit links from mdBook (`book.toml`) — no more broken edit buttons
- **Docs**: Fixed `security.txt` — removed private repo advisory/policy URLs
- **Docs**: Replaced GitHub link in docs landing page with releases link
- **Docs**: Fixed troubleshooting page — issues link replaced with email contact
- **Docs**: Blog pages — GitHub links → RavenFabric-Published releases
- **Docs**: Compliance docs version updated from `v0.5-dev` to `v0.1.4` with correct stats (50k LOC, 1,037 tests)
- **Docs**: Fixed README post-quantum wording from "(planned)" to reflect actual implementation (`HybridKemContext` + `PqxdhRatchet`)
- **Docs**: Updated `MANUAL-TASKS-TODO.md` — marked transport drivers section as completed, updated issue #89 reference
- **Docs**: Updated ROADMAP — pkg installer, zypper, F-Droid changed from `[ ] Planned` to `[x]` with packaging files
- **Docs**: README CI badge replaced with static version badge (private repo badge returns 404)
- **Versions**: OBS `_service` revision updated from `v0.1.3` to `v0.1.4`
- **Versions**: Web UI footer version updated from `v0.1.0` to `v0.1.4`
- **Release workflow**: Fixed Homebrew formula test assertion (`--help` instead of `--version`)

## [0.1.3] — 2026-05-08

### Added

- **Transport**: I2P driver (`i2p.rs`) — SAM bridge protocol v3.1 (TCP 7656), stream connect/accept, destination validation, session management (15 tests)
- **Transport**: Veilid driver (`veilid.rs`) — JSON-RPC API transport via Veilid daemon, DHT route-based addressing, app_call protocol, route validation (15 tests)
- **Transport**: Reticulum Network Stack driver (`reticulum.rs`) — shared instance TCP, 2-byte framed protocol, hex destination hash validation, FNV-1a hashing (18 tests)
- **Transport**: BLE driver (`ble.rs`) — Nordic UART Service GATT proxy, MAC address validation, MTU-based fragmentation/reassembly (17 tests)
- **Transport**: Wi-Fi Direct driver (`wifi_direct.rs`) — wpa_supplicant ctrl, P2P device address validation, peer info parsing (12 tests)
- **Transport**: Audio modem driver (`audio_modem.rs`) — 2-FSK modulation, near-ultrasonic 18/19kHz, zero-crossing detection, CRC-16/CCITT framing (15 tests)
- **Transport**: QR-stream driver (`qr_stream.rs`) — QR frame sequencing, fragment/reassemble, ECC levels, bitrate estimation (15 tests)
- **Transport**: LoRa/Meshtastic driver (`lora.rs`) — Meshtastic serial/TCP protocol, magic-byte framing, node ID validation, spreading factor airtime (17 tests)
- **Transport**: AX.25 packet radio driver (`ax25.rs`) — KISS TNC framing, callsign/SSID parsing, UI frames (19 tests)
- **Transport**: HF radio/Winlink driver (`hf_radio.rs`) — VARA HF modem TCP interface, CONNECT/MYCALL commands, message framing (16 tests)
- **Transport**: Satellite link driver (`satellite.rs`) — Iridium SBD AT commands, IMEI validation, SBD checksum, orbital pass windows (17 tests)
- **Transport**: Mixnet driver (`mixnet.rs`) — Sphinx packet format, multi-hop routing, SURB anonymous replies, latency estimation (20 tests)
- **CI**: Created GitHub issue #92 tracking Actions 0-step workflow failures (suspected exhausted minutes)

### Fixed

- **CI**: Fixed branch protection MSRV check name from "MSRV (1.85)" to "MSRV (1.88)" to match actual Rust MSRV
- **Docs**: Updated LOC counts to match actual codebase (~50,000 total, rf-transport ~21,900)
- **Docs**: Updated test counts to match actual test suite (1,037 Rust tests, rf-transport 542, rf-mcp-client 15)
- **Docs**: Updated `docs/src/architecture/overview.md` with accurate per-crate stats
- **Docs**: Updated `MANUAL-TASKS-TODO.md` with detailed diagnosis of 0-step workflow failures

## [0.1.2] — 2026-05-08

### Added

- **Transport**: Tor hidden service driver (`tor.rs`) — full SOCKS5 CONNECT via local Tor proxy, .onion validation, 8 tests
- **Transport**: Yggdrasil overlay driver (`yggdrasil.rs`) — TCP over Yggdrasil IPv6 mesh (200::/7), listen support, 7 tests
- **Packaging**: Snap package manifest (`deploy/snap/snapcraft.yaml`) — daemon support, strict confinement, amd64+arm64
- **Packaging**: WiX MSI installer manifest (`deploy/wix/ravenfabric.wxs`) — Windows service, PATH, feature tree
- **Packaging**: NSIS EXE installer script (`deploy/nsis/ravenfabric.nsi`) — GUI installer, service install, Start Menu
- **CI**: Publish binaries to [egkristi/RavenFabric-Published](https://github.com/egkristi/RavenFabric-Published) on release (versioned directories + GitHub Release + SHA256SUMS)
- **CI**: Automated crates.io publish job in release workflow (dependency-ordered, skip-on-error)

### Fixed

- **CI**: Fixed `cargo fmt` check failure — expanded struct initializations in `named_pipe.rs` to multi-line format
- **Docker**: Fixed `latest` tag not being applied to container images (tag-triggered workflows don't match `is_default_branch`)
- **Packaging**: Updated `flake.nix` version from 0.1.0 to 0.1.2
- **Packaging**: Bumped version to 0.1.2 in all packaging manifests (AUR, Alpine, Chocolatey, Scoop, WinGet, Python SDK, TypeScript SDK)
- **Docs**: Updated install URLs from raw GitHub to `get.ravenfabric.io` across README, ROADMAP, install docs, and deploy script

### Changed

- **Release**: Release pipeline now produces all 8 platform binaries (verified: linux-amd64, linux-arm64, linux-amd64-musl, linux-arm64-musl, linux-armv7-musl, darwin-arm64, darwin-amd64, windows-amd64)

## [0.1.1] — 2026-05-08

### Changed

- **Docs**: README.md — comprehensive accuracy audit removing fabricated YAML formats (reduced 1,449 → 1,099 lines, 24% reduction), fixed policy/security/transport/bootstrap/grains sections to match actual codebase, consolidated redundant comparison sections, corrected license badge to AGPL-3.0-or-later
- **Docs**: README.md — updated LOC counts to match actual codebase (~43,800 total, rf-rpc ~5,900, rf-agent ~380)
- **Docs**: ROADMAP.md — cleaned up stale closed-issue references in Distribution & Packaging section, replaced `#N` refs with descriptive status text
- **Docs**: copilot-instructions.md — updated total LOC (~43,800) and rf-integration-tests LOC (~580)
- **Deploy**: Helm Chart.yaml — fixed maintainers YAML format (structured `name:` / `email:` keys)
- **Docs**: cloudnativepg.md — fixed CLI syntax (`rf tunnel` → `rf forward`, added `--token` flags), corrected sealed secrets status (Planned → Done), updated PostgreSQL image tag (18 → 17)
- **Docs**: ai-agent-skill.md — fixed `rf tunnel` → `rf forward` and `rf shell` syntax

### Fixed

- **Tests**: MCP E2E integration tests — fixed race condition where parallel tests shared same temp directory, causing intermittent "missing field `spec`" YAML parse failures in CI

### Added

- **Crypto**: Noise XX handshake (Noise_XX_25519_ChaChaPoly_BLAKE2s) with wire protocol
- **Crypto**: SecureChannel with concurrent read/write via split Mutex pattern
- **Crypto**: StaticKey management (generate, load, save, zeroed on drop)
- **Transport**: Driver trait with AsyncStream abstraction
- **RPC**: Request/Response types with msgpack serialization
- **Audit**: Structured JSON-lines audit logging (FileAuditLogger)
- **Policy**: YAML policy loading with deny-by-default enforcement
- **Policy**: Command pattern matching (regex) and filesystem path checks with symlink resolution
- **Executor**: Command execution with policy enforcement, timeout, output limiting
- **Bootstrap**: OTP generation and validation (single-use, hash-stored, TTL-enforced)
- **CLI**: Skeleton with exec/dev/status subcommands
- **CI**: GitHub Actions (check, fmt, clippy, test, coverage, MSRV)
- **CI**: Cross-platform release workflow (Linux, macOS, ARM64)
- **CI**: CodeQL security scanning
- **CI**: Dependabot for Cargo and GitHub Actions
- **MCP Server**: `rf-mcp-server` binary with JSON-RPC 2.0 over stdio (8 tools: exec, query_policy, file_read, file_write, list_capabilities, audit_query, request_approval, check_approval)
- **Transport**: Named pipe driver for Windows local IPC (`\\.\pipe\ravenfabric`)
- **Transport**: Vsock driver for VM-to-hypervisor communication (Firecracker, QEMU, cloud-hypervisor)
- **Transport**: Abstract namespace socket driver (Linux-only, kernel-managed, no filesystem cleanup)
- **Transport**: Auto-select driver (probes available transports, selects best by priority)
- **Transport**: Socket activation support (systemd-style LISTEN_FDS protocol)
- **Policy**: Behavioral anomaly detection — velocity, novelty, timing, and escalation scoring per identity with automatic capability reduction
- **Audit**: AI compliance reporting — EU AI Act risk classification, NIST AI RMF mapping, human oversight tracking, report generation (JSON/CSV export)
- **RPC**: Embedded Web UI dashboard — real-time agent metrics, connected agents table, activity feed (self-contained HTML/CSS/JS, no external dependencies)
- **MCP Server**: API token authentication — `--api-token` / `RF_API_TOKEN`, constant-time validation
- **MCP Server**: Per-session rate limiting — sliding window throttle (`--rate-limit` / `RF_RATE_LIMIT`, default 60/min)
- **MCP Server**: Session isolation — unique session ID, process-level sandbox, session ID exposed in initialize response
- **Docs**: Claude Code integration guide (`docs/src/integrations/claude-code.md`)
- **Docs**: Cursor integration guide (`docs/src/integrations/cursor.md`)
- **Docs**: Aider integration guide (`docs/src/integrations/aider.md`)
- **Docs**: Claude Desktop integration guide (`docs/src/integrations/claude-desktop.md`)
- **Docs**: AI Agent Quick Start tutorial (`docs/src/getting-started/ai-quickstart.md`)
- **MCP Server**: Anomaly-audit integration — behavioral events written to audit log with baseline comparison
- **MCP Server**: `rf_check_approval` tool — poll approval status (PENDING/APPROVED/DENIED)
- **MCP Server**: `approve()` / `deny()` API for operator approval control
- **MCP Server**: Token rotation — comma-separated tokens for grace period, `--api-token-file` for external rotation
- **MCP Server**: Alert routing — `--alert-webhook` / `RF_ALERT_WEBHOOK` sends anomaly events to HTTP endpoint
- **MCP Server**: RBAC per caller — `--callers` TOML config maps tokens to per-caller policy profiles
- **MCP Server**: Per-session cryptographic identity — short-lived Curve25519 keypair generated per session, public key in `initialize` response and capabilities
- **MCP Server**: HTTP+SSE transport — `--http-listen` for multi-user server deployment (requires `http-sse` feature), per-session isolation, SSE streaming, health endpoint
- **Executor**: Desired-state convergence engine — declarative resource management (packages, files, services, sysctl) with drift detection, remediation mode, version constraints, `ConvergenceReport` (18 tests)
- **Executor**: Event system — trigger-based execution with Cron, FileWatch, ProcessExit, Webhook, Timer triggers, broadcast-based `EventBus`, `TimerScheduler` (12 tests)
- **Executor**: Result parsing and assertions — multi-format parser (JSON, YAML, CSV, key-value, lines, regex) with assertion engine (Eq, Ne, Contains, Matches, Gt/Lt/Gte/Lte, Exists) (18 tests)
- **Executor**: Grains auto-collection — Salt-like system facts (OS, arch, hostname, env) with label selector matching for agent targeting (10 tests)
- **SDK**: Python MCP client (`sdks/python/`) — pip-installable package with async + sync API, StdioTransport, JSON-RPC 2.0, LangChain + CrewAI + OpenAI + Anthropic + AutoGen integrations, typed dataclasses, 41 tests
- **SDK**: TypeScript MCP client (`sdks/typescript/`) — npm package with fully typed async API, StdioTransport, Promise-based JSON-RPC, 12 tests
- **SDK**: Agent framework benchmark suite (`sdks/python/benchmarks/`)
- **Crypto**: `no_std` feature gate — `rf-crypto --no-default-features` compiles without std, exposes `frame_codec` module (ChaCha20-Poly1305 encrypt/decrypt, 7 tests)
- **Crypto**: WASM target support — `rf-crypto` compiles for `wasm32-wasip1`
- **CI**: `no_std + WASM` job validates both compilation targets
- **Deploy**: OpenWrt package (`deploy/openwrt/`) — Makefile + procd init script
- **Deploy**: macOS DMG build script (`deploy/macos/build-dmg.sh`)
- **Deploy**: Alpine APKBUILD (`deploy/alpine/`) with OpenRC init scripts
- **Deploy**: Android NDK cross-compile config (`deploy/android/`) + AndroidManifest.xml
- **Deploy**: iOS Network Extension guide (`deploy/ios/`) + cargo config
- **Docs**: Asciinema demo recording script (`docs/demo/demo-record.sh`)
- **Docs**: `no_std` evaluation for bare-metal ARM (`docs/evaluations/no-std-evaluation.md`)
- **Transport**: MASQUE proxy transport (HTTP/3 CONNECT-UDP tunneling)
- **Transport**: ECH (Encrypted Client Hello) transport for censorship resistance
- **MCP Client**: Rust MCP client SDK (`rf-mcp-client`) — stdio transport, typed tool wrappers (720 LOC, 14 tests)
- **MCP Server**: Fuzz target `fuzz_mcp_protocol` for protocol fuzzing
- **Transport**: File-descriptor passing via SCM_RIGHTS
- **Deploy**: Docker multi-stage build — 4 targets (agent, relay, cli, mcp-server) from scratch/alpine, Rust 1.88 musl static
- **Deploy**: Docker Compose local demo — relay + agent + CLI containers with shared network
- **Deploy**: Helm chart — relay Deployment, agent DaemonSet, ConfigMap, Ingress, NOTES.txt, full values.yaml
- **CI**: Docker workflow — build and push 4 images (multi-arch amd64+arm64) on tag via QEMU
- **Tests**: MCP server E2E integration tests — 8 tests covering initialize/auth, tools/list, exec (allow/deny), policy query, capabilities, invalid method, rate limiting
