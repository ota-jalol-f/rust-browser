//! The built-in start page: a visible dashboard the user can SEE and interact with
//! to verify the browser works — rendering, live device enumeration, and a JS
//! console (DevTools-lite).
//!
//! Kept deliberately simple (no emoji / exotic glyphs — those crash the renderer's
//! font path). The live device list is enumerated by the Rust device host at
//! startup and injected into [`DEVICES_PLACEHOLDER`] before the page loads, so it
//! shows real hardware without needing the (currently disabled) live JS bridge.

/// Token in [`DASHBOARD_HTML`] replaced with a JSON `{usb, serial}` device snapshot.
pub const DEVICES_PLACEHOLDER: &str = "__SENTINEL_DEVICES_JSON__";

/// The dashboard HTML (self-contained: inline CSS + JS, no external resources).
pub const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sentinel</title>
<style>
  * { box-sizing: border-box; }
  body { margin: 0; font-family: "Segoe UI", Arial, sans-serif; background: #0e1116; color: #e6edf3; padding: 28px; line-height: 1.45; }
  h1 { margin: 0 0 4px; font-size: 26px; color: #58a6ff; }
  .sub { color: #8b949e; margin-bottom: 22px; font-size: 13px; }
  .card { background: #161b22; border: 1px solid #30363d; border-radius: 10px; padding: 18px 20px; margin-bottom: 18px; }
  .card h2 { margin: 0 0 12px; font-size: 15px; color: #adbac7; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th, td { text-align: left; padding: 6px 10px; border-bottom: 1px solid #21262d; }
  th { color: #8b949e; }
  code, pre, textarea { font-family: Consolas, monospace; }
  textarea { width: 100%; height: 64px; background: #0d1117; color: #e6edf3; border: 1px solid #30363d; border-radius: 8px; padding: 10px; font-size: 13px; }
  button { background: #238636; color: #fff; border: 0; border-radius: 8px; padding: 8px 16px; font-size: 13px; cursor: pointer; margin-top: 10px; }
  pre.out { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px; margin-top: 12px; min-height: 22px; white-space: pre-wrap; word-break: break-word; font-size: 13px; color: #7ee787; }
  .muted { color: #8b949e; font-size: 12px; }
  .ok { color: #7ee787; } .warn { color: #d29922; } .err { color: #f85149; }
</style>
</head>
<body>
  <h1>Sentinel</h1>
  <div class="sub">Lightweight privacy browser - built-in dashboard. If you can read this, rendering works.</div>

  <div class="card">
    <h2>Live devices <span class="muted">- enumerated by the Rust device host (nusb / serialport) at startup</span></h2>
    <div id="devStatus" class="muted">...</div>
    <table id="usbTable" style="display:none">
      <thead><tr><th>#</th><th>Vendor</th><th>Product</th><th>Manufacturer</th><th>Product name</th></tr></thead>
      <tbody id="usbBody"></tbody>
    </table>
    <div id="serialLine" class="muted" style="margin-top:10px"></div>
  </div>

  <div class="card">
    <h2>JS console <span class="muted">- type JavaScript, press Run</span></h2>
    <textarea id="consoleIn">navigator.userAgent</textarea>
    <div><button onclick="runConsole()">Run</button></div>
    <pre class="out" id="consoleOut"></pre>
  </div>

  <div class="sub muted">The device list above proves the device-host stack end to end. The live
  window.native JS bridge is temporarily disabled (its DOM-ready install crashes on AppCore views);
  it is proven by the automated tests and being fixed separately.</div>

<script>
  var SENTINEL_DEVICES = __SENTINEL_DEVICES_JSON__;

  function hex(n) { return '0x' + (n >>> 0).toString(16); }

  function runConsole() {
    var out = document.getElementById('consoleOut');
    var src = document.getElementById('consoleIn').value;
    try {
      // eval() is intentional: a DevTools-style JS console in the trusted built-in
      // page, running the user's own typed expression. Not used on untrusted input.
      var r = eval(src);
      out.className = 'out ok';
      out.textContent = (typeof r === 'object') ? JSON.stringify(r, null, 2) : String(r);
    } catch (e) {
      out.className = 'out err';
      out.textContent = 'Error: ' + e;
    }
  }

  function renderDevices() {
    var status = document.getElementById('devStatus');
    var d = SENTINEL_DEVICES;
    if (!d) { status.className = 'warn'; status.textContent = 'No device snapshot injected.'; return; }
    try {
      var usb = d.usb && d.usb.data ? d.usb.data : [];
      status.className = 'ok';
      status.textContent = usb.length + ' USB device(s) enumerated live:';
      var body = document.getElementById('usbBody');
      for (var i = 0; i < usb.length; i++) {
        var dev = usb[i];
        var tr = document.createElement('tr');
        tr.innerHTML = '<td>' + (i + 1) + '</td><td>' + hex(dev.vendor_id) + '</td><td>' + hex(dev.product_id) +
          '</td><td>' + (dev.manufacturer || '-') + '</td><td>' + (dev.product || '-') + '</td>';
        body.appendChild(tr);
      }
      if (usb.length) { document.getElementById('usbTable').style.display = ''; }
      var ports = d.serial && d.serial.data ? d.serial.data : [];
      document.getElementById('serialLine').textContent = 'Serial ports: ' + (ports.length ? ports.join(', ') : '(none)');
    } catch (e) {
      status.className = 'err';
      status.textContent = 'Failed to render devices: ' + e;
    }
  }

  renderDevices();
</script>
</body>
</html>
"##;
