# no_std Subset Evaluation for Bare-Metal ARM (ESP32, nRF52)

**Issue:** [#83](https://github.com/egkristi/RavenFabric/issues/83)
**Date:** 2026-05-07
**Status:** Evaluation Complete — Viable with feature-gating

## Summary

A `no_std` subset of RavenFabric is **viable** for bare-metal ARM targets (ESP32, nRF52840)
with the following approach:

1. Feature-gate file I/O and async runtime in `rf-crypto`
2. Use `alloc` crate for heap allocations (HashMap, Vec, Arc)
3. Replace `snow` with pure-Rust Noise primitives (`chacha20poly1305` + `x25519-dalek`)
4. Provide a minimal `rf-codec` crate for wire protocol encode/decode only

## Dependency Analysis

### rf-crypto (target for no_std)

| Dependency | std Required? | no_std Alternative |
|---|---|---|
| `snow` | Yes (uses ring/std) | `x25519-dalek` + `chacha20poly1305` + `blake2` (all no_std) |
| `rand` | Partial | `rand_core` with hardware RNG trait |
| `chacha20poly1305` | No | Already no_std compatible |
| `sha2` | No | Already no_std compatible |
| `serde` | No | `serde` with `default-features = false, features = ["derive", "alloc"]` |
| `tokio` | Yes | Remove (use blocking or interrupt-driven) |
| `tracing` | Yes | Remove or use `defmt` |
| `hex` | No | no_std compatible |

### std Usages in rf-crypto

| File | Usage | no_std Solution |
|---|---|---|
| `keys.rs` | `std::fs`, `std::path` | Feature-gate behind `#[cfg(feature = "std")]` |
| `keys.rs` | `std::os::unix::fs::PermissionsExt` | Feature-gate (Unix-only anyway) |
| `channel.rs` | `std::sync::Arc` | `alloc::sync::Arc` |
| `error.rs` | `std::io::Error` | Feature-gate file errors |
| `resumption.rs` | `HashMap`, `HashSet` | `alloc::collections` (BTreeMap for no_std) |
| `pq.rs` | `HashMap` | `alloc::collections::BTreeMap` |
| `secrets.rs` | `HashMap`, `ptr::write_volatile` | `BTreeMap`, `core::ptr::write_volatile` |

### Total Changes Required

- **~15 lines** of `#[cfg(feature = "std")]` gating
- **~8 lines** of `use` changes (std:: → alloc::)
- **1 new dependency**: Replace `snow` with direct crypto primitives behind feature
- **No algorithmic changes** — Noise XX handshake logic is the same

## Target Hardware

### ESP32 (Xtensa/RISC-V, 520KB SRAM, 4MB flash)

- **Rust support**: `esp-idf-hal` with `std` OR `esp-hal` (no_std)
- **Crypto**: Hardware AES accelerator available, but ChaCha20 is CPU-only
- **RAM budget**: ~200KB available after stack/heap — Noise handshake needs ~2KB
- **Flash budget**: Stripped binary ~500KB-1MB feasible
- **Verdict**: ✅ Viable. Noise XX handshake fits easily in memory

### nRF52840 (ARM Cortex-M4F, 256KB RAM, 1MB flash)

- **Rust support**: `embassy-nrf` (async, no_std) — excellent
- **Crypto**: Hardware AES/CCM, but ChaCha20Poly1305 must be software
- **RAM budget**: ~100KB available — tight but feasible
- **Flash budget**: ~500KB available — feasible with LTO
- **Verdict**: ✅ Viable. Tighter constraints but workable

### STM32 (ARM Cortex-M, various)

- **Rust support**: `embassy-stm32` — excellent
- **Verdict**: ✅ Same approach as nRF52

## Proposed Architecture

```
rf-crypto (feature = "std", default)
├── keys.rs       — File I/O for key storage
├── channel.rs    — Tokio async SecureChannel
├── resumption.rs — Session resumption (needs HashMap)
└── noise.rs      — Noise XX handshake

rf-crypto (feature = "no_std")
├── noise_core.rs — Pure Noise XX (x25519 + chacha20poly1305 + blake2s)
├── frame.rs      — Frame encode/decode (length-prefixed ciphertext)
└── keys_mem.rs   — In-memory key operations only
```

## Feature Flag Design

```toml
[features]
default = ["std"]
std = ["snow", "tokio", "tracing", "serde_json"]
no_std = ["chacha20poly1305", "x25519-dalek", "blake2"]
alloc = []  # Use alloc crate without full std
```

## Implementation Plan

### Phase 1: Feature-gate existing code (low risk)

1. Add `#![cfg_attr(not(feature = "std"), no_std)]` to `rf-crypto/src/lib.rs`
2. Gate `keys.rs` file operations behind `#[cfg(feature = "std")]`
3. Gate `tokio` usage behind `#[cfg(feature = "std")]`
4. Replace `std::collections` with conditional imports

### Phase 2: Pure-Rust Noise XX (medium effort)

1. Implement Noise XX pattern using `x25519-dalek` + `chacha20poly1305` + `blake2`
2. This avoids the `snow` → `ring` dependency chain entirely
3. ~200 lines for a minimal Noise XX state machine

### Phase 3: Embedded integration (proof of concept)

1. Create `examples/esp32/` with basic Noise handshake over UART
2. Create `examples/nrf52/` with Embassy async + BLE transport
3. Measure: binary size, RAM usage, handshake latency

## Performance Estimates

| Operation | ESP32 (240MHz) | nRF52840 (64MHz) |
|---|---|---|
| X25519 key exchange | ~10ms | ~50ms |
| ChaCha20Poly1305 encrypt (1KB) | ~0.1ms | ~0.5ms |
| BLAKE2s hash (32B) | <0.01ms | ~0.05ms |
| Full Noise XX handshake | ~30ms | ~150ms |

These are acceptable for IoT use cases (connections are long-lived, handshake is one-time).

## Binary Size Estimates

| Configuration | Size (stripped, LTO) |
|---|---|
| Full rf-crypto (std, snow) | ~2MB |
| no_std rf-crypto (pure Rust) | ~80KB |
| Minimal frame codec only | ~20KB |

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| snow not no_std compatible | Blocks direct reuse | Use pure-Rust primitives directly |
| alloc not available on all targets | Limits HashMap usage | Provide static-allocation fallback |
| Hardware RNG quality varies | Weak keys | Require TRNG or entropy accumulator |
| No filesystem on bare metal | Can't persist keys | Use flash sectors or external EEPROM |
| No async runtime | Different programming model | Embassy provides embedded async |

## Conclusion

A `no_std` subset of RavenFabric is **feasible and practical** for ESP32 and nRF52.
The recommended approach is:

1. **Short-term**: Feature-gate `rf-crypto` to compile without `std` (Phase 1)
2. **Medium-term**: Implement pure-Rust Noise XX for embedded (Phase 2)
3. **Long-term**: Full embedded examples with BLE/UART transport (Phase 3)

The core Noise XX protocol and frame encryption are fundamentally `no_std`-compatible.
The main barriers are convenience features (file I/O, async runtime, tracing) which
are easily gated behind cargo features.

**Estimated effort**: Phase 1 (1 day), Phase 2 (3 days), Phase 3 (1 week)
