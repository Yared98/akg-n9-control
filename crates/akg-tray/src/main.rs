//! AKG N9 Hybrid — tray app.
//!
//! Shows battery, firmware, and feature states, and controls reversible settings. Every on/off
//! control is a CheckMenuItem that reflects the state read via GET and toggles on click; ANC modes,
//! levels, and auto-off presets are "radio" checkboxes marking the active item. Writes pass through
//! the strict whitelist in the `akg-race` crate; flash/FOTA/reset/NVKey are never exposed.
//!
//! Reading runs on the event loop thread (blocks ~0.5 s every 30 s), via `ControlFlow::WaitUntil`.
#![windows_subsystem = "windows"]

mod mediakey;

use std::time::{Duration, Instant};

use mediakey::PlayStopHook;

use akg_race::{
    apply_anc, apply_anc_level, apply_audio_limiter, apply_auto_play_pause,
    apply_auto_power_off, apply_chat_optimizer, apply_find_me, apply_hi_res, apply_low_vol_eq,
    apply_lr_balance, apply_shutdown, apply_safe_sound, apply_spatial_3d, apply_speak_to_chat,
    apply_voice_prompt, apply_voiceaware, apply_voiceaware_level, hid::HidRace, parse_anc,
    parse_audio_limiter_status, parse_auto_power_off_status, parse_battery,
    parse_chat_optimizer_status, parse_fw_version, parse_hi_res_status, parse_low_vol_eq_status,
    parse_lr_balance_status, parse_nvkey, parse_safe_sound_status, parse_spatial_3d_full,
    parse_speak_to_chat_status, parse_voiceaware_level, parse_voiceaware_status, parse_vp_status,
    req_anc_status, req_audio_limiter_status, req_auto_power_off_status, req_battery,
    req_chat_optimizer_status, req_fw_version, req_hi_res_status, req_low_vol_eq_status,
    req_lr_balance_status, req_nvkey_read, req_safe_sound_status, req_spatial_3d_status,
    req_speak_to_chat_status, req_voiceaware_level, req_voiceaware_status, req_vp_status, AncMode,
    AncSet, RaceTransport, Role,
};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tauri_winrt_notification::Toast;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIconBuilder, TrayIconEvent};

const POLL: Duration = Duration::from_secs(30);
/// While "find my headphones" is ringing: short interval to detect Play, and safety limit that
/// stops the sound and releases the key hook if no one stops it (avoids swallowing Play/Pause forever).
const RING_POLL: Duration = Duration::from_millis(100);
const RING_MAX: Duration = Duration::from_secs(120);

/// State read from the headset in a cycle.
#[derive(Default)]
struct Status {
    connected: bool,
    battery: Option<u8>,
    fw: Option<String>,
    anc: Option<(AncMode, u16)>,
    vp: Option<bool>,
    stc: Option<bool>,
    spatial_full: Option<(bool, u8)>,
    limiter: Option<bool>,
    chat: Option<bool>,
    auto_off: Option<(bool, u16)>,
    hi_res: Option<bool>,
    safe_sound: Option<bool>,
    lr: Option<bool>,
    low_eq: Option<bool>,
    voiceaware: Option<bool>,
    voiceaware_level: Option<u16>,
    serial: Option<String>,
    chip: Option<String>,
}

/// Reads an NVKey as a string (discards zeroes/spaces). None if empty.
fn nvkey_string(report: &[u8]) -> Option<String> {
    let b = parse_nvkey(report)?;
    let s = String::from_utf8_lossy(&b);
    let s = s.trim_end_matches('\0').trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Auto power-off presets (label, enabled, minutes).
const AUTOOFF_OPTS: [(&str, bool, u16); 6] = [
    ("Nunca", false, 0),
    ("5 min", true, 5),
    ("10 min", true, 10),
    ("15 min", true, 15),
    ("30 min", true, 30),
    ("60 min", true, 60),
];

/// Sends a query and tries to read the reply for a few cycles: the dongle returns the frame only after
/// 1–3 polls (the 1st read is often stale), so we retry until the parser matches or times out
/// (~0.5 s). Returns None if nothing valid arrives.
fn query<R>(
    dev: &HidRace,
    req: &[u8; akg_race::REPORT_SIZE],
    parse: impl Fn(&[u8]) -> Option<R>,
) -> Option<R> {
    dev.send(req).ok()?;
    for _ in 0..25 {
        std::thread::sleep(Duration::from_millis(20));
        if let Ok(r) = dev.recv() {
            if let Some(v) = parse(&r) {
                return Some(v);
            }
        }
    }
    None
}

fn read_status() -> Status {
    let mut s = Status::default();
    let dev = match HidRace::open() {
        Ok(d) => d,
        Err(_) => return s, // disconnected
    };
    s.connected = true;
    s.fw = query(&dev, &req_fw_version(Role::Headset), parse_fw_version);
    s.battery = query(&dev, &req_battery(), parse_battery);
    s.anc = query(&dev, &req_anc_status(), parse_anc);
    s.vp = query(&dev, &req_vp_status(), parse_vp_status);
    s.stc = query(&dev, &req_speak_to_chat_status(), parse_speak_to_chat_status);
    s.spatial_full = query(&dev, &req_spatial_3d_status(), parse_spatial_3d_full);
    s.limiter = query(&dev, &req_audio_limiter_status(), parse_audio_limiter_status);
    s.chat = query(&dev, &req_chat_optimizer_status(), parse_chat_optimizer_status);
    s.auto_off = query(&dev, &req_auto_power_off_status(), parse_auto_power_off_status);
    s.hi_res = query(&dev, &req_hi_res_status(), parse_hi_res_status);
    s.safe_sound = query(&dev, &req_safe_sound_status(), parse_safe_sound_status);
    s.lr = query(&dev, &req_lr_balance_status(), parse_lr_balance_status);
    s.low_eq = query(&dev, &req_low_vol_eq_status(), parse_low_vol_eq_status);
    s.voiceaware = query(&dev, &req_voiceaware_status(), parse_voiceaware_status);
    s.voiceaware_level = query(&dev, &req_voiceaware_level(), parse_voiceaware_level);
    // Static (serial, chip) — NVKey 0xF508 / 0x1002.
    s.serial = query(&dev, &req_nvkey_read(0xF508, 32), nvkey_string);
    s.chip = query(&dev, &req_nvkey_read(0x1002, 32), nvkey_string);
    s
}

// ---- write helpers (open connection on the fly; connection failure is silent) --------------

fn apply(set: AncSet) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_anc(&dev, set);
    }
}
fn apply_level(level: u16) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_anc_level(&dev, level);
    }
}
fn apply_vp(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_voice_prompt(&dev, on);
    }
}
fn apply_sd() {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_shutdown(&dev);
    }
}
fn apply_autooff(enabled: bool, min: u16) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_auto_power_off(&dev, enabled, min);
    }
}
fn apply_findme(ring: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_find_me(&dev, ring);
    }
}
fn apply_stc(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_speak_to_chat(&dev, on, 0);
    }
}
fn apply_spatial(on: bool, kind: u8) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_spatial_3d(&dev, on, kind);
    }
}
fn apply_limiter(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_audio_limiter(&dev, on);
    }
}
fn apply_chat(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_chat_optimizer(&dev, on);
    }
}
fn apply_hr(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_hi_res(&dev, on as u8);
    }
}
fn apply_ss(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_safe_sound(&dev, on as u8);
    }
}
fn apply_lr(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_lr_balance(&dev, on, 0); // level 0 (centered); fine tuning to be validated
    }
}
fn apply_loweq(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_low_vol_eq(&dev, on);
    }
}
fn apply_autopause(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_auto_play_pause(&dev, on);
    }
}
fn apply_va(on: bool) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_voiceaware(&dev, on);
    }
}
fn apply_va_level(level: u16) {
    if let Ok(dev) = HidRace::open() {
        let _ = apply_voiceaware_level(&dev, level);
    }
}

fn anc_label(mode: AncMode, level: u16) -> String {
    match mode {
        AncMode::Off => "desligado".into(),
        AncMode::Anc => format!("ANC {level}"),
        AncMode::TalkThru => "TalkThru".into(),
        AncMode::Ambient => format!("Ambiente {level}"),
        AncMode::Other(f) => format!("modo 0x{f:02X} ({level})"),
    }
}

fn tooltip(s: &Status) -> String {
    if !s.connected {
        return "AKG N9 · desconectado".into();
    }
    let bat = s.battery.map(|b| format!("{b}%")).unwrap_or_else(|| "?".into());
    let anc = s.anc.map(|(m, l)| anc_label(m, l)).unwrap_or_else(|| "?".into());
    format!("AKG N9 · {bat} · {anc}")
}

/// Icon color based on battery: teal (ok), amber (medium), red (low), gray (no reading).
fn icon_color(s: &Status) -> [u8; 3] {
    match s.battery {
        _ if !s.connected => [0x80, 0x80, 0x80],
        Some(b) if b < 20 => [0xD8, 0x4A, 0x3A],
        Some(b) if b < 50 => [0xE0, 0xA8, 0x2E],
        Some(_) => [0x2E, 0xA8, 0x9B],
        None => [0x80, 0x80, 0x80],
    }
}

/// 32×32 icon drawn in memory: headphone silhouette (headband arc + two earcups),
/// in the given color, transparent background. No external file.
/// Native Windows toast warning of low battery (uses PowerShell app id, no registration).
fn notify_low_battery(pct: u8) {
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title("AKG N9 — bateria baixa")
        .text1(&format!("Bateria em {pct}%. Hora de carregar o fone."))
        .show();
}

fn make_icon(rgb: [u8; 3]) -> tray_icon::Icon {
    const N: i32 = 32;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    // Filled ellipse (earcup), axes (rx, ry) centered at (cx, cy).
    let ear = |fx: f32, fy: f32, cx: f32, cy: f32, rx: f32, ry: f32| {
        let ex = (fx - cx) / rx;
        let ey = (fy - cy) / ry;
        ex * ex + ey * ey <= 1.0
    };
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32, y as f32);
            // Headband: ring centered at (15.5, 15) only on the top half (arc).
            let (dx, dy) = (fx - 15.5, fy - 15.0);
            let r = (dx * dx + dy * dy).sqrt();
            let band = r > 9.3 && r < 12.3 && fy < 15.5;
            // Earcups at the ends of the arc.
            let cup_l = ear(fx, fy, 5.0, 20.0, 3.6, 6.2);
            let cup_r = ear(fx, fy, 26.0, 20.0, 3.6, 6.2);
            // Lighter inner "cushion" gives headphone look (optional, same color here).
            if band || cup_l || cup_r {
                let i = ((y * N + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 0xFF]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, N as u32, N as u32).expect("icon")
}

fn main() {
    let event_loop = EventLoopBuilder::new().build();
    let menu = Menu::new();

    // Status lines (disabled).
    let mi_bat = MenuItem::new("Bateria: —", false, None);
    let mi_fw = MenuItem::new("Firmware: —", false, None);

    // Listening modes — "radio" checkboxes: the active one is checked.
    let mi_off = CheckMenuItem::new("ANC desligado", true, false, None);
    let mi_anc = CheckMenuItem::new("ANC ligado", true, false, None);
    let mi_talk = CheckMenuItem::new("TalkThru", true, false, None);
    let mi_amb = CheckMenuItem::new("Ambiente", true, false, None);

    // Level 1..=7 — "radio" checkboxes.
    let levels: Vec<CheckMenuItem> = (1..=7)
        .map(|n| CheckMenuItem::new(format!("Nível {n}"), true, false, None))
        .collect();
    let submenu_level = Submenu::new("Nível", true);
    submenu_level
        .append_items(
            &levels
                .iter()
                .map(|m| m as &dyn tray_icon::menu::IsMenuItem)
                .collect::<Vec<_>>(),
        )
        .expect("submenu nivel");

    // Voice prompts + Speak-to-Chat — state toggles.
    let mi_vp = CheckMenuItem::new("Avisos de voz", true, false, None);
    let mi_stc = CheckMenuItem::new("Speak-to-Chat", true, false, None);

    // Auto-off — "radio" checkboxes with active preset checked.
    let autooff: Vec<CheckMenuItem> = AUTOOFF_OPTS
        .iter()
        .map(|(l, _, _)| CheckMenuItem::new(*l, true, false, None))
        .collect();
    let submenu_autooff = Submenu::new("Auto-desligar", true);
    submenu_autooff
        .append_items(
            &autooff
                .iter()
                .map(|m| m as &dyn tray_icon::menu::IsMenuItem)
                .collect::<Vec<_>>(),
        )
        .expect("submenu autooff");

    // Find my headphones — one-shot actions (not a state toggle).
    // Find my headphones: single checkbox. Checked = ringing; unchecking (or pressing play on the headset) stops.
    let mi_find = CheckMenuItem::new("Achar fone (tocar som)", true, false, None);
    let r_find = mi_find.clone();

    // Features — state toggles.
    let mi_lim = CheckMenuItem::new("Limitador", true, false, None);
    let mi_chat = CheckMenuItem::new("Chat Optimizer", true, false, None);
    let mi_hr = CheckMenuItem::new("Hi-Res", true, false, None);
    let mi_ss = CheckMenuItem::new("Safe Sound", true, false, None);
    let mi_lr = CheckMenuItem::new("Balanço L/R (centralizar)", true, false, None);
    let mi_low_eq = CheckMenuItem::new("EQ volume baixo", true, false, None);
    let mi_pause = CheckMenuItem::new("Pausar ao tirar o fone", true, false, None);
    let mi_va = CheckMenuItem::new("VoiceAware", true, false, None);
    let submenu_recursos = Submenu::new("Recursos", true);
    submenu_recursos
        .append_items(&[
            &mi_lim,
            &mi_chat,
            &mi_hr,
            &mi_ss,
            &mi_lr,
            &mi_low_eq,
            &mi_pause,
            &mi_va,
        ])
        .expect("submenu recursos");

    // VoiceAware level 1..=5 — "radio" checkboxes.
    let va_levels: Vec<CheckMenuItem> = (1..=5)
        .map(|n| CheckMenuItem::new(format!("Nível {n}"), true, false, None))
        .collect();
    let submenu_va_level = Submenu::new("VoiceAware nível", true);
    submenu_va_level
        .append_items(
            &va_levels
                .iter()
                .map(|m| m as &dyn tray_icon::menu::IsMenuItem)
                .collect::<Vec<_>>(),
        )
        .expect("submenu voiceaware nivel");


    // Spatial sound — "radio" checkboxes: Off / Movie / Music / Game.
    let mi_sp_off = CheckMenuItem::new("Desligado", true, false, None);
    let mi_sp_movie = CheckMenuItem::new("Filme", true, false, None);
    let mi_sp_music = CheckMenuItem::new("Música", true, false, None);
    let mi_sp_game = CheckMenuItem::new("Jogo", true, false, None);
    let submenu_spatial = Submenu::new("Som espacial", true);
    submenu_spatial
        .append_items(&[&mi_sp_off, &mi_sp_movie, &mi_sp_music, &mi_sp_game])
        .expect("submenu som espacial");

    // Diagnostics — static reading (NVKey). Disabled lines.
    let mi_diag_serial = MenuItem::new("Série: —", false, None);
    let mi_diag_chip = MenuItem::new("Chip: —", false, None);
    let mi_diag_fw = MenuItem::new("Firmware: —", false, None);
    let submenu_diag = Submenu::new("Diagnóstico", true);
    submenu_diag
        .append_items(&[&mi_diag_serial, &mi_diag_chip, &mi_diag_fw])
        .expect("submenu diag");

    let mi_shutdown = MenuItem::new("Desligar o fone", true, None);
    let mi_quit = MenuItem::new("Sair", true, None);

    menu.append_items(&[
        &mi_bat,
        &mi_fw,
        &PredefinedMenuItem::separator(),
        &mi_off,
        &mi_anc,
        &mi_talk,
        &mi_amb,
        &submenu_level,
        &PredefinedMenuItem::separator(),
        &mi_vp,
        &mi_stc,
        &submenu_autooff,
        &submenu_recursos,
        &submenu_va_level,
        &submenu_spatial,
        &submenu_diag,
        &PredefinedMenuItem::separator(),
        &mi_find,
        &PredefinedMenuItem::separator(),
        &mi_shutdown,
        &PredefinedMenuItem::separator(),
        &mi_quit,
    ])
    .expect("menu");

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("AKG N9 · iniciando…")
        .with_icon(make_icon([0x80, 0x80, 0x80]))
        .build()
        .expect("tray");

    let menu_rx = MenuEvent::receiver();
    let _tray_rx = TrayIconEvent::receiver();

    // Clones for refresh (set_checked); originals remain for click handling.
    let (r_off, r_anc, r_talk, r_amb) = (mi_off.clone(), mi_anc.clone(), mi_talk.clone(), mi_amb.clone());
    let r_levels: Vec<CheckMenuItem> = levels.iter().map(|m| m.clone()).collect();
    let (r_vp, r_stc) = (mi_vp.clone(), mi_stc.clone());
    let r_autooff: Vec<CheckMenuItem> = autooff.iter().map(|m| m.clone()).collect();
    let (r_sp_off, r_sp_movie, r_sp_music, r_sp_game) = (
        mi_sp_off.clone(),
        mi_sp_movie.clone(),
        mi_sp_music.clone(),
        mi_sp_game.clone(),
    );
    let (r_lim, r_chat, r_hr) = (mi_lim.clone(), mi_chat.clone(), mi_hr.clone());
    let (r_ss, r_lr, r_low_eq) = (mi_ss.clone(), mi_lr.clone(), mi_low_eq.clone());
    let r_va = mi_va.clone();
    let r_va_levels: Vec<CheckMenuItem> = va_levels.iter().map(|m| m.clone()).collect();

    // Battery notification hysteresis: alerts ≤15%, rearms ≥25% (avoids repeating every cycle).
    let low_notified = std::cell::Cell::new(false);

    let refresh = move |tray: &tray_icon::TrayIcon| {
        let s = read_status();
        let _ = tray.set_tooltip(Some(tooltip(&s)));
        let _ = tray.set_icon(Some(make_icon(icon_color(&s))));
        if let Some(b) = s.battery {
            if b <= 15 && !low_notified.get() {
                notify_low_battery(b);
                low_notified.set(true);
            } else if b >= 25 {
                low_notified.set(false);
            }
        }
        mi_bat.set_text(format!(
            "Bateria: {}",
            s.battery
                .map(|b| format!("{b}%"))
                .unwrap_or_else(|| if s.connected { "?".into() } else { "desconectado".into() })
        ));
        mi_fw.set_text(format!("Firmware: {}", s.fw.clone().unwrap_or_else(|| "—".into())));
        // Diagnostics (static).
        if let Some(v) = s.serial.clone() {
            mi_diag_serial.set_text(format!("Série: {v}"));
        }
        if let Some(v) = s.chip.clone() {
            mi_diag_chip.set_text(format!("Chip: {v}"));
        }
        mi_diag_fw.set_text(format!("Firmware: {}", s.fw.clone().unwrap_or_else(|| "—".into())));

        // ANC modes: checks the active one.
        let mode = s.anc.map(|(m, _)| m);
        r_off.set_checked(mode == Some(AncMode::Off));
        r_anc.set_checked(mode == Some(AncMode::Anc));
        r_talk.set_checked(mode == Some(AncMode::TalkThru));
        r_amb.set_checked(mode == Some(AncMode::Ambient));

        // Level: checks the active one (1..=7).
        let lvl = s.anc.map(|(_, l)| l);
        for (i, it) in r_levels.iter().enumerate() {
            it.set_checked(lvl == Some((i + 1) as u16));
        }

        // Spatial sound: checks the active one (Off when !on; otherwise Movie/Music/Game according to type 0/1/2).
        if let Some((on, kind)) = s.spatial_full {
            r_sp_off.set_checked(!on);
            r_sp_movie.set_checked(on && kind == 0);
            r_sp_music.set_checked(on && kind == 1);
            r_sp_game.set_checked(on && kind == 2);
        }

        // State toggles: only updates when GET replied (otherwise keeps the last state).
        if let Some(v) = s.vp {
            r_vp.set_checked(v);
        }
        if let Some(v) = s.stc {
            r_stc.set_checked(v);
        }
        if let Some(v) = s.limiter {
            r_lim.set_checked(v);
        }
        if let Some(v) = s.chat {
            r_chat.set_checked(v);
        }
        if let Some(v) = s.hi_res {
            r_hr.set_checked(v);
        }
        if let Some(v) = s.safe_sound {
            r_ss.set_checked(v);
        }
        if let Some(v) = s.lr {
            r_lr.set_checked(v);
        }
        if let Some(v) = s.low_eq {
            r_low_eq.set_checked(v);
        }
        if let Some(v) = s.voiceaware {
            r_va.set_checked(v);
        }

        // VoiceAware level: checks the active one (1..=5).
        let va_lvl = s.voiceaware_level;
        for (i, it) in r_va_levels.iter().enumerate() {
            it.set_checked(va_lvl == Some((i + 1) as u16));
        }

        // Auto-off: checks the active preset.
        for (i, it) in r_autooff.iter().enumerate() {
            let (_, en, min) = AUTOOFF_OPTS[i];
            let checked = match s.auto_off {
                Some((false, _)) => !en,              // "Nunca"
                Some((true, t)) => en && t == min,    // minutes preset
                None => false,
            };
            it.set_checked(checked);
        }
    };

    // "Find my headphones" ringing: (instant when started, Play/Pause key hook if installed).
    let mut ringing: Option<(Instant, Option<PlayStopHook>)> = None;

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::NewEvents(StartCause::Init) => refresh(&tray),
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if let Some((since, hook)) = &ringing {
                    // Ringing: does not perform slow reads (the hook needs a free loop). Only checks
                    // if play was pressed or if safety limit was reached.
                    let pressed = hook.as_ref().map_or(false, |h| h.take_pressed());
                    if pressed || since.elapsed() >= RING_MAX {
                        apply_findme(false);
                        r_find.set_checked(false);
                        ringing = None; // drop the hook
                    }
                } else {
                    refresh(&tray);
                }
            }
            _ => {}
        }

        if let Ok(ev) = menu_rx.try_recv() {
            if ev.id == mi_quit.id() {
                *control_flow = ControlFlow::Exit;
            } else if ev.id == mi_shutdown.id() {
                apply_sd();
            } else if ev.id == mi_off.id() {
                apply(AncSet::Off);
                refresh(&tray);
            } else if ev.id == mi_anc.id() {
                apply(AncSet::Anc);
                refresh(&tray);
            } else if ev.id == mi_talk.id() {
                apply(AncSet::TalkThru);
                refresh(&tray);
            } else if ev.id == mi_amb.id() {
                apply(AncSet::Ambient);
                refresh(&tray);
            } else if let Some(n) = levels.iter().position(|it| it.id() == &ev.id) {
                apply_level((n + 1) as u16);
                refresh(&tray);
            } else if let Some(i) = autooff.iter().position(|it| it.id() == &ev.id) {
                let (_, en, min) = AUTOOFF_OPTS[i];
                apply_autooff(en, min);
                refresh(&tray);
            } else if ev.id == mi_vp.id() {
                apply_vp(mi_vp.is_checked());
            } else if ev.id == mi_stc.id() {
                apply_stc(mi_stc.is_checked());
            } else if ev.id == mi_sp_off.id() {
                apply_spatial(false, 0);
                refresh(&tray);
            } else if ev.id == mi_sp_movie.id() {
                apply_spatial(true, 0);
                refresh(&tray);
            } else if ev.id == mi_sp_music.id() {
                apply_spatial(true, 1);
                refresh(&tray);
            } else if ev.id == mi_sp_game.id() {
                apply_spatial(true, 2);
                refresh(&tray);
            } else if ev.id == mi_lim.id() {
                apply_limiter(mi_lim.is_checked());
            } else if ev.id == mi_chat.id() {
                apply_chat(mi_chat.is_checked());
            } else if ev.id == mi_hr.id() {
                apply_hr(mi_hr.is_checked());
            } else if ev.id == mi_ss.id() {
                apply_ss(mi_ss.is_checked());
            } else if ev.id == mi_lr.id() {
                apply_lr(mi_lr.is_checked());
            } else if ev.id == mi_low_eq.id() {
                apply_loweq(mi_low_eq.is_checked());
            } else if ev.id == mi_pause.id() {
                apply_autopause(mi_pause.is_checked());
            } else if ev.id == mi_va.id() {
                apply_va(mi_va.is_checked());
            } else if let Some(n) = va_levels.iter().position(|it| it.id() == &ev.id) {
                for (j, it) in va_levels.iter().enumerate() {
                    it.set_checked(j == n);
                }
                apply_va_level((n + 1) as u16);
            } else if ev.id == mi_find.id() {
                if mi_find.is_checked() {
                    // Started ringing: installs the Play/Pause hook (None = without button shortcut).
                    apply_findme(true);
                    ringing = Some((Instant::now(), PlayStopHook::install()));
                } else {
                    apply_findme(false);
                    ringing = None; // drop the hook
                }
            }
        }

        // Next wakeup: short while ringing (to check play), normal otherwise.
        if !matches!(*control_flow, ControlFlow::Exit) {
            let wait = if ringing.is_some() { RING_POLL } else { POLL };
            *control_flow = ControlFlow::WaitUntil(Instant::now() + wait);
        }
    });
}
