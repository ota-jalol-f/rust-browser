# sentinel-engine

Ultralight integration for Sentinel. Wraps `ul-next` 0.5.4.

## Boundary

- **Input:** HTML/URLs + a window/size from `sentinel-shell`; config from `sentinel-core`.
- **Output:** rendered pixels (headless path) and, later, a live windowed `View`.
- **Depends on:** `sentinel-core` (errors/config) and `ul-next` (Ultralight bindings).

## The `ultralight` feature

The Ultralight SDK is proprietary and cannot be vendored here, so all SDK-touching
code lives behind the **off-by-default** `ultralight` Cargo feature. Without it the
crate compiles to an empty public surface, keeping the workspace buildable/testable
on machines without the SDK.

```bash
# Compiles the real engine code against ul-next (needs network for the crate;
# does NOT need the SDK at build time because `loaded` mode dlopens at runtime):
cargo build -p sentinel-engine --features ultralight

# Run the render smoke test (needs the SDK shared libraries on the path at runtime):
cargo test -p sentinel-engine --features ultralight -- --ignored
```

## Runtime setup (required to actually render)

`ul-next` is built with its `loaded` feature: the Ultralight shared libraries are
loaded at runtime via `libloading`. Obtain them from the
[Ultralight SDK](https://ultralig.ht/) and place them where the OS loader finds them:

- **Windows:** `UltralightCore.dll`, `WebCore.dll`, `Ultralight.dll`, `AppCore.dll`
  next to `sentinel.exe` (or on `PATH`).
- The SDK's `resources/` folder (cacert.pem, ICU data) must be reachable from the
  working directory for full page loading.

Licensing: Ultralight is free for non-commercial and indie commercial use
(< $100K annual revenue); a paid license is required otherwise.
