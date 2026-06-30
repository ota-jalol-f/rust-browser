# Sentinel

A lightweight, **security- and privacy-focused** desktop web browser written in Rust,
targeting Windows 7+ with two capabilities ordinary browsers lack: **native device
control** (USB/serial) and **native DLL execution**, both gated behind explicit user
consent.

> Working codename "Sentinel" — provisional.

## Why

- **Light:** rendering is delegated to [Ultralight](https://ultralig.ht/) (a GPU/CPU
  WebKit-based embeddable engine), so memory/CPU stay far below Chrome.
- **Private by default:** encrypted local storage; no third-party JS injection; no
  telemetry; permission-gated capabilities.
- **Powerful when you opt in:** a web page can — with an explicit, per-origin consent
  prompt — talk to USB/serial hardware or load a native `.dll`, executed in an isolated
  host process.

See the full design and honest threat model in
[`docs/superpowers/specs/2026-06-29-rust-browser-design.md`](docs/superpowers/specs/2026-06-29-rust-browser-design.md).

## Workspace layout

| Crate | Status | What it does |
|---|---|---|
| `sentinel-core` | ✅ verified (12 tests) | Error/Result, `BrowserConfig`, navigation `History` |
| `sentinel-crypto` | ✅ verified (11 tests) | Argon2id KDF + AES-256-GCM at-rest encryption |
| `sentinel-storage` | ✅ verified (6 tests) | Encrypted SQLite store: history + bookmarks |
| `sentinel-engine` | ✅ render + JS bridge verified (SDK tests) | Ultralight integration; headless CPU render and `window.native` JS↔Rust bridge both proven |
| `sentinel-shell` | ✅ GUI runs (4 tests) | Window + overlay + navigation; opens google.com |
| `sentinel-permissions` | ✅ verified (12 tests) | Per-origin capability consent gate (Ask = secure default) |
| `sentinel-ipc` | ✅ verified (9 tests) | Typed host protocol + length-delimited framing |
| `sentinel-bridge` | ✅ verified (12 tests) | Permission-gated router + JSON dispatch; `window.native` end-to-end proven |
| `sentinel-device-host` | ✅ enumeration on real HW (5 tests) | USB/serial via nusb + serialport; device I/O stubbed |
| `sentinel-dll-host` | ⏳ planned | Out-of-process native DLL execution |

## Build & test

```bash
# Pure-Rust crates — fully tested in CI, no SDK needed:
cargo test --workspace

# Ultralight engine — compiles the real binding; rendering needs the SDK DLLs at runtime:
cargo build -p sentinel-engine --features ultralight
cargo test  -p sentinel-engine --features ultralight -- --ignored   # on a machine with the SDK
```

Toolchain: Rust 1.86 stable, `x86_64-pc-windows-msvc`. Windows 7 is a later validation
gate (CPU renderer + `win7` target via `build-std`), intentionally not blocking early work.

## Roadmap

Phased, each phase its own spec → plan → implementation:
1. **Core shell** ✅ — engine + window + navigation (GUI runs).
2. **Storage + security** — storage ✅; security hardening (CSP/TLS/injection lockdown) remains.
3. **Native bridge + permissions** ✅ — `window.native.*` gated end-to-end (verified headless).
4. **USB / serial** — enumeration ✅ (real hardware); per-device I/O + IPC transport remain.
5. **DLL host** — planned (libloading + bindgen, consent + sandbox).

**End-to-end proven:** a granted page call to `window.native.usb.list()` flows JS → shim →
Rust dispatcher → permission gate → real device-host enumeration (nusb) → JSON back to the
page — verified headlessly against this machine's hardware.

Remaining cross-cutting work: route DLL requests through the executor too; the per-origin
consent prompt UI; per-device I/O (open/transfer); security hardening (CSP/TLS/injection
lockdown); and an out-of-process tokio IPC transport for true host isolation (the executor
seam and frame protocol are already in place).

Plans live in [`docs/superpowers/plans/`](docs/superpowers/plans/).

## Licensing note

Ultralight is proprietary: free for non-commercial and indie commercial use
(< $100K annual revenue); a paid license is required otherwise.
