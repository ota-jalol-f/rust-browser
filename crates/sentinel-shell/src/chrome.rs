//! The browser chrome: a tab strip + address bar rendered as its own Ultralight
//! view above the content view. It drives navigation and tab management by calling
//! the Rust-installed `window.__sentinelNav(cmd)` (see
//! `sentinel_engine::page::PageView::install_nav_now`), which enqueues commands the
//! host loop applies to the active tab / tab list:
//!   `nav <url>`, `back`, `forward`, `reload`, `newtab`, `selecttab <i>`, `closetab <i>`.
//! Rust pushes state back with `window.setUrl(s)` and `window.setTabs(arr)`.
//!
//! Kept to plain ASCII glyphs (color emoji crash the renderer's font path).

/// Logical height of the chrome strip, in points (scaled by DPI at runtime):
/// 34px tab strip + 44px address bar.
pub const BAR_HEIGHT: f64 = 78.0;

/// The chrome HTML. `window.setUrl(s)` syncs the shown URL; `window.setTabs(arr)`
/// renders the tab strip from `[{title, active}]`.
pub const CHROME_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
  html, body { margin: 0; height: 100%; overflow: hidden; background: #20242b;
    font-family: "Segoe UI", Arial, sans-serif; }
  #tabs { display: flex; align-items: stretch; height: 34px; padding: 4px 6px 0 6px; gap: 4px;
    box-sizing: border-box; overflow: hidden; }
  .tab { display: flex; align-items: center; max-width: 180px; min-width: 60px; height: 30px;
    padding: 0 6px 0 10px; background: #2d333b; color: #adbac7; border: 1px solid #444c56;
    border-bottom: none; border-radius: 6px 6px 0 0; cursor: pointer; font-size: 12px; gap: 6px; }
  .tab.active { background: #0d1117; color: #e6edf3; }
  .tab .label { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tab .close { background: transparent; border: none; color: #768390; cursor: pointer;
    font-size: 14px; line-height: 1; padding: 0 2px; min-width: 0; height: auto; }
  .tab .close:hover { color: #e6edf3; }
  #newtab { background: #2d333b; color: #cdd9e5; border: 1px solid #444c56; border-radius: 6px;
    height: 30px; min-width: 30px; cursor: pointer; font-size: 16px; }
  #newtab:hover { background: #373e47; }
  #bar { display: flex; align-items: center; height: 44px; padding: 0 8px; gap: 6px; }
  #bar button { background: #2d333b; color: #cdd9e5; border: 1px solid #444c56; border-radius: 6px;
    height: 28px; min-width: 30px; cursor: pointer; font-size: 14px; }
  #bar button:hover { background: #373e47; }
  #url { flex: 1; height: 28px; background: #0d1117; color: #e6edf3; border: 1px solid #444c56;
    border-radius: 6px; padding: 0 10px; font-size: 13px; outline: none; }
</style></head><body>
  <div id="tabs"></div>
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
  window.setTabs = function (arr) {
    var box = document.getElementById('tabs');
    box.innerHTML = '';
    arr.forEach(function (t, i) {
      var tab = document.createElement('div');
      tab.className = 'tab' + (t.active ? ' active' : '');
      var label = document.createElement('span');
      label.className = 'label';
      label.textContent = t.title || 'New Tab';
      label.onclick = function () { cmd('selecttab ' + i); };
      tab.appendChild(label);
      if (arr.length > 1) {
        var x = document.createElement('button');
        x.className = 'close';
        x.textContent = '×';
        x.onclick = function (e) { e.stopPropagation(); cmd('closetab ' + i); };
        tab.appendChild(x);
      }
      box.appendChild(tab);
    });
    var plus = document.createElement('button');
    plus.id = 'newtab';
    plus.textContent = '+';
    plus.title = 'New tab';
    plus.onclick = function () { cmd('newtab'); };
    box.appendChild(plus);
  };
</script></body></html>
"##;
