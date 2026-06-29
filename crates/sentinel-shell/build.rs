//! Build script: stage the Ultralight SDK next to the executable.
//!
//! `ul-next-sys` downloads the exact Ultralight SDK build that `ul-next` is pinned
//! to, into its own `OUT_DIR` (`.../build/ul-next-sys-<hash>/out/ul-sdk`). In
//! `loaded` mode those DLLs are dlopen'd at runtime, so they must sit next to
//! `sentinel.exe`, and AppCore needs the `resources/` folder reachable too.
//!
//! This script copies both into the active profile directory (`target/<profile>`)
//! whenever the `ultralight` feature is enabled, so `cargo run --features ultralight`
//! works without any manual setup and survives `cargo clean`.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Only stage the SDK when the GUI feature (and thus ul-next-sys) is present.
    if env::var_os("CARGO_FEATURE_ULTRALIGHT").is_none() {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
    // OUT_DIR = target/<profile>/build/sentinel-shell-<hash>/out
    //           out(0) shell-hash(1) build(2) <profile>(3)
    let build_dir = match out_dir.ancestors().nth(2) {
        Some(p) => p.to_path_buf(),
        None => return,
    };
    let profile_dir = match out_dir.ancestors().nth(3) {
        Some(p) => p.to_path_buf(),
        None => return,
    };

    let Some(sdk) = find_sdk(&build_dir) else {
        println!("cargo:warning=Ultralight SDK not found under {build_dir:?}; the GUI will fail to load its DLLs at runtime.");
        return;
    };

    // Copy the runtime DLLs next to the executable.
    if let Ok(entries) = fs::read_dir(sdk.join("bin")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("dll") {
                copy_if_newer(&path, &profile_dir.join(entry.file_name()));
            }
        }
    }

    // Copy the resources folder (cacert.pem, ICU data) next to the executable.
    let res_src = sdk.join("resources");
    if res_src.is_dir() {
        copy_dir(&res_src, &profile_dir.join("resources"));
    }
}

/// Find `ul-next-sys-*/out/ul-sdk` under the cargo `build/` directory.
fn find_sdk(build_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(build_dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("ul-next-sys-") {
            let candidate = entry.path().join("out").join("ul-sdk");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Copy `src` to `dst` unless `dst` is already up to date (cheap incremental builds).
fn copy_if_newer(src: &Path, dst: &Path) {
    let up_to_date = matches!(
        (fs::metadata(src).and_then(|m| m.modified()), fs::metadata(dst).and_then(|m| m.modified())),
        (Ok(s), Ok(d)) if d >= s
    );
    if !up_to_date {
        let _ = fs::copy(src, dst);
    }
}

/// Recursively copy a directory (one level deep is enough for `resources/`).
fn copy_dir(src: &Path, dst: &Path) {
    let _ = fs::create_dir_all(dst);
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let target = dst.join(entry.file_name());
            if path.is_dir() {
                copy_dir(&path, &target);
            } else {
                copy_if_newer(&path, &target);
            }
        }
    }
}
