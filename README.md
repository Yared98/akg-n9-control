# akg-n9-control

**English** | [Português](README.pt-BR.md)

Windows tray app (Rust) to **monitor and control the AKG N9 Hybrid headphones from the PC**, talking
the Airoha **RACE** protocol over USB-HID, through the 2.4 GHz dongle (`0ECB:20D9`) or the headset's
USB-C port (`0ECB:20D8`). No vendor software needed.

*Unofficial: not affiliated with or endorsed by AKG, Harman, Samsung or Airoha.*

## Crates
- `crates/akg-race`: RACE protocol library over HID. Reads, parsers, and a **write layer with an id
  whitelist** (only verified, reversible commands). Tests run without hardware (`cargo test`).
- `crates/akg-tray`: tray app. A panel with the headset state and the controls as toggles.

## What the app does
- **Reads** (every 30 s): battery, firmware, ANC mode/level and the state of the features. The icon
  changes color with the battery level, and a toast shows below 15%.
- **Controls** (toggles that reflect the real state): ANC off / ANC / TalkThru / Ambient and level,
  Spatial sound (Movie/Music/Game), voice prompts, Speak-to-Chat, VoiceAware (and level), pause on
  removal, auto power-off, limiter, Chat Optimizer, Hi-Res, Safe Sound, low-volume EQ, L/R balance,
  find my headphones (plays a sound; **stops when you press play on the headset**) and power off.
- **Diagnostics**: serial, chip and firmware (read-only).

## Build, test and run
    cargo test                 # protocol + parsers (no headset needed)
    cargo build --release      # binary at target/release/akg-tray.exe
    cargo run -p akg-tray      # opens the tray icon

Needs the headset connected (dongle or USB-C). Right-click the tray icon to open the menu.

## Safety
The write commands were extracted from the official app's code and checked against it (not guessed),
and tested on an AKG N9 Hybrid (firmware 4.5.0) through the dongle. `send_write` refuses any id outside
`ALLOWED_WRITE_IDS`. These stay **out of the app**: flash erase/write, FOTA, factory reset and NVKey
writes (`0x0A01`), which is how the headset stores gestures, auto-standby, Multi-AI and the EQ
coefficients. Music EQ is not supported either (the coefficients come from a native library of the
official app); see [docs/EQ-NOTES.md](docs/EQ-NOTES.md). **Use at your own risk**: writing settings to a
device can change how it behaves.

## Languages
The tray app is available in English and Portuguese (Brazil), automatically matching your Windows display language. You can change the language anytime from the menu. To add a translation or customize one, see [docs/TRANSLATING.md](docs/TRANSLATING.md).

## Documentation
- [docs/TRANSLATING.md](docs/TRANSLATING.md): how to add or customize translations.
- [docs/FUNCTIONS.md](docs/FUNCTIONS.md): mapped and verified commands (id, payload, classification).
- [docs/FUNCTIONS_full.md](docs/FUNCTIONS_full.md): index of the 94 methods of the app (**unverified**).
- [docs/EQ-NOTES.md](docs/EQ-NOTES.md): why music EQ is not supported.
- [docs/STATUS.md](docs/STATUS.md): project status.

License: MIT.
