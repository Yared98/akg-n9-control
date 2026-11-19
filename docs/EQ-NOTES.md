# AKG N9 EQ — reverse engineering notes (for implementation)

Portuguese version: [pt-BR/EQ-NOTES.md](pt-BR/EQ-NOTES.md)

The owner approved NVKey write for EQ (11/18/2026). Goal: select presets and create profiles
(edit curve), like the official app.

## App path (Airoha PEQ, chip AB1585)
- `setEQSettings2(hmDevice, EQSettings2, isNotify)` → `AirohaEQControl.setEQSettings2`:
  - `category == 201` → PersoniFi EQ (separate feature).
  - `category == 0` (STUDIO/flat) → `AirohaPeqMgr.setPeqGroupIdx((byte)0)` — **reversible, no NVKey**.
  - others (with payload) → `UpdateAndSavePeqData(...)` → stages `PeqStageSaveUiData` /
    `PeqStageSaveCoef` → **NVKey write** (`genWriteNvKeyPacket`).
- UI data NVKey: `0xEF00 + index` (`setPeqUiDataNvKey`: i + 61184).
- Coefficient NVKey: `AirohaPeqMgr.NVKEY_AUDIO_COEF_BASE + AUDIO_CUSTOM_COEF_OFFSET + i`.
- UI data payload (`getSavePeqUiDataPayload`) is built from `PeqUiDataStru` /
  `PeqBandInfo[10]` (freq, Q = freq/Qvalue, gain, enable) + masterGain. The **coefficients** come from
  the IIR (biquad) conversion of each band — math to reproduce (libpeq).

## Presets (records saved on the headset)
`EnumEqPresetIdx`: JAZZ, VOCAL, BASS, …, CLUB, STUDIO. These are EQ records; the app reads/applies them via
`getEQSettings2(category)` / `getCustomIndex` / `getEqRecordSize`. Bands: 32,64,125,250,500,1k,2k,4k,8k,16k.

## Implementation plan (phases)
1. **Select preset** — confirm the exact command to "apply record N" (group/index or
   preset setEQSettings2). Expose an EQ presets submenu in the tray. [tractable]
2. **Custom profile (curve)** — major effort:
   a. Reproduce gain(dB)→IIR coefficient conversion per band (biquad) from libpeq.
   b. Build `getSavePeqUiDataPayload` and save packets (UI data + coef) exactly.
   c. Write to NVKeys (`0xEF00+i`, coef base+offset+i) — with whitelist allowing these ids
      ONLY under this EQ flow (do not open general 0x0A01).
   d. UX: since sliders don't fit in a tray menu, define bands via a **config file**
      (e.g., gain per band) that the app reads and applies — or a separate window (higher effort).

## Safety
The current whitelist blocks 0x0A01 (NVKey). For EQ, allow ONLY the EQ NVKeys (0xEF00+i and the coef
keys), ideally via a dedicated `apply_eq_*` function, keeping gestures/standby/factory out.

## UPDATE — Phase 1 is REVERSIBLE (setPeqGroupIdx)
- **Apply preset = `PeqStageSetPeqGrpIdx`** → id `0x0900`, payload `00 00 <index>` (module 0x0000).
  Reversible, does NOT write to NVKey. (id 0x0900 is already in the whitelist — same as voice prompt, module distinguishes.)
- Preset list = "audio path" NVKey (`NVKEY_AUDIO_PATH` = 0xF233), read via `PeqStageReadAudiPath`
  + `PeqStageReadPeqSubset`. Format: `[count U16][records...]`; each record has the coef NVID at +4/+5;
  isCustom if NVID falls into the custom range (`NVKEY_AUDIO_COEF_BASE + AUDIO_CUSTOM_COEF_OFFSET + 0..3`),
  catID = offset+101; built-in catID = position+1. (Audio path parsing = refinement for dynamic scan with names.)
- **Phase 1 MVP (reversible):** "EQ Preset" submenu applying `setPeqGroupIdx(index)` by index.
- **Phase 2 (NVKey, approved):** create/edit curve = IIR coefficients + save via NVKey (0xEF00+i, coef base+offset).

## DEFINITIVE CONCLUSION (extraction review) — Music EQ is blocked by native lib
Traced top-down (DashboardActivity → CmdCtrl → AirohaPEQControl1568 → AirohaPeqMgr):
- A music preset = set of **per-band gains** (10 bands). Applying = `setEQSettings2(category, gains)`.
  `category==0` = EQ off (flat). There is NO "select preset" command — every preset sends gains.
- The headset needs the **IIR coefficients**, which the app calculates via a **NATIVE library** `NativePeq`:
  `setPeqPoint(freq,Q,gain)` → `generateCofe()` → `changeRescaleCofe(masterGain)` → `getCofeParam()` (short[]).
  These coefficients go into `CoefParamStruct` and are sent (RealTimeUpdate) + saved via NVKey.
- **Blocker:** faithfully reproducing the coefficients requires the native `.so` (Airoha's fixed-point
  format + rescale). It is not permissions (NVKey already approved) or protocol — it is the native calculation,
  impractical to reimplement without the lib. `setPeqGroupIdx` alone only changes the active group (on the N9
  this affects voice/call EQ settings), without loading coefficients.
- **Options (all bad):** (a) reimplement biquad peaking-EQ and hope the format matches (risky,
  might sound wrong); (b) extract/run the .so (infeasible on Windows/Rust); (c) leave music EQ to the official
  app. Recommendation: (c) — it is the only feature that cannot be cleanly reimplemented; the rest of the app works.
