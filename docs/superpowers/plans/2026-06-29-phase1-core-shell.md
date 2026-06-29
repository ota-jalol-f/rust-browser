# Phase 1: Core Shell — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a Cargo workspace and a working Ultralight-backed browser shell that opens a window, navigates to a URL, renders it, runs page JS, and supports back/forward/reload — built on a verified, pure-Rust foundation.

**Architecture:** A Cargo workspace splits verifiable pure-Rust crates (`sentinel-core` types/errors/config, plus a headless engine smoke path) from the GUI binary (`sentinel-shell`). The engine wraps `ul-next` 0.5.4: a low-level `Renderer` + CPU `Surface` path used for headless render tests in CI, and the high-level `App`/`Window`/`Overlay` path used for the actual windowed browser. Navigation state (history stack, current URL) lives in pure-Rust types that are unit-tested independently of the GUI.

**Tech Stack:** Rust 1.86 (stable, `x86_64-pc-windows-msvc`), `ul-next` 0.5.4 (Ultralight bindings), `thiserror`, `serde`. Ultralight SDK binaries are fetched by `ul-next`'s build/`loaded` feature.

## Global Constraints

- Platform dev/CI baseline: Windows 10/11, `x86_64-pc-windows-msvc`. Windows 7 is a later validation gate (CPU renderer + `win7` target/`build-std`) — do NOT block Phase 1 on it.
- Rust edition 2021. Workspace resolver = "2".
- Every file starts with a `//!` module doc comment (purpose, public surface, dependencies).
- Every public item has a `///` doc comment (what, params, returns, errors).
- TDD: write the failing test first, watch it fail, implement minimally, watch it pass, commit.
- `ul-next` is created **once per process** — headless render smoke test is a single serial integration test; the windowed `App` path is never created in the same process as a `Renderer` test.
- Ultralight: free indie/non-commercial tier only; do not add code assuming a paid feature.
- No telemetry, no network calls except page traffic the user initiates.

---

### Task 1: Workspace skeleton + `sentinel-core` error & config types

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/sentinel-core/Cargo.toml`
- Create: `crates/sentinel-core/src/lib.rs`
- Create: `crates/sentinel-core/src/error.rs`
- Create: `crates/sentinel-core/src/config.rs`
- Test: inline `#[cfg(test)]` in `error.rs` and `config.rs`

**Interfaces:**
- Produces:
  - `sentinel_core::error::Error` (enum, `thiserror::Error`) with variants `Engine(String)`, `Io(#[from] std::io::Error)`, `Config(String)`, `Navigation(String)`.
  - `sentinel_core::error::Result<T> = std::result::Result<T, Error>`.
  - `sentinel_core::config::BrowserConfig { home_url: String, window_width: u32, window_height: u32, enable_gpu: bool }` with `Default` (home_url = `"about:blank"`, 1280x800, enable_gpu = true) and `fn validate(&self) -> Result<()>` (non-zero dimensions, non-empty home_url).

- [ ] **Step 1: Write the workspace root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/sentinel-core"]

[workspace.package]
edition = "2021"
license = "Proprietary"
authors = ["Sentinel Browser"]

[workspace.dependencies]
thiserror = "1"
serde = { version = "1", features = ["derive"] }
```

- [ ] **Step 2: Write `crates/sentinel-core/Cargo.toml`**

```toml
[package]
name = "sentinel-core"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
thiserror.workspace = true
serde.workspace = true
```

- [ ] **Step 3: Write the failing test for `BrowserConfig::validate`**

In `crates/sentinel-core/src/config.rs`:

```rust
//! Browser-wide configuration types.
//!
//! Public surface: [`BrowserConfig`] — user-tunable settings consumed by the
//! engine and shell. Depends only on `serde` and the crate `error` module.

use serde::{Deserialize, Serialize};
use crate::error::{Error, Result};

/// User-tunable browser settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// URL opened when the browser starts and on "home".
    pub home_url: String,
    /// Initial window width in logical pixels.
    pub window_width: u32,
    /// Initial window height in logical pixels.
    pub window_height: u32,
    /// Prefer the GPU renderer when the platform supports it.
    pub enable_gpu: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self { home_url: "about:blank".into(), window_width: 1280, window_height: 800, enable_gpu: true }
    }
}

impl BrowserConfig {
    /// Validate invariants. Returns [`Error::Config`] when a field is unusable.
    pub fn validate(&self) -> Result<()> {
        if self.home_url.trim().is_empty() {
            return Err(Error::Config("home_url must not be empty".into()));
        }
        if self.window_width == 0 || self.window_height == 0 {
            return Err(Error::Config("window dimensions must be non-zero".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        assert!(BrowserConfig::default().validate().is_ok());
    }

    #[test]
    fn zero_dimensions_rejected() {
        let mut c = BrowserConfig::default();
        c.window_width = 0;
        assert!(matches!(c.validate(), Err(Error::Config(_))));
    }

    #[test]
    fn empty_home_url_rejected() {
        let mut c = BrowserConfig::default();
        c.home_url = "   ".into();
        assert!(matches!(c.validate(), Err(Error::Config(_))));
    }
}
```

- [ ] **Step 4: Write `error.rs`**

```rust
//! Crate-wide error and result types.
//!
//! Public surface: [`Error`] (all failure modes) and [`Result`]. No deps beyond `thiserror`.

use thiserror::Error;

/// All failure modes surfaced by Sentinel's core and engine layers.
#[derive(Debug, Error)]
pub enum Error {
    /// The rendering engine (Ultralight) failed to initialize or operate.
    #[error("engine error: {0}")]
    Engine(String),
    /// An I/O operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Configuration was invalid.
    #[error("config error: {0}")]
    Config(String),
    /// A navigation request was invalid (bad URL, empty history, etc.).
    #[error("navigation error: {0}")]
    Navigation(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 5: Write `lib.rs`**

```rust
//! `sentinel-core`: shared types for the Sentinel browser.
//!
//! Pure-Rust, GUI-free, fully unit-tested. Holds the error type, result alias,
//! and browser configuration consumed by the engine and shell crates.

pub mod config;
pub mod error;

pub use config::BrowserConfig;
pub use error::{Error, Result};
```

- [ ] **Step 6: Run tests, verify they pass**

Run: `cargo test -p sentinel-core`
Expected: `test result: ok. 3 passed`

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/sentinel-core
git commit -m "feat(core): workspace + config/error types with tests"
```

---

### Task 2: Navigation history model (`sentinel-core::nav`)

Pure-Rust back/forward/reload state machine, fully testable without a GUI. The engine later drives Ultralight from this model.

**Files:**
- Create: `crates/sentinel-core/src/nav.rs`
- Modify: `crates/sentinel-core/src/lib.rs` (add `pub mod nav;`)
- Test: inline `#[cfg(test)]` in `nav.rs`

**Interfaces:**
- Produces `sentinel_core::nav::History`:
  - `fn new() -> History` (empty, no current entry).
  - `fn current(&self) -> Option<&str>` — current URL.
  - `fn navigate(&mut self, url: impl Into<String>)` — push a new entry, truncating any forward entries.
  - `fn can_go_back(&self) -> bool`, `fn can_go_forward(&self) -> bool`.
  - `fn back(&mut self) -> Result<&str>` / `fn forward(&mut self) -> Result<&str>` — move cursor, return new current, `Err(Error::Navigation)` if not possible.
  - `fn len(&self) -> usize`, `fn is_empty(&self) -> bool`.

- [ ] **Step 1: Write failing tests**

In `crates/sentinel-core/src/nav.rs`:

```rust
//! In-memory navigation history (back/forward/reload) state machine.
//!
//! Public surface: [`History`]. Pure data structure — the engine drives
//! Ultralight from it; it has no Ultralight dependency and is fully unit-tested.

use crate::error::{Error, Result};

/// A linear navigation history with a movable cursor, like a browser tab.
#[derive(Debug, Default, Clone)]
pub struct History {
    entries: Vec<String>,
    /// Index of the current entry, or `None` when history is empty.
    cursor: Option<usize>,
}

impl History {
    /// Create an empty history with no current entry.
    pub fn new() -> Self { Self { entries: Vec::new(), cursor: None } }

    /// The current URL, if any.
    pub fn current(&self) -> Option<&str> {
        self.cursor.map(|i| self.entries[i].as_str())
    }

    /// Navigate to `url`: drop any forward entries, push, and point the cursor at it.
    pub fn navigate(&mut self, url: impl Into<String>) {
        if let Some(i) = self.cursor {
            self.entries.truncate(i + 1);
        }
        self.entries.push(url.into());
        self.cursor = Some(self.entries.len() - 1);
    }

    /// True if there is an entry behind the cursor.
    pub fn can_go_back(&self) -> bool { matches!(self.cursor, Some(i) if i > 0) }

    /// True if there is an entry ahead of the cursor.
    pub fn can_go_forward(&self) -> bool {
        matches!(self.cursor, Some(i) if i + 1 < self.entries.len())
    }

    /// Move the cursor back one entry and return the new current URL.
    pub fn back(&mut self) -> Result<&str> {
        if !self.can_go_back() {
            return Err(Error::Navigation("no back entry".into()));
        }
        let i = self.cursor.unwrap() - 1;
        self.cursor = Some(i);
        Ok(self.entries[i].as_str())
    }

    /// Move the cursor forward one entry and return the new current URL.
    pub fn forward(&mut self) -> Result<&str> {
        if !self.can_go_forward() {
            return Err(Error::Navigation("no forward entry".into()));
        }
        let i = self.cursor.unwrap() + 1;
        self.cursor = Some(i);
        Ok(self.entries[i].as_str())
    }

    /// Number of entries in history.
    pub fn len(&self) -> usize { self.entries.len() }
    /// True when there are no entries.
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_history_is_empty() {
        let h = History::new();
        assert!(h.is_empty());
        assert_eq!(h.current(), None);
        assert!(!h.can_go_back() && !h.can_go_forward());
    }

    #[test]
    fn navigate_sets_current() {
        let mut h = History::new();
        h.navigate("https://a.com");
        assert_eq!(h.current(), Some("https://a.com"));
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn back_and_forward_traverse() {
        let mut h = History::new();
        h.navigate("https://a.com");
        h.navigate("https://b.com");
        assert_eq!(h.back().unwrap(), "https://a.com");
        assert_eq!(h.current(), Some("https://a.com"));
        assert_eq!(h.forward().unwrap(), "https://b.com");
    }

    #[test]
    fn navigate_truncates_forward() {
        let mut h = History::new();
        h.navigate("https://a.com");
        h.navigate("https://b.com");
        h.back().unwrap();
        h.navigate("https://c.com"); // should drop b.com
        assert!(!h.can_go_forward());
        assert_eq!(h.current(), Some("https://c.com"));
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn back_on_empty_errors() {
        let mut h = History::new();
        assert!(matches!(h.back(), Err(Error::Navigation(_))));
    }
}
```

- [ ] **Step 2: Add `pub mod nav;` to `lib.rs`** (and `pub use nav::History;`)

- [ ] **Step 3: Run tests, verify pass**

Run: `cargo test -p sentinel-core nav`
Expected: `5 passed`

- [ ] **Step 4: Commit**

```bash
git add crates/sentinel-core
git commit -m "feat(core): navigation history state machine with tests"
```

---

### Task 3: `sentinel-engine` crate — wrap `ul-next`, headless render smoke test

This is the first Ultralight-touching code. Building it requires the Ultralight SDK that `ul-next` pulls in; the render smoke test verifies the binding actually produces pixels. If the SDK/toolchain is unavailable in CI, this test is `#[ignore]`d and run manually.

**Files:**
- Create: `crates/sentinel-engine/Cargo.toml`
- Create: `crates/sentinel-engine/src/lib.rs`
- Create: `crates/sentinel-engine/src/headless.rs`
- Modify: workspace `Cargo.toml` members
- Test: `crates/sentinel-engine/tests/render_smoke.rs`

**Interfaces:**
- Consumes: `sentinel_core::{Result, Error}`.
- Produces:
  - `sentinel_engine::headless::render_html_to_rgba(html: &str, width: u32, height: u32) -> sentinel_core::Result<Vec<u8>>` — initializes platform handlers + a CPU `Renderer`, creates a `View`, loads `html`, pumps `update`/`render` until the load finishes, and copies the CPU surface into an RGBA byte buffer. Used by tests and future thumbnailing.

- [ ] **Step 1: Add the crate to the workspace**

Workspace `Cargo.toml` members: `["crates/sentinel-core", "crates/sentinel-engine"]`.

- [ ] **Step 2: Write `crates/sentinel-engine/Cargo.toml`**

```toml
[package]
name = "sentinel-engine"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
sentinel-core = { path = "../sentinel-core" }
ul-next = "0.5.4"
```

- [ ] **Step 3: Write `headless.rs` (CPU render path)**

```rust
//! Headless HTML→pixels rendering via Ultralight's CPU renderer.
//!
//! Public surface: [`render_html_to_rgba`]. Used for render smoke tests and
//! future page thumbnails. Initializes Ultralight platform handlers exactly
//! once per process (Ultralight requires this before `Renderer::create`).

use sentinel_core::{Error, Result};
use ul_next::{
    config::Config,
    platform,
    renderer::Renderer,
    view::ViewConfig,
};

/// Render `html` at `width`x`height` and return a tightly-packed RGBA buffer.
///
/// Errors with [`Error::Engine`] if the renderer or view cannot be created.
/// Note: Ultralight's `Renderer` may be created only once per process, so
/// callers must not mix this with the windowed `App` path in the same process.
pub fn render_html_to_rgba(html: &str, width: u32, height: u32) -> Result<Vec<u8>> {
    // Platform handlers must be set before creating the renderer.
    platform::enable_platform_fontloader();
    platform::enable_platform_filesystem(".")
        .map_err(|e| Error::Engine(format!("filesystem platform init: {e:?}")))?;
    platform::enable_default_logger("./ultralight.log")
        .map_err(|e| Error::Engine(format!("logger init: {e:?}")))?;

    let config = Config::start().build().map_err(|e| Error::Engine(format!("config: {e:?}")))?;
    let renderer = Renderer::create(config).map_err(|e| Error::Engine(format!("renderer: {e:?}")))?;

    let view = renderer
        .create_view(width, height, &ViewConfig::start().is_accelerated(false).build()
            .map_err(|e| Error::Engine(format!("view config: {e:?}")))?, None)
        .ok_or_else(|| Error::Engine("create_view returned None".into()))?;

    view.load_html(html).map_err(|e| Error::Engine(format!("load_html: {e:?}")))?;

    // Pump the renderer until the page finishes loading (bounded to avoid hangs).
    for _ in 0..600 {
        renderer.update();
        renderer.render();
        if view.is_loading() == false {
            break;
        }
    }

    let surface = view.surface().ok_or_else(|| Error::Engine("no CPU surface".into()))?;
    let bitmap = surface.bitmap();
    let pixels = bitmap.pixels();
    Ok(pixels.to_vec())
}
```

> NOTE: exact `ul-next` 0.5.4 method names (`enable_platform_fontloader`, `ViewConfig::start`, `surface().bitmap().pixels()`) must be confirmed against `cargo doc` at implementation time; adjust to the real API if a name differs. Keep the public signature stable.

- [ ] **Step 4: Write `lib.rs`**

```rust
//! `sentinel-engine`: Ultralight integration for Sentinel.
//!
//! Wraps `ul-next` 0.5.4. Provides a headless CPU render path (this module)
//! and, later, the windowed browser view. Engine errors map to
//! [`sentinel_core::Error::Engine`].

pub mod headless;
```

- [ ] **Step 5: Write the render smoke test**

`crates/sentinel-engine/tests/render_smoke.rs`:

```rust
//! Verifies Ultralight actually renders: a red page must produce non-blank pixels.
//! `#[ignore]` by default because it needs the Ultralight SDK + may need a GPU/driver;
//! run with `cargo test -p sentinel-engine -- --ignored`.

#[test]
#[ignore = "requires Ultralight SDK; run manually with --ignored"]
fn red_page_produces_non_blank_pixels() {
    let html = "<html><body style='margin:0;background:#ff0000'></body></html>";
    let rgba = sentinel_engine::headless::render_html_to_rgba(html, 64, 64).expect("render");
    assert_eq!(rgba.len(), 64 * 64 * 4);
    // At least one pixel should be reddish (R high, G/B low). Surface may be BGRA;
    // accept either channel order by checking a strong single-channel dominance.
    let has_red = rgba.chunks_exact(4).any(|p| {
        let max = *p[..3].iter().max().unwrap();
        let min = *p[..3].iter().min().unwrap();
        max > 200 && min < 80
    });
    assert!(has_red, "expected a strongly-colored pixel, surface was blank");
}
```

- [ ] **Step 6: Build the engine crate (compile check)**

Run: `cargo build -p sentinel-engine`
Expected: compiles. If `ul-next` cannot fetch the Ultralight SDK in this environment, document the failure and the manual SDK setup step; do not fake success.

- [ ] **Step 7: Run the smoke test manually (where SDK + display exist)**

Run: `cargo test -p sentinel-engine -- --ignored`
Expected: PASS (`red_page_produces_non_blank_pixels`). Record the result honestly.

- [ ] **Step 8: Commit**

```bash
git add crates/sentinel-engine Cargo.toml
git commit -m "feat(engine): headless Ultralight CPU render + smoke test"
```

---

### Task 4: `sentinel-shell` binary — window, navigation wiring

The actual browser window using the high-level `App`/`Window`/`Overlay` API, driven by the `History` model from Task 2. GUI verification is manual (needs a display).

**Files:**
- Create: `crates/sentinel-shell/Cargo.toml`
- Create: `crates/sentinel-shell/src/main.rs`
- Create: `crates/sentinel-shell/src/browser.rs`
- Modify: workspace `Cargo.toml` members
- Test: inline tests for the non-GUI parts of `browser.rs` (URL normalization)

**Interfaces:**
- Consumes: `sentinel_core::{BrowserConfig, nav::History}`, `ul_next::{app::App, window::WindowFlags, Overlay}`.
- Produces:
  - `sentinel_shell::browser::normalize_url(input: &str) -> String` — turns `"example.com"` into `"https://example.com"`, leaves valid schemes alone, maps empty → `"about:blank"`. Pure, unit-tested.
  - `sentinel_shell::browser::Browser` — owns the `App`, `Window`, `Overlay`, and a `History`; methods `go(&mut self, url)`, `back`, `forward`, `reload`, wired to the overlay's `View`.

- [ ] **Step 1: Add crate to workspace members.**

- [ ] **Step 2: Write `crates/sentinel-shell/Cargo.toml`**

```toml
[package]
name = "sentinel-shell"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[[bin]]
name = "sentinel"
path = "src/main.rs"

[dependencies]
sentinel-core = { path = "../sentinel-core" }
sentinel-engine = { path = "../sentinel-engine" }
ul-next = { version = "0.5.4", features = ["appcore_linked"] }
```

- [ ] **Step 3: Write the failing test for `normalize_url`**

In `crates/sentinel-shell/src/browser.rs` (only the `normalize_url` fn + tests for now):

```rust
//! Browser window controller: owns the Ultralight `App`/`Window`/`Overlay` and
//! drives them from the pure-Rust `History` model.
//!
//! Public surface: [`normalize_url`] (pure, tested) and [`Browser`] (GUI, manual).

/// Normalize raw address-bar input into a loadable URL.
///
/// - Empty/whitespace → `"about:blank"`.
/// - Already has a scheme (`http:`, `https:`, `file:`, `about:`) → unchanged.
/// - Otherwise → prefixed with `https://`.
pub fn normalize_url(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return "about:blank".into();
    }
    let has_scheme = ["http:", "https:", "file:", "about:"]
        .iter()
        .any(|p| s.starts_with(p));
    if has_scheme { s.to_string() } else { format!("https://{s}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_host_gets_https() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
    }
    #[test]
    fn scheme_preserved() {
        assert_eq!(normalize_url("http://x.test"), "http://x.test");
        assert_eq!(normalize_url("about:blank"), "about:blank");
    }
    #[test]
    fn empty_is_blank() {
        assert_eq!(normalize_url("   "), "about:blank");
    }
}
```

- [ ] **Step 4: Run the test, verify pass**

Run: `cargo test -p sentinel-shell normalize`
Expected: `3 passed`

- [ ] **Step 5: Implement `Browser` (GUI) and `main.rs`**

`Browser` holds `App`, `Window`, `Overlay`, `History`. On `go`, it calls `normalize_url`, `history.navigate`, and `overlay.view().load_url(...)`. `main.rs` builds a `BrowserConfig`, creates the `Browser`, loads `config.home_url`, and calls `app.run()`. Use the `basic_app` example shape from `ul-next` as the reference for `App::new`, `Window`, and `Overlay::new(window, w, h, x, y)`.

```rust
// main.rs
//! Sentinel browser entry point. Builds config, opens the window, runs the loop.

use sentinel_core::BrowserConfig;

fn main() -> sentinel_core::Result<()> {
    let config = BrowserConfig::default();
    config.validate()?;
    let mut browser = sentinel_shell::browser::Browser::new(&config)?;
    browser.go(&config.home_url)?;
    browser.run(); // blocks in app.run()
    Ok(())
}
```

- [ ] **Step 6: Build the shell**

Run: `cargo build -p sentinel-shell`
Expected: compiles (needs SDK). Document honestly if SDK fetch is unavailable here.

- [ ] **Step 7: Manual GUI verification (on a machine with a display + SDK)**

Run: `cargo run -p sentinel-shell`
Expected: a window opens; `about:blank` then a navigated page renders; back/forward/reload work. Record the result and a screenshot.

- [ ] **Step 8: Commit**

```bash
git add crates/sentinel-shell Cargo.toml
git commit -m "feat(shell): browser window + navigation wired to History"
```

---

## Self-Review

**Spec coverage (Phase 1 portion of the spec):**
- Cargo workspace ✓ Task 1. Ultralight integration ✓ Task 3. Window + load URL + render ✓ Task 4 + Task 3. Run page JS — JS execution is inherent to Ultralight loading a page; an explicit `evaluate_script` wrapper is deferred to Phase 3 (bridge) where it is actually needed, to honor YAGNI. Navigation (back/forward/reload/address bar) ✓ Task 2 (model) + Task 4 (wiring). Tabs — deferred to a Phase 1.1 follow-up plan to keep this plan to a verifiable core; the `History`-per-tab model already supports multi-tab.
- Documentation requirement ✓ every file has `//!`, every public item `///`.
- Verification requirement ✓ pure-Rust tasks (1, 2, and `normalize_url` in 4) are CI-verifiable now; engine/GUI tasks (3, 4) have explicit manual verification steps with honest reporting.

**Placeholder scan:** No "TBD/TODO/handle edge cases". The one flagged uncertainty (exact `ul-next` method names in Task 3) is explicitly called out with the instruction to confirm against `cargo doc` and keep the signature stable — this is honest, not a hidden placeholder.

**Type consistency:** `Error::{Engine,Io,Config,Navigation}`, `Result<T>`, `History` methods, `BrowserConfig` fields, `normalize_url`, `render_html_to_rgba` signatures are consistent across tasks.

**Known gap (tracked):** Tabs and an explicit JS-eval wrapper are deferred with rationale above; both have a clear home in later plans.
