# Sentinel — Design Specification

> **Working codename:** "Sentinel" (privacy/security focus — name is provisional and can change).
> **Date:** 2026-06-29
> **Status:** Approved design, ready for implementation planning.

---

## 1. Purpose & Vision

Sentinel is a **lightweight, security- and privacy-focused desktop web browser** built in Rust.
It targets Windows 7 and newer, uses far fewer resources than Chrome, and adds two
capabilities ordinary browsers cannot match:

1. **Native device control** — web pages can talk to USB and serial hardware (scanners,
   microcontrollers, instruments, POS devices) through a permission-gated bridge.
2. **Native DLL execution** — a web page may load and call its own native `.dll`, with explicit
   per-origin user consent, executed inside an isolated host process.

The default posture is **locked down**: encrypted local storage, no third-party JS injection,
strict permission gating. Power is **opt-in and explicit**: the user deliberately grants a page
access to devices or native code.

### Non-goals (honest scope boundaries)

- **Not** a re-implementation of a browser engine. Rendering, layout, and JS execution are
  delegated to **Ultralight** (a WebKit/JavaScriptCore-based embeddable renderer).
- **Not** a process-per-origin sandboxed engine like Chrome. Ultralight runs all pages in one
  renderer; therefore Sentinel does **not** promise cross-origin process isolation at the engine
  level. Security is delivered at the **network, storage, and permission** layers (see §4).
- **Not** guaranteed to render every site perfectly. Ultralight bundles a WebKit fork that trails
  upstream; DRM (Widevine) and some bleeding-edge CSS/JS may be unavailable. "Everyday browsing"
  works for the large majority of sites but is best-effort on the heaviest web apps.

---

## 2. Technology Stack

| Concern | Crate / API | Role |
|---|---|---|
| HTML/CSS/JS rendering | **Ultralight** via `ul-next` | Renders pages, runs JS (JavaScriptCore) |
| FFI binding generation | **bindgen** | Generate Rust bindings for native plugin/DLL ABIs |
| Dynamic library loading | **libloading** | Load `.dll` at runtime (plugins + page-supplied DLLs) |
| Serial I/O | **serialport** | Enumerate and read/write serial ports |
| USB I/O | **nusb** | Pure-Rust cross-platform USB (WinUSB on Windows) |
| Async runtime | **tokio** | Async bridge, device I/O, networking, IPC |
| Persistent storage | **rusqlite** (SQLite) | History, bookmarks, cookies, permissions, passwords |
| Filesystem | **std::fs** | Profile dirs, cache, downloads, config |

### Licensing & platform constraints (from research)

- **Ultralight** is proprietary: free for non-commercial use and indie commercial use (< $100K
  annual revenue); paid otherwise. SDK binaries are downloaded, not built from source. This is an
  accepted dependency.
- **Windows 7**: Rust's `x86_64-win7-windows-msvc` target is **Tier 3** (no prebuilt toolchain;
  requires `build-std` or a self-built toolchain). Strategy: develop and CI on **Windows 10/11
  (stable MSVC target)**; validate Windows 7 later as a dedicated gate using the win7 target and
  Ultralight's **CPU renderer** (Win7 lacks guaranteed D3D11). Win7 does **not** block Phase 1.
- **nusb** requires the target USB device to be bound to the **WinUSB** driver (via WCID descriptor
  or Zadig). This is a deployment note surfaced to users in the device-permission UI.

---

## 3. Architecture Overview

Sentinel is a multi-process application. Process boundaries exist for **stability and damage
containment**, since the engine itself is single-process.

```
┌──────────────────────────────────────────────────────────────┐
│  Main / Browser Process                                        │
│                                                                │
│  ┌────────────┐  ┌──────────────┐  ┌────────────────────────┐  │
│  │ UI / Shell │  │  Tab Manager │  │  Storage (rusqlite +    │  │
│  │ (chrome)   │  │  + Navigation│  │  encryption, std::fs)   │  │
│  └─────┬──────┘  └──────┬───────┘  └────────────────────────┘  │
│        │                │                                       │
│  ┌─────▼────────────────▼───────────────────────────────────┐  │
│  │  Ultralight Renderer (ul-next): View per tab, JS (JSC)    │  │
│  └─────┬─────────────────────────────────────────────────────┘ │
│        │  window.native.*  (JS ↔ Rust bridge)                   │
│  ┌─────▼──────────────┐  ┌──────────────────────────────────┐  │
│  │ Permission Broker  │  │ Native Bridge (tokio async)      │  │
│  │ (per-origin grants)│  │  routes calls → subsystems       │  │
│  └─────┬──────────────┘  └───────┬──────────────┬───────────┘  │
└────────┼─────────────────────────┼──────────────┼──────────────┘
         │ IPC (request/grant)     │ IPC          │ IPC
   ┌─────▼─────┐            ┌───────▼──────┐ ┌─────▼─────────────┐
   │ (user UI  │            │ Device Host  │ │ DLL Host Process  │
   │  prompts) │            │  Process     │ │ (sandboxed)       │
   └───────────┘            │ nusb +       │ │ libloading +      │
                            │ serialport   │ │ bindgen ABI       │
                            └──────────────┘ └───────────────────┘
```

- **Main process**: UI shell, tabs, Ultralight renderer, storage, permission broker, bridge router.
- **Device Host process**: owns `nusb`/`serialport` handles; isolates driver/hardware faults.
- **DLL Host process**: loads and calls page-supplied or plugin DLLs; isolates native crashes and
  limits blast radius of untrusted native code. Communicates only via a typed IPC channel.

### Module boundaries (each unit: one purpose, defined interface, independently testable)

| Module | Responsibility | Depends on |
|---|---|---|
| `shell` | Window, chrome UI, tab strip, address bar, input routing | `engine`, `tabs` |
| `engine` | Ultralight lifetime, View creation, paint loop, JS context | `ul-next` |
| `tabs` | Tab model, navigation history, lifecycle | `engine`, `storage` |
| `storage` | Encrypted rusqlite access: history, bookmarks, cookies, perms, secrets | `rusqlite`, `crypto` |
| `crypto` | Key derivation, AES-GCM encrypt/decrypt of stored data | (ring/aes-gcm) |
| `security` | CSP relay, integrity policy, TLS hardening, JS-injection lockdown | `engine` |
| `bridge` | `window.native.*` JS API, request routing, serialization | `engine`, `permissions`, hosts |
| `permissions` | Per-origin grant store + consent prompts | `storage`, `shell` |
| `device-host` | Out-of-process USB/serial via nusb + serialport | `nusb`, `serialport`, `ipc` |
| `dll-host` | Out-of-process DLL load/call via libloading + bindgen ABI | `libloading`, `ipc` |
| `ipc` | Typed request/response channel between processes | `tokio`, serde |

---

## 4. Security & Privacy Model (honest threat model)

### What Sentinel guarantees

- **Encrypted at rest**: history, bookmarks, cookies, saved credentials, and permission grants are
  stored in an encrypted rusqlite database. Key is derived from a user master passphrase (Argon2)
  and never written in plaintext.
- **No third-party JS injection by default**: no DevTools JS console, no arbitrary extensions, no
  `javascript:` bookmarklets. The page's own scripts run; nothing the operator or a MITM adds runs.
  CSP from the server is relayed/enforced to the extent Ultralight supports it; TLS is hardened
  (modern ciphers, HSTS honored, cert errors are hard-fail by default).
- **Permission gating**: device and native-DLL access require explicit, per-origin user consent,
  stored and revocable.
- **No telemetry**: nothing leaves the machine except the page traffic the user initiates.

### What Sentinel explicitly does NOT guarantee

- **No cross-origin process isolation** at the engine level (Ultralight is single-renderer). A
  malicious page exploiting an engine bug is not contained the way Chrome contains it.
- **Engine is not bleeding-edge patched**: Ultralight's bundled WebKit trails upstream; treat
  Sentinel as unsuitable for adversarial/high-risk browsing of untrusted sites until/unless engine
  patch cadence is verified per release.
- **Granting native DLL or device access to a page is equivalent to installing software from that
  origin.** Once granted, the page runs native code or talks to hardware with the user's privileges.
  The consent UI states this in plain language. This is the user's deliberate, informed trade-off.

### DLL / native trust model (user chose "fully open")

- Any origin may request to load a `.dll` it supplies, **but only after an explicit, scary consent
  prompt** naming the origin and the DLL hash.
- The DLL runs in the **DLL Host process**, never in the main process. A crash or hang there cannot
  take down the browser or other tabs.
- Each granted DLL is recorded (origin, file hash, timestamp) and is revocable from settings.
- Future hardening (post-MVP): restricted-token / job-object sandboxing of the DLL Host, syscall
  filtering. MVP delivers process isolation + consent + audit log, not a full syscall sandbox.

---

## 5. Native Bridge API (`window.native.*`)

Injected **only by the browser binary** into a page's JS context — this is a first-party
capability, not user injection. All calls are async (return Promises), routed through the
permission broker, and serialized over IPC to the relevant host process.

```js
// USB
await window.native.usb.list()                      // -> [{vendorId, productId, serial, ...}]
await window.native.usb.open({vendorId, productId}) // -> handle (after permission grant)
await handle.transferIn(endpoint, length)           // -> ArrayBuffer
await handle.transferOut(endpoint, data)            // -> bytesWritten

// Serial
await window.native.serial.list()                   // -> [{port, ...}]
const port = await window.native.serial.open({path, baudRate})
await port.write(data); await port.read(length)

// DLL (gated by explicit consent)
const lib = await window.native.dll.load({url|bytes, expectedHash})
await lib.call("symbol", abiSignature, args)        // typed via bindgen-described ABI
```

Permission flow: first call to a gated API for an origin triggers a consent prompt; the grant is
persisted (origin + capability + optional device/DLL identity) and reused silently thereafter until
revoked.

---

## 6. Data Model (rusqlite, encrypted)

Tables (all value columns encrypted via `crypto` before write where sensitive):

- `history(id, url, title, visited_at, visit_count)`
- `bookmarks(id, url, title, folder, created_at)`
- `cookies(host, name, value, path, expires, secure, http_only, same_site)`
- `credentials(id, origin, username, secret_blob)` — secret_blob is AES-GCM ciphertext
- `permissions(origin, capability, scope_json, granted_at, revoked_at)`
- `dll_grants(origin, dll_hash, source, granted_at, revoked_at)`
- `settings(key, value)`

Master key: Argon2id(passphrase, salt) → 256-bit key; per-record nonces; AES-256-GCM.

---

## 7. Phased Roadmap

Each phase is an independent spec → plan → implementation cycle.

- **Phase 1 — Core Shell (foundation).** Cargo workspace; integrate `ul-next`; open a window; load a
  URL; render; run page JS; basic navigation (back/forward/reload/address bar); one tab, then tabs.
  *Confirms the engine binding works on target — the real first risk.* **Build this first.**
- **Phase 2 — Storage + Security.** Encrypted rusqlite; history/bookmarks/cookies; crypto module;
  JS-injection lockdown; TLS hardening; CSP relay.
- **Phase 3 — Native Bridge + Permissions.** `window.native` skeleton; permission broker; consent UI;
  IPC scaffolding to host processes.
- **Phase 4 — USB / Serial.** Device Host process; `nusb` + `serialport`; USB/serial bridge APIs.
- **Phase 5 — DLL plugins.** DLL Host process; `libloading` + `bindgen` ABI; consent + audit;
  browser-installed plugins and page-supplied DLLs.

---

## 8. Testing Strategy

- **Per-module unit tests** with mocked dependencies (storage against in-memory SQLite; bridge
  against a fake host; permissions against a fake prompt).
- **IPC contract tests**: serialize/deserialize round-trips for every request/response type.
- **Engine smoke tests** (Phase 1): headless render of a known HTML string → assert pixels/DOM hooks.
- **Security tests**: assert no `window.native` on un-granted origins; assert injected JS paths are
  closed; assert stored blobs are ciphertext (no plaintext leakage).
- **Device/DLL tests**: against loopback/mock devices and a test DLL with a known ABI.
- **Verification discipline**: every function is covered by a test or an explicit, documented manual
  verification before being marked done (per project requirement).

---

## 9. Documentation Requirements

- Every file begins with a module doc comment (`//!`) stating purpose, public surface, dependencies.
- Every public function/struct has a `///` doc comment: what it does, params, returns, errors,
  and a usage note where non-obvious.
- Each crate in the workspace has a README describing its boundary and how to use it.
- Architecture decisions recorded under `docs/superpowers/specs/`.

---

## 10. Open Risks (tracked)

| Risk | Mitigation |
|---|---|
| Ultralight Win7 D3D11 absent | CPU renderer fallback; validate in Win7 gate |
| Win7 Tier-3 toolchain friction | Dev on Win10/11; build-std for Win7 later |
| Ultralight engine CVE lag | Document; verify patch cadence per release; not for adversarial browsing |
| Web-compat gaps (DRM, modern CSS) | Best-effort; document unsupported surfaces |
| Native DLL = full machine access | Explicit consent + process isolation + audit; documented honestly |
| Ultralight commercial license cost | Stay within indie tier or budget for license |
