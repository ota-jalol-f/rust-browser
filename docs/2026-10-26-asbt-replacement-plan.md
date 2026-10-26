# Sentinel → ASBT boshqariladigan brauzerining Win7 o'rinbosari: reja

Sana: 2026-10-26 (yangilangan: .NET'siz, butun Rust stack) · Muallif: muhokama (OtaJalol + Claude)

Bu hujjat `E:\Experiments\rust-browser` (Sentinel) ni `E:\Experiments\browser`
(Asbr.MyBrowser, .NET 10 + WebView2) referensining **ishchi darajasiga** yetkazish va unga
**plugin tizimini** qo'shish bo'yicha fazali rejani belgilaydi.

## 0. Qabul qilingan qarorlar (gating decisions)

| Qaror | Tanlov | Oqibati |
|---|---|---|
| Windows 7 | **Qat'iy talab** | Engine Ultralight bo'lib qoladi (WebView2/CEF Win7'ni Chromium 109 dan keyin tashlagan). |
| Maqsad | **Aniq o'rinbosar** (drop-in) | pv-next Vue ilovasi va mavjud hardware pluginlar o'zgarmay ishlashi kerak. JS yuzasi mos bo'lishi shart. |
| Plugin stack | **Butun Rust/native, .NET'siz** | .NET runtime dependency yo'q, yagona Rust stack, kichikroq paket. Barcha pluginlar Rust'da yoki native vendor SDK'ga FFI orqali. |

**Nega .NET'siz:** .NET runtime'ga bog'liqlikni butunlay yo'qotamiz (yagona toolchain, kichikroq
va sodda paket). C# plugin kodi endi **ishga tushirilmaydi** — u faqat **spetsifikatsiya/reference**
sifatida ishlatiladi (matnlar, fixture'lar, golden fayllar, FAKE_DEVICES ssenariylari Rust testlariga
ko'chiriladi).

### Tasdiqlangan platforma faktlari
- WebView2 v109 — Win7'ni qo'llab-quvvatlagan oxirgi runtime; SDK `1.0.1519.0+` Win7'ni tashlaydi.
- .NET: net48/.NET 6 Win7 SP1'da ishlaydi, **.NET 7+ ishlamaydi** (shuning uchun ham .NET yo'li
  qulay emas edi). Bu endi ahamiyatsiz — .NET umuman ishlatilmaydi.
- Sentinel va uning engine DLL'lari **x64 native** (Ultralight/WebCore/AppCore/`sentinel.exe`).

## 1. Hal qiluvchi xavf — ENGINE MOSLIGI (hamma narsadan oldin)

Butun reja bitta tekshirilmagan faraz ustida turadi: **Ultralight pv-next Vue SPA'sini
render qila va ishlata oladimi?** ~10 ta pluginni Rust'da qayta yozib, keyin engine ilovani
ko'tara olmasligini bilish — eng yomon natija. Shuning uchun bu birinchi.

- Isbotlangan: Ultralight oddiy sahifa/dashboardni render qiladi (headless test), JS↔Rust bridge
  winit+Renderer yo'lida ishlaydi. Zamonaviy SPA'ni ko'tarishi **isbotlanmagan**.
- pv-next bu diskda topilmadi. Referens `http://localhost:5173` (Vite) ni ochadi.
- **Birinchi amaliy qadam** (Faza 0): Sentinel'ni pv-next `dist` buildiga va Vite dev-serveriga
  yo'naltirib, dashboard JS-konsoli orqali xatolarni yig'ish.

> **NATIJA (2026-10-26): ✅ Asosiy darvoza O'TDI.** Headless probe
> (`crates/sentinel-engine/tests/pv_next_compat.rs`) pv-next `dist`ni `localhost:8173`dan yukladi:
> Ultralight **v1.4.0 / WebKit 615 (Safari 16.4.1)**, Vue 3 mount bo'ldi (`__vue_app__`), Router
> `#/sign-in`ga o'tdi, **login ekrani to'liq render bo'ldi**, `Proxy/Promise/fetch/WeakRef/BigInt`
> bor. Ya'ni ES modullar + Vue 3 + Router + Pinia + i18n ISHLAYDI. Jonli `pasport-viza.uz` bo'sh
> chiqdi — Ultralight headless tarmog'i yetib bormadi (TLS/tarmoq), engine aybi emas.
> **NATIJA 2 (2026-10-26): engine capability probe** (`tests/engine_capabilities.rs`,
> `headless::probe_capabilities`) — Ultralight'ning bu buildida quyidagilar **YO'Q**:
> `WebAssembly` (undefined), `getUserMedia`/`mediaDevices`/`RTCPeerConnection`/`MediaStream`,
> WebGL va WebGL2, OffscreenCanvas, SharedArrayBuffer. Web Workers bor. Runtime toggle topilmadi
> (bu Ultralight'ning ma'lum cheklovi — JIT/WASM'siz JSC, WebGL backend'siz).
>
> **Dist inventarizatsiyasi:** 3 `.wasm` fayl — `ort-wasm` (ONNX Runtime Web, faceworksv2 yuz/
> liveness), `vips` (wasm-vips, rasm qayta ishlash). `getUserMedia` 2 faylda (kamera). WebRTC yo'q,
> MediaPipe/TF yo'q.
>
> **Xulosa:** core Vue ilova ishlaydi; lekin **kamera + yuz AI + og'ir rasm qayta ishlash**
> Ultralight'da in-page ishlamaydi va **native plagin** orqali berilishi shart (allaqachon
> rejalashtirilgan domen). Yaxshi yangilik: faceworksv2'ning ONNX modellari native **`ort`
> crate** (ONNX Runtime native) bilan Rust'da ishlatiladi — xuddi shu modellar, WASM'siz; vips →
> native libvips / Rust image crate'lari; kamera → native capture plagin. **Narx:** pv-next'ning
> kamera/yuz/foto view'lari getUserMedia/WASM o'rniga plagin bridge'ni chaqirishi uchun
> o'zgartirilishi kerak (localizatsiyalangan, plagin chegarasiga mos).
>
> **Hali tekshirilmagan**: boshqa chuqur/og'ir view'lar; vips aynan qayerda ishlatiladi (faqat foto
> oqimidami yoki kengroqmi) — ta'sir hajmini yakuniy aniqlash uchun.

## 2. Arxitektura (Rust-only)

```
┌──────────────────────────────────────────────────────────┐
│ Sentinel (Rust, x64, Win7+)                                 │
│  winit oyna → Ultralight Renderer                           │
│   ├─ chrome view (tablar, address bar)                      │
│   ├─ content view(lar) ← pv-next Vue ilova                  │
│   │     window.native + share.RunPlugin / hostObjects shim  │
│   │     (ASYNC Promise bridge: callId → __sentinelResolve)  │
│   └─ plugin UI overlay view (HTML modal: imzo/ID/foto oyna) │
│                                                             │
│  Plugin registry (Rust trait: id/name/version/dispatch)     │
│   ├─ in-process Rust pluginlar ─ FFI → x64 native SDK        │
│   │     RS_SDK (barmoq), EDSDK (foto), libesigner (ekalit)  │
│   └─ x86-only SDK uchun: sentinel-ipc (named pipe) ─────────┼─┐
│  sentinel-permissions (config allowlist: portal originlari) │ │
└──────────────────────────────────────────────────────────┘ │
                                                               ▼
                               ┌─────────────────────────────────┐
                               │ kichik RUST x86 helper (.NET emas)│
                               │  ElyMRTD.dll (x86 native) → chip  │
                               └─────────────────────────────────┘
```

- **Engine**: Ultralight, low-level Renderer + winit (AppCore emas; crash hal qilingan).
- **Pluginlar**: Rust plugin trait + registry (`IUiPlugin` o'rniga). Har biri
  `{"data","success","error_message"}` konvertini qaytaradi.
- **Device kirishi**: x64 native vendor SDK'larga to'g'ridan-to'g'ri Rust FFI (`libloading`), yoki
  xom protokol (serial/PCSC/USB).
- **x86-only SDK**: Elyctis chip (ElyMRTD) x86 — kichik **Rust x86 helper** jarayoni orqali
  (named pipe, `sentinel-ipc` transporti). .NET umuman yo'q.
- **Xavfsizlik**: per-origin capability modeli boshqariladigan kiosk uchun **config allowlist**
  sifatida — portal originlariga plugin ruxsatlari oldindan beriladi. Bu referensdan ustun
  (referensda ruxsat modeli umuman yo'q, admin huquqida ishlaydi).

### Vendor SDK bitlik/managed jadvali (PE sarlavhalaridan o'lchangan)

| Plugin | Vendor yadro | Bitlik | Tur | Rust yo'li |
|---|---|---|---|---|
| Fingerprints | `RS_SDK.dll` (+opencv, tflite x64 native) | **x64** | native | ✅ to'g'ridan FFI (litsenziya — pastga qarang) |
| Photo | `EDSDK.dll` | **x64** | native | ✅ to'g'ridan FFI |
| EKalit | `libesigner.dll` (yadro) | **x64** | native | ✅ to'g'ridan FFI (`.stn.dll` managed wrapper — kerak emas) |
| IdBox MRZ | serial port | — | — | ✅ toza Rust (`serialport`) |
| IdBox chip UID | WinSCard (OS) | — | — | ✅ toza Rust (`pcsc`) |
| IdBox chip BAC/DG | `ElyMRTD.dll` (+libeac/libeay) | **x86** | native | ⚠️ x86 Rust helper orqali FFI |
| System / Lang | — | — | — | ✅ toza Rust |
| **SignPad** | `SigPlusNET.dll` | x86 | **.NET managed** | ❌ eng qiyin: native Topaz SDK yoki serial/HID protokolini qayta yozish |
| Office | COM interop | — | managed | qiyin: Rust COM (`windows` crate) yoki kechiktirish |

**Muhim:** deyarli barcha yadrolar x64 native — ya'ni .NET'siz Rust FFI bilan chaqiriladi.
Faqat **SigPlusNET (imzo)** toza managed — eng katta xavf shu yerda.

## 3. Fazali reja

### Faza 0 — Darvoza va xavfsizlik tuzatishlari (pluginlardan oldin)
1. **Engine-moslik testi** ✅ BAJARILDI (yuqoridagi NATIJA; probe harness repoda).
2. **Origin parse teshigini yopish** ✅ BAJARILDI ([app.rs](../crates/sentinel-shell/src/app.rs)):
   endi origin **haqiqiy yuklangan URL**dan aniqlanadi — home imtiyozlari faqat boshlang'ich
   dashboardга (URL yo'q / about:blank), har qanday navigatsiya qilingan real URL o'z origini
   yoki `Origin::opaque()` (file:/data:/parse bo'lmaydiganlar). `Origin::opaque()` qo'shildi.
   (Avtomatik xavfsizlik review'i bayroq-asosli birinchi urinishdagi kamchilikni topdi —
   URL-asosli qilib mustahkamlandi.)
3. **DLL bo'sh-hash chetlab o'tishini yopish** ✅ BAJARILDI (`dll-host/src/host.rs`):
   `DllHost.require_integrity` (default on) — sahifadan kelgan `DllLoad` bo'sh/bo'shliqli hash
   bilan rad etiladi; test/ishonchli ichki foydalanish uchun `without_integrity_gate()`.
   Testlar: permissions 13, dll-host 9, js_bridge e2e gate (haqiqiy hash) 1 — barchasi yashil.

### Faza 1 — Shell'ni "ishchi holat"ga keltirish
- **Oynani resizable qilish** ✅ BAJARILDI: `with_resizable(false)` olib tashlandi; `Resized`da
  chrome/content view'lar o'lchanadi (`PageView::resize` + ul-next `View::resize`). Vizual tasdiq
  jonli ishni talab qiladi.
- **Efemer sessiya (storage/history qarori)** ✅ BAJARILDI: foydalanuvchi qarori — **tarix
  saqlanmaydi**. `Engine` endi non-persistent `Session` (`create_session(false, …)`) ishlatadi →
  cookie/localStorage/cache/tarix **diskka yozilmaydi** (fuqaro ma'lumotli kiosk maxfiyligi).
  `sentinel-storage`/`sentinel-crypto` shell'ga ulanmaydi.
- **Tablar (chrome-HTML tab strip)** ✅ BAJARILDI (kompilyatsiya + clippy toza): `content` →
  `Vec<Tab>` + `active` (har tab o'z content view'i, `is_home`/`loading` holati), input/paint/nav
  faol tabga yo'naltirilgan. Chrome'da tab strip UI (`setTabs`) + `+`/tanlash/yopish tugmalari;
  `newtab`/`selecttab <i>`/`closetab <i>` buyruqlari; BAR_HEIGHT 44→78. Vizual/interaktiv tasdiq
  jonli ishni talab qiladi (`cargo run -p sentinel-shell --features ultralight`).
- **Boshqariladigan allowlist config**: portal originlariga plugin capability'larini oldindan
  beruvchi config fayli.

### Faza 2 — Plugin yadro infratuzilmasi (Rust, pluginlardan oldin)
1. **Async Promise bridge + STREAMING** ✅ BAJARILDI (engine `jsbridge`, headless testlar yashil):
   `window.share.RunPlugin/GetPlugins` Promise qaytaradi; JS `callId` beradi, native
   `__sentinelEnqueue(callId, request)` uni hostga uzatadi, host keyin `resolve_call`
   (`__sentinelResolve`) bilan yakunlaydi. **Streaming** (referens .NET ko'prigida yo'q edi —
   stream uzatib bo'lmasdi): `share.RunPluginStream(id, req, onChunk)` oraliq `emit_chunk`
   (`__sentinelEmit`) kadrlarini oladi (idbox chip-status hodisalari, jonli preview, progress),
   oxirida `resolve`. Isbot: `tests/async_bridge.rs` (callId→resolve) va `tests/async_stream.rs`
   (2 chunk + final) — ikkalasi haqiqiy Ultralight JSC'da o'tdi. Busy/re-entrancy guard keyin
   registry/overlay bosqichida.
2. **Plugin UI host**: modal device oynalari (imzo/ID/foto) WPF edi — endi **Ultralight HTML
   overlay** (uchinchi overlay view yoki ikkinchi winit oyna) + input capture.
3. **Rust plugin trait + registry** ✅ BAJARILDI: `sentinel-plugin` krati — `Plugin` trait
   (`descriptor` + `execute`), `PluginDescriptor`/`PluginKind`, `PluginResult`
   (`{data,success,error_message}`, reference bilan bayt-mos), va `PluginRegistry`
   (`descriptors()` → `share.GetPlugins()`, `dispatch(id, req)` → `share.RunPlugin`). Toza, GUI'siz,
   9 test + clippy toza. (Async bridge/UI host shuning ustiga quriladi.)
3b. **Registry + async bridge shell'ga ulandi (`window.share`)** ✅ BAJARILDI: faol tab yuklanganda
   `install_async_bridge` o'rnatiladi (document origini bilan bog'lab); har chaqiruv o'z origini
   bo'yicha **`Capability::Plugin`** gate'idan o'tadi (yangi capability), so'ng `registry`ga
   dispatch bo'lib `PluginResult`/descriptor JSON bilan resolve qilinadi. Namuna `Asbt.System`
   plugini registratsiya qilingan. **Xavfsizlik**: (a) origin enqueue paytida `PendingCall`ga
   bog'lanadi va gate shu origin bo'yicha; (b) **har tabga alohida navbat** — natija faqat o'zini
   chaqirgan tab'da resolve bo'ladi (cross-tab leak yo'q); (c) navigatsiyada faol tab navbati
   tozalanadi. (Qoldiq hardening: per-document nonce — keyin.) Avtomatik review 2 topilmasi shu
   tarzda yopildi.
4. **`sentinel-ipc` named-pipe transport**: x86 vendor SDK'lar uchun kichik Rust helper (JSON-lines,
   hello-handshake, parent-PID bog'lash — referens idbox helperidek, lekin .NET emas).

### Faza 3 — JS moslik shim (drop-in uchun hal qiluvchi)
pv-next o'zgarmay ishlashi uchun aynan shu yuza:
- `window.chrome.webview.hostObjects.<name>.Method(...)` — **Promise qaytaruvchi**.
- `share.RunPlugin(id, jsonPayload)` va `share.GetPlugins()` — JSON string,
  `{"data","success","error_message"}` konverti.
- Aniq yuzani **pv-next'ning plugin-servislaridan** olish (`isAvailable()`, versiya gate'lari) —
  hujjatlardan emas.

### Faza 4 — Plugin manager / yangilash
- Manifest (id, version, entryFile, sha256, minHostVersion), SemVer, **SHA-256 tekshiruv**,
  update-feed. Rust pluginlar sifatida (native cdylib yoki ichki modullar).

### Faza 5 — Pluginlarni Rust'da implement qilish (oson → qiyin)
C# kodi spetsifikatsiya sifatida ishlatiladi; matnlar → Rust const (homoglyph testlari bilan),
fixture/golden/FAKE_DEVICES → Rust testlari. Byte-aniq JPEG mumkin emas (GDI+ ≠ Rust encoder) —
**piksel-parity tolerantligi** ishlatiladi (foto plugin allaqachon shunday qiladi).

Tartib:
1. **Oson, toza Rust**: System, LangKeyboard, IdBox MRZ (serial), IdBox chip UID (`pcsc`).
2. **O'rta, x64 native FFI**: Photo (EDSDK), EKalit (libesigner), Fingerprints (RS_SDK).
   - Barmoq izi uchun `RS_SDK_USB_Protocol.md` + `E:\Experiments\virtual-usb` emulyatori mavjud.
3. **Qiyin, oxirida**: IdBox chip BAC/DG (x86 Rust helper + ElyMRTD FFI), SignPad (native Topaz
   SDK yoki protokol), Office (COM).

### Faza 6 — Mustahkamlash va paketlash
- Ruxsat allowlist persist (`sentinel-storage`).
- Paketlash: Ultralight SDK DLL'lari + resources, native vendor DLL'lar, kod imzolash, Win7 validatsiya.
- "Optimizatsiya": o'lchovsiz ish qo'shmaymiz — 60fps render bor; avval profiling.

## 4. Litsenziya ogohlantirishi
- Referensdagi RS_SDK "lock-byte" patchi (`SdkUnlocker`/`SdkPatcher`) vendor litsenziyasini
  **chetlab o'tadi** — buni **qabul qilmaymiz**.
- Barmoq izi protokolini qayta implement qilish ham **yuridik tasdiqni** talab qiladi. Vendor
  SDK'larini FFI orqali qonuniy litsenziya bilan chaqirish afzal.

## 5. Men sizdan nimani so'rayman (ochiq savollar)
1. **pv-next frontendi qayerda?** (`dist` yo'li yoki ishlaydigan URL) — engine testi uchun.
2. **Birinchi kerakli pluginlar** qaysi? pv-next haqiqatda qaysilarini chaqiradi? (Office/EKalit/
   SignPad kerakmi yoki keyinroqmi?)
3. **Imzo plansheti (Topaz)**: native Topaz SigPlus SDK bormi, yoki serial/HID protokolini qayta
   yozamizmi? (bu eng qiyin plugin)

## 6. Tavsiya etilgan birinchi qadam
**Faza 0**: (a) pv-next'ni Sentinel'da ochib engine-moslikni o'lchash, (b) ikki xavfsizlik teshigini
yopish. Bular rejaning qolganini tasdiqlaydi yoki yo'nalishini o'zgartiradi.
