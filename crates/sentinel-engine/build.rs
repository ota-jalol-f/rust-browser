//! Build script: stage the Ultralight SDK next to the executables that use it.
//!
//! `ul-next-sys` downloads the exact pinned Ultralight SDK into its `OUT_DIR`
//! (`.../build/ul-next-sys-<hash>/out/ul-sdk`). The DLLs must sit next to whatever
//! binary loads them, and Ultralight reads `<exe-dir>/resources/`. We stage both
//! the runtime DLLs and the `resources/` folder into the profile directory
//! (`target/<profile>`, where app binaries live) **and** its `deps/` subdir (where
//! `cargo test` integration-test binaries live), so both `cargo run` and
//! `cargo test --features ultralight` work without manual setup.
//!
//! Only runs when the `ultralight` feature is enabled (i.e. ul-next-sys is built).

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if env::var_os("CARGO_FEATURE_ULTRALIGHT").is_none() {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
    // OUT_DIR = target/<profile>/build/<thiscrate>-<hash>/out
    let build_dir = match out_dir.ancestors().nth(2) {
        Some(p) => p.to_path_buf(),
        None => return,
    };
    let profile_dir = match out_dir.ancestors().nth(3) {
        Some(p) => p.to_path_buf(),
        None => return,
    };

    let Some(sdk) = find_sdk(&build_dir) else {
        println!("cargo:warning=Ultralight SDK not found under {build_dir:?}; engine binaries will fail to load it at runtime.");
        return;
    };

    // Stage into the profile dir (app exes) and deps/ (test exes).
    for dest in [profile_dir.clone(), profile_dir.join("deps")] {
        let _ = fs::create_dir_all(&dest);
        stage_dlls(&sdk.join("bin"), &dest);
        copy_dir(&sdk.join("resources"), &dest.join("resources"));
    }
}

/// Copy every `*.dll` from `bin_dir` into `dest`.
fn stage_dlls(bin_dir: &Path, dest: &Path) {
    if let Ok(entries) = fs::read_dir(bin_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("dll") {
                copy_if_newer(&path, &dest.join(entry.file_name()));
            }
        }
    }
}

/// Find `ul-next-sys-*/out/ul-sdk` under the cargo `build/` directory.
fn find_sdk(build_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(build_dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with("ul-next-sys-") {
            let candidate = entry.path().join("out").join("ul-sdk");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Copy `src` to `dst` unless `dst` is already up to date.
fn copy_if_newer(src: &Path, dst: &Path) {
    let up_to_date = matches!(
        (fs::metadata(src).and_then(|m| m.modified()), fs::metadata(dst).and_then(|m| m.modified())),
        (Ok(s), Ok(d)) if d >= s
    );
    if !up_to_date {
        let _ = fs::copy(src, dst);
    }
}

/// Recursively copy a directory.
fn copy_dir(src: &Path, dst: &Path) {
    if !src.is_dir() {
        return;
    }
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
