# Final status of the AKG N9 Control project

Portuguese version: [pt-BR/STATUS.md](pt-BR/STATUS.md)

Date: 11/18/2026. Summary of what was delivered and what remains pending human review.

## Delivered (Phases 0–3)
- **crate `akg-race`** — RACE protocol over HID. Reads + **reversible write layer with strict id
  whitelist** (only verified reversible commands pass; flash/FOTA/reset/shutdown and NVKey write
  `0x0A01` are rejected in `send_write`). **16 passing tests.**
  - 14 write builders, 9 read parsers (GET), all with payloads **extracted and verified from
    `genRacePackets` in the APK** (not inferred).
- **crate `akg-tray`** — tray app. Reads (every 30 s) battery, firmware, ANC mode/level, and feature
  states. Every on/off control is a **CheckMenuItem** that reflects the actual state and toggles on
  click; ANC modes, level, and auto-off presets are "radio" checkboxes.
  - Controls: ANC (off/ANC/TalkThru/Ambient), level 1–7, voice prompts, speak-to-chat, auto power-off,
    find my headphones (play/stop), 3D Audio, Limiter, Chat Optimizer.
- **FUNCTIONS.md** — table of all mapped commands (id, payload, classification), discovery of the
  `01` enable in ANC modes, the filter map (01=ANC, 09=TalkThru, 0A=Ambient), and GETs.

### Validated on hardware (via dongle, with official app)
- ANC on/off and level: round-trip with ack + readback `0x0901`. TalkThru/Ambient: the owner confirmed
  that they switch the mode; the `01` enable was the missing fix.

## Out of scope for SAFETY (not a failure — it is the goal rule)
- **Band EQ (parametric): NOT implemented.** The app saves coefficients via **NVKey write `0x0A01`**
  (`PeqStageSaveCoef`/`SaveUiData`...). The strict goal rule forbids NVKey writes without human
  approval command-by-command. Therefore the tray's "EQ" requirement and the safety rule are in
  **conflict**, and safety takes precedence (as required by the goal itself). Merely switching an
  already stored preset (`0x0900`) would be reversible, but it is coupled to the save-via-NVKey flow;
  no standalone value.
- **Also blocked (NVKey-write `0x0A01`):** gestures, auto-standby, Multi-AI.
- **Phase 4 (firmware dump, ERNW RACE Toolkit):** optional; not executed.

## Pending owner items (agreed: "check later")
1. **Re-validate on hardware** batches 2–5 (voice, auto-off, find my headphones, speak-to-chat, 3D,
   limiter, chat) and reconfirm TalkThru/Ambient after payload fix — none of this was re-validated on
   the headset after the latest changes, at the owner's request.
2. Values to confirm on the headset: Hi-Res type, mic gain range, Safe Sound mode, PSAP level/index,
   3D Audio type.
3. Optional: validate remaining readings from `FUNCTIONS_full.md` by sending each command over USB.

## Main commits
Phase 3 (ANC) → write layer/whitelist → tray → read fixes → feature batches 1–5 → state GETs →
refactoring to toggles (CheckMenuItem). See `git log`.
