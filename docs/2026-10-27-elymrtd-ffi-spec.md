# ElyMRTD / ElyRdrControl native FFI spec (for idbox ReadDocument)

**Status:** signatures confirmed from PE metadata — **no vendor header needed.**
`ElyMRTDdotNet.dll` turned out to be a *pure-IL P/Invoke* wrapper
(`Machine=I386`, `CLR=ILOnly, Requires32Bit`), so its `DllImport` metadata gives
the native `ElyMRTD.dll` export names, parameter types, calling convention and
CharSet directly. Cross-checked against the native export table (`ElyMRTD.dll`:
`Machine=I386`, 89 exports; `ElyRdrControl.dll`: `Machine=I386`, 22 exports).

Derived with the reference tool `E:\Experiments\browser\tools\Fingerprints.Probe\pe-exports.cs`
plus a small CharSet/calling-convention probe, over the DLLs in
`E:\Experiments\browser\src\Asbt.IdBox.ChipHelper\lib\`. The DLLs are **x86 (I386)**,
so the helper that FFIs them **must be built i686** (`i686-win7-windows-msvc`, the
`cargo +nightly win7-helper` alias) — it cannot run in the x64 browser process.

## Global ABI

- **Calling convention:** every free function is `CallingConventionWinApi` →
  **`stdcall` on x86.** In Rust: `extern "stdcall"` (equivalently `extern "system"`
  on `i686-*-windows`). Getting this wrong corrupts the stack.
- **CharSet:** the text getters are `CharSetUnicode` → **UTF-16 `wchar_t*`**
  buffers. `connect`, `establishBAC`, `calculateChecksum`, `passiveAuthentication`
  take default-CharSet strings → **ANSI `char*`**.
- **`bool` returns** are .NET `Boolean` over `stdcall` → **Win32 `BOOL` (4-byte
  i32)**; treat `!= 0` as true in Rust (`i32`, not `bool`).
- **`ref UInt32 length`** is an in/out `*mut u32` (buffer capacity in, actual
  count out — see the two-call pattern).
- The two `biometricTemplate::getHeader/getData` exports are C++ **ThisCall**
  members with mangled names (`?getHeader@biometricTemplate@@QAEXPAEPAK@Z`); not
  needed for the DG1/DG2/photo read — skip them.

## ElyMRTD.dll — native signatures (all `stdcall`)

Managed name → native entry → signature (as the P/Invoke marshals it):

Session / transport:
- `init()` → `void`
- `connect(char* reader_name /*ANSI*/)` → `i32`  — APPLET_TYPES: `0` ICAO, `1` IDL, `-1` fail
- `getSelectAntenna(u8* data, u32* length)` → `void`
- `establishBAC(char* mrz_infos /*ANSI*/)` → `BOOL(i32)`
- `disconnect()` → `void`

Data-group reads (status codes, `i32`):
- `readDG1()`, `readDG2()`(entry `readDG2`), `readDG3/4/6/11/12/13/14/15()` → `i32`
- `readEF_SOd()` → `BOOL(i32)`

Authentication:
- `chipAuthentication()` → `BOOL(i32)`
- `activeAuthentication()` → `BOOL(i32)` (managed returns it; verify width)
- `terminalAuthentication()` → `BOOL(i32)`
- `passiveAuthenticationNoChain()` → `i32`  (managed `_passiveAuthentication()` overload)
- `passiveAuthentication(char* cscaFileName, char* dscaFileName)` → `i32`
- `calculateChecksum(char* data)` → `BOOL(i32)`

Text field getters — **Unicode**, `getX(wchar_t* buf, u32* length) -> void`:
`getName getSurname getFamilyName getGivenNames getDocumentType getIssuingState
getDocNum getNationality getBirthDate getFullBirthDate getSex getValidityDate
getOptionalData getOptionalDetails getPersonalNumber getBirthPlace getResidence
getPhoneNumber getProfession getDeliveryDate getIssuingAuthority getIssuingDate
getExpiryDate getIssuingCountry getLicenceNumber`
- `getCountryNameL(u8* code, wchar_t* data, u32* length)` → `void` (managed
  `_getCountryName`)

Binary getters — `getX(u8* buf, u32* length) -> void`:
- `getPhoto`, `getCategories`, `getDG1`, `getDG2`, `getDG3`, `getDG11`, `getDG12`, `getDG13`
- biometrics templates: `getFingersBiometricsTemplates`, `getIrisBiometricsTemplates`,
  `getFacialTemplates` (same buffer+length shape)
- writers used by the reference test seams: `setDG1(u8*,u32)`, `setDG2(u8*,u32)`

### Two-call buffer pattern (inferred — verify on device)

The managed wrapper's public getters pre-size a `StringBuilder`/`byte[]`, pass the
capacity in `*length`, call once, then read `*length` back as the actual count.
So the Rust side: allocate a generous buffer (e.g. DG2 photo can be tens of KB),
set `*length = capacity`, call, then use `*length` bytes/`u16`s. Whether a
zero/too-small first call returns the required size is **not proven from metadata**
— confirm against a real card. Text buffers are UTF-16 code units; decode with the
returned count, not a NUL scan, until verified.

## ElyRdrControl.dll — reader comm (all `stdcall`, native I386)

Used around the chip read (the reference calls `RDR_CloseComm` right after the
antenna scan). Exports include: `RDR_OpenComm`, `RDR_OpenCommEx`, `RDR_CloseComm`,
`RDR_SendCommand`, `RDR_ListReaders`, `RDR_CL_ListReaders`, `RDR_CNT_*`,
`RDR_GetFirmwareVersion`, `RDR_GetSerialNumber`, `RDR_Set/GetStaticRFParams`,
`RDR_Set/GetDynamicRFParams`, `RDR_Start/StopPolling`, `RDR_SetRF`,
`RDR_Read/WriteCTLRegister`, `RDR_SetContactNegotiationSpeed`. Exact arg types are
in `ElySCardDotNet.dll`'s P/Invokes (same probe) when needed.

## Read flow (from reference `Asbt.IdBox.ChipHelper/Shared/ChipReadJob.cs` + `IElyMrtd`)

1. `init()` once.
2. new SCard context + antenna select (`getSelectAntenna`); the reference also
   drives `ElyRdrControl` (`RDR_CloseComm` after `Scanner.Inquire()`).
3. `connect(reader)` → applet type (0 ICAO / 1 IDL).
4. `establishBAC(mrzInfo)` — `mrzInfo` is the MRZ line(s) read from the ID BOX over
   **serial** (not the chip). BAC vs BAP branch per the applet.
5. `readDG1()` then `readDG2()` (→ status); on success `getDG1`/`getName`/… and
   `getPhoto` (DG2 face image, JPEG/JP2).
6. parse into the `AnswereJson` shape; `disconnect()`.

The whole chip read takes seconds and pushes status updates — in our design this
runs in the **i686 helper** and streams status over the plugin API (NO modal
window); idbox `ReadDocument` is a plain plugin call. See
[[sentinel-idbox-real-contract]] and [[sentinel-dropin-blockers]].

## Runtime gotcha

The *managed* `ElyMRTDDotNet` ctor opens `%LOCALAPPDATA%\elyMRTD\Log.txt`
exclusively and throws if the folder is missing. We call **native** `ElyMRTD.dll`
directly (no managed ctor), but `init()` may do its own logging/licensing — create
`%LOCALAPPDATA%\elyMRTD\` before `init()` and verify on a real device.

## Confirmed vs. needs device

- **Confirmed (metadata):** export names, parameter shapes, return types, stdcall,
  Unicode/ANSI split, i686 arch.
- **Needs a real ID BOX + card to verify:** the exact buffer two-call protocol
  (required-size query vs pre-sized), `init()` licensing/log behaviour, DG2 image
  format, and the BAC/BAP branch conditions.
