//! The built-in start page: a visible dashboard the user can SEE and interact with
//! to verify the browser works — rendering, the `window.native` bridge, live device
//! enumeration, and a JS console (DevTools-lite).
//!
//! Loaded via `Browser::show_html`. Its scripts run before `window.native` is
//! installed (DOM-ready installs it), so device population happens in
//! [`window.onSentinelReady`], which the bridge calls right after install.

/// The dashboard HTML (self-contained: inline CSS + JS, no external resources).
pub const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sentinel</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body {
    margin: 0; font-family: "Segoe UI", system-ui, sans-serif;
    background: #0e1116; color: #e6edf3;
    padding: 28px; line-height: 1.45;
  }
  h1 { margin: 0 0 4px; font-size: 26px; letter-spacing: .2px; }
  h1 .shield { color: #58a6ff; }
  .sub { color: #8b949e; margin-bottom: 22px; font-size: 13px; }
  .badge {
    display: inline-block; background: #1f6feb22; color: #58a6ff;
    border: 1px solid #1f6feb55; border-radius: 999px;
    padding: 2px 10px; font-size: 12px; margin-left: 6px;
  }
  .card {
    background: #161b22; border: 1px solid #30363d; border-radius: 10px;
    padding: 18px 20px; margin-bottom: 18px;
  }
  .card h2 { margin: 0 0 12px; font-size: 15px; color: #adbac7; font-weight: 600; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th, td { text-align: left; padding: 6px 10px; border-bottom: 1px solid #21262d; }
  th { color: #8b949e; font-weight: 500; }
  code, pre, textarea { font-family: "Cascadia Code", Consolas, monospace; }
  textarea {
    width: 100%; height: 70px; background: #0d1117; color: #e6edf3;
    border: 1px solid #30363d; border-radius: 8px; padding: 10px; font-size: 13px; resize: vertical;
  }
  button {
    background: #238636; color: #fff; border: 0; border-radius: 8px;
    padding: 8px 16px; font-size: 13px; cursor: pointer; margin-top: 10px;
  }
  button:hover { background: #2ea043; }
  pre.out {
    background: #0d1117; border: 1px solid #30363d; border-radius: 8px;
    padding: 12px; margin-top: 12px; min-height: 24px; white-space: pre-wrap;
    word-break: break-word; font-size: 13px; color: #7ee787;
  }
  .muted { color: #8b949e; font-size: 12px; }
  .ok { color: #7ee787; } .warn { color: #d29922; } .err { color: #f85149; }
</style>
</head>
<body>
  <h1><span class="shield">🛡</span> Sentinel <span class="badge" id="ver">bridge: …</span></h1>
  <div class="sub">Lightweight privacy browser — built-in dashboard. If you can read this, rendering works.</div>

  <div class="card">
    <h2>Native bridge &amp; devices</h2>
    <div id="bridgeStatus" class="muted">Waiting for window.native …</div>
    <table id="usbTable" style="display:none">
      <thead><tr><th>#</th><th>Vendor</th><th>Product</th><th>Manufacturer</th><th>Product name</th></tr></thead>
      <tbody id="usbBody"></tbody>
    </table>
    <div id="serialLine" class="muted" style="margin-top:10px"></div>
  </div>

  <div class="card">
    <h2>JS console <span class="muted">— type JavaScript, press Run (try: <code>window.native.usb.list()</code>)</span></h2>
    <textarea id="consoleIn">window.native.usb.list()</textarea>
    <div><button onclick="runConsole()">Run</button></div>
    <pre class="out" id="consoleOut"></pre>
  </div>

  <div class="sub muted">Native calls are permission-gated. This built-in page is a trusted origin and is pre-granted, so you see live results.</div>

<script>
  function hex(n) { return '0x' + (n >>> 0).toString(16).padStart(4, '0'); }

  function runConsole() {
    var out = document.getElementById('consoleOut');
    var src = document.getElementById('consoleIn').value;
    try {
      // eval() is intentional: this is a DevTools-style JS console in the trusted
      // built-in page, running the user's own typed expression so they can verify
      // window.native and page state. Not used on untrusted input.
      var r = eval(src);
      out.className = 'out ok';
      out.textContent = (typeof r === 'object') ? JSON.stringify(r, null, 2) : String(r);
    } catch (e) {
      out.className = 'out err';
      out.textContent = 'Error: ' + e;
    }
  }

  // Called by the bridge right after window.native is installed.
  window.onSentinelReady = function () {
    document.getElementById('ver').textContent = 'bridge: v' + window.native.version;
    var status = document.getElementById('bridgeStatus');
    try {
      var u = window.native.usb.list();
      if (u && u.status === 'ok' && u.response && Array.isArray(u.response.data)) {
        var devs = u.response.data;
        status.className = 'ok';
        status.textContent = '✓ window.native works — ' + devs.length + ' USB device(s) enumerated live:';
        var body = document.getElementById('usbBody');
        body.innerHTML = '';
        devs.forEach(function (d, i) {
          var tr = document.createElement('tr');
          tr.innerHTML = '<td>' + (i + 1) + '</td><td>' + hex(d.vendor_id) + '</td><td>' + hex(d.product_id) +
            '</td><td>' + (d.manufacturer || '—') + '</td><td>' + (d.product || '—') + '</td>';
          body.appendChild(tr);
        });
        document.getElementById('usbTable').style.display = '';
      } else if (u && u.status === 'needs_consent') {
        status.className = 'warn';
        status.textContent = '⚠ window.native present, but USB needs consent (no prompt UI yet): ' + JSON.stringify(u);
      } else {
        status.className = 'warn';
        status.textContent = 'window.native present. usb.list() → ' + JSON.stringify(u);
      }
      var s = window.native.serial.list();
      if (s && s.status === 'ok' && s.response) {
        document.getElementById('serialLine').textContent = 'Serial ports: ' + JSON.stringify(s.response.data);
      }
    } catch (e) {
      status.className = 'err';
      status.textContent = 'window.native call failed: ' + e;
    }
  };

  // If the bridge was already installed before this script ran, populate now.
  if (window.native) { window.onSentinelReady(); }
</script>
</body>
</html>
"##;
