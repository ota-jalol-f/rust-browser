//! The browser chrome: a thin address-bar strip rendered as its own Ultralight
//! view above the content view. It drives navigation by calling the Rust-installed
//! `window.__sentinelNav(cmd)` (see `sentinel_engine::page::PageView::install_nav_bridge`),
//! which enqueues commands the host loop applies to the content view.
//!
//! Kept to plain ASCII glyphs (color emoji crash the renderer's font path).

/// Logical height of the chrome strip, in points (scaled by DPI at runtime).
pub const BAR_HEIGHT: f64 = 44.0;

/// The address-bar HTML. `window.setUrl(s)` lets Rust sync the shown URL.
pub const CHROME_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html, body { margin: 0; height: 100%; overflow: hidden; background: #20242b;
    font-family: "Segoe UI", Arial, sans-serif; }
  #bar { display: flex; align-items: center; height: 44px; padding: 0 8px; gap: 6px; }
  button { background: #2d333b; color: #cdd9e5; border: 1px solid #444c56; border-radius: 6px;
    height: 28px; min-width: 30px; cursor: pointer; font-size: 14px; }
  button:hover { background: #373e47; }
  #url { flex: 1; height: 28px; background: #0d1117; color: #e6edf3; border: 1px solid #444c56;
    border-radius: 6px; padding: 0 10px; font-size: 13px; outline: none; }
</style></head><body>
  <div id="bar">
    <button onclick="cmd('back')" title="Back">&lt;</button>
    <button onclick="cmd('forward')" title="Forward">&gt;</button>
    <button onclick="cmd('reload')" title="Reload">Reload</button>
    <input id="url" placeholder="Enter a URL and press Enter" />
    <button onclick="go()" title="Go">Go</button>
  </div>
<script>
  var u = document.getElementById('url');
  function cmd(c) { if (window.__sentinelNav) window.__sentinelNav(c); }
  function go() { if (window.__sentinelNav) window.__sentinelNav('nav ' + u.value); }
  u.addEventListener('keydown', function (e) { if (e.key === 'Enter') { e.preventDefault(); go(); } });
  window.setUrl = function (s) { u.value = s; };
</script></body></html>
"##;
