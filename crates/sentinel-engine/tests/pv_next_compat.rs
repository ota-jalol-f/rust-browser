//! Engine-compatibility probe: can Ultralight actually boot the pv-next Vue 3 SPA
//! (ES modules, modulepreload, Rolldown bundler output)?
//!
//! `#[ignore]` because it needs the Ultralight SDK at runtime and network access.
//! Run with (defaults to the live site; override with PROBE_URL):
//!   cargo test -p sentinel-engine --features ultralight --test pv_next_compat -- --ignored --nocapture
//!
//! Verdict heuristic: if `loaderPresent` stays true and `appChildren<=1` after the
//! settle budget, the SPA never mounted (likely ES-module / engine incompatibility).
//! If the loader is gone / real content/children appear, it booted.

#![cfg(feature = "ultralight")]

use std::time::Duration;

#[test]
#[ignore = "requires Ultralight SDK + network; run with --ignored --nocapture"]
fn pv_next_boots_in_ultralight() {
    let url = std::env::var("PROBE_URL").unwrap_or_else(|_| "https://pasport-viza.uz".to_string());
    let budget_secs: u64 = std::env::var("PROBE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(25);

    println!("PROBE_URL={url} PROBE_SECS={budget_secs}");
    let out = sentinel_engine::headless::probe_url(&url, 1280, 800, Duration::from_secs(budget_secs))
        .expect("probe_url should run when the SDK is present");
    println!("PROBE_RESULT {out}");

    // Dump the tail of Ultralight's own log (JS console errors, module failures,
    // network errors land here via the default logger installed in init_renderer).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let log = dir.join("ultralight.log");
            match std::fs::read_to_string(&log) {
                Ok(s) => {
                    let lines: Vec<&str> = s.lines().collect();
                    let start = lines.len().saturating_sub(120);
                    println!("ULTRALIGHT_LOG_TAIL ({}):\n{}", log.display(), lines[start..].join("\n"));
                }
                Err(e) => println!("ULTRALIGHT_LOG_READ_ERR {}: {e}", log.display()),
            }
        }
    }
}
