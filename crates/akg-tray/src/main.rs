//! AKG N9 Hybrid — tray app.
//!
//! Shows battery, firmware, and feature states, and controls reversible settings. Every on/off
//! control is a CheckMenuItem that reflects the state read via GET and toggles on click; ANC modes,
//! levels, and auto-off presets are "radio" checkboxes marking the active item. Writes pass through
//! the strict whitelist in the `akg-race` crate; flash/FOTA/reset/NVKey are never exposed.
//!
//! Reading runs on the event loop thread, with short link checks between full polls.
#![windows_subsystem = "windows"]

mod autostart;
mod balance;
mod i18n;
mod media;
mod mediakey;
mod settings;

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use i18n::{available, save_lang, I18n, LangInfo};
use mediakey::PlayStopHook;

use akg_race::{
    apply_anc, apply_anc_level, apply_audio_limiter, apply_auto_play_pause,
    apply_auto_power_off, apply_chat_optimizer, apply_find_me, apply_hi_res, apply_low_vol_eq,
    apply_lr_balance, apply_safe_sound, apply_shutdown, apply_spatial_3d, apply_speak_to_chat,
    apply_voice_prompt, apply_voiceaware, apply_voiceaware_level, hid::HidRace, parse_anc,
    parse_audio_limiter_status, parse_auto_power_off_status, parse_battery,
    parse_chat_optimizer_status, parse_fw_version, parse_hi_res_status, parse_low_vol_eq_status,
    parse_lr_balance_full, parse_nvkey, parse_safe_sound_status, parse_spatial_3d_full,
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
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

const POLL: Duration = Duration::from_secs(30);
const LINK_POLL: Duration = Duration::from_secs(10);
/// While "find my headphones" is ringing: short interval to detect Play, and safety limit that
/// stops the sound and releases the key hook if no one stops it (avoids swallowing Play/Pause forever).
const RING_POLL: Duration = Duration::from_millis(100);
const RING_MAX: Duration = Duration::from_secs(120);

/// State read from the headset in a cycle.
#[derive(Default, Clone)]
struct Status {
    connected: bool,
    battery: Option<u8>,
    fw: Option<String>,
    dongle_fw: Option<String>,
    name: Option<String>,
    anc: Option<(AncMode, u16)>,
    vp: Option<bool>,
    stc: Option<bool>,
    spatial_full: Option<(bool, u8)>,
    limiter: Option<bool>,
    chat: Option<bool>,
    auto_off: Option<(bool, u16)>,
    hi_res: Option<bool>,
    safe_sound: Option<bool>,
    lr_level: Option<i8>,
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

/// Auto power-off presets: (enabled, minutes).
const AUTOOFF_OPTS: [(bool, u16); 6] = [
    (false, 0),
    (true, 5),
    (true, 10),
    (true, 15),
    (true, 30),
    (true, 60),
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
    s.dongle_fw = query(&dev, &req_fw_version(Role::Dongle), parse_fw_version);
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

    s.lr_level = query(&dev, &req_lr_balance_status(), parse_lr_balance_full);

    s.low_eq = query(&dev, &req_low_vol_eq_status(), parse_low_vol_eq_status);
    s.voiceaware = query(&dev, &req_voiceaware_status(), parse_voiceaware_status);
    s.voiceaware_level = query(&dev, &req_voiceaware_level(), parse_voiceaware_level);

    // Name (custom 0xF203 preferred, otherwise default 0xF202).
    let custom_name = query(&dev, &req_nvkey_read(0xF203, 64), nvkey_string);
    s.name = custom_name.or_else(|| query(&dev, &req_nvkey_read(0xF202, 64), nvkey_string));

    // Static (serial, chip) — NVKey 0xF508 / 0x1002.
    s.serial = query(&dev, &req_nvkey_read(0xF508, 32), nvkey_string);
    s.chip = query(&dev, &req_nvkey_read(0x1002, 32), nvkey_string);
    s
}

/// Short poll: opens the device, queries battery, and returns true if linked.
fn read_link() -> bool {
    // A single missed reply must not look like a disconnect (it would toast and pause the music):
    // only report "not linked" after a second attempt also fails.
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(800));
        }
        if let Ok(dev) = HidRace::open() {
            if query(&dev, &req_battery(), parse_battery).is_some() {
                return true;
            }
        }
    }
    false
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
/// Queue for balance writes. Opening the HID device enumerates every HID device on the machine
/// (tens to hundreds of ms), which made the slider stutter when done on the UI thread for each
/// move. A background thread keeps one handle open, always writes only the newest pending value,
/// and drops the handle after a few idle seconds so the headset is not held open.
fn balance_queue() -> &'static std::sync::mpsc::Sender<i8> {
    static TX: std::sync::OnceLock<std::sync::mpsc::Sender<i8>> = std::sync::OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<i8>();
        std::thread::spawn(move || {
            let mut dev: Option<HidRace> = None;
            loop {
                let mut step = match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(s) => s,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        dev = None; // idle: release the device
                        match rx.recv() {
                            Ok(s) => s,
                            Err(_) => return,
                        }
                    }
                    Err(_) => return,
                };
                while let Ok(newer) = rx.try_recv() {
                    step = newer; // coalesce: only the latest value matters
                }
                for _attempt in 0..2 {
                    if dev.is_none() {
                        dev = HidRace::open().ok();
                    }
                    match &dev {
                        Some(d) if apply_lr_balance(d, step).is_ok() => break,
                        _ => dev = None, // stale handle (e.g. replugged): reopen once
                    }
                }
            }
        });
        tx
    })
}

fn apply_lr(step: i8) {
    let _ = balance_queue().send(step);
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

fn anc_label(i18n: &I18n, mode: AncMode, level: u16) -> String {
    match mode {
        AncMode::Off => i18n.t("anc.off"),
        AncMode::Anc => i18n.tf("anc.anc", &[("level", &level.to_string())]),
        AncMode::TalkThru => i18n.t("anc.talkthru"),
        AncMode::Ambient => i18n.tf("anc.ambient", &[("level", &level.to_string())]),
        AncMode::Other(f) => i18n.tf(
            "anc.other",
            &[("mode", &format!("0x{f:02X}")), ("level", &level.to_string())],
        ),
    }
}

fn tooltip(i18n: &I18n, s: &Status) -> String {
    if !s.connected {
        return i18n.t("tooltip.disconnected");
    }
    let prefix = match &s.name {
        Some(n) if n.chars().count() <= 24 => n.as_str(),
        _ => "AKG N9",
    };
    let bat = s.battery.map(|b| format!("{b}%")).unwrap_or_else(|| "?".into());
    let anc = s
        .anc
        .map(|(m, l)| anc_label(i18n, m, l))
        .unwrap_or_else(|| "?".into());
    i18n.tf(
        "tooltip.status",
        &[("name", prefix), ("battery", &bat), ("anc", &anc)],
    )
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

/// Native Windows toast warning of low battery (uses PowerShell app id, no registration).
fn notify_low_battery(i18n: &I18n, pct: u8) {
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(&i18n.t("toast.title"))
        .text1(&i18n.tf("toast.body", &[("percent", &pct.to_string())]))
        .show();
}

/// Native Windows toast notification for connection state transitions.
fn notify_connection(i18n: &I18n, linked: bool, battery: Option<u8>) {
    if linked {
        let pct = battery
            .map(|b| b.to_string())
            .unwrap_or_else(|| i18n.t("status.na"));
        let _ = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(&i18n.t("toast.connected_title"))
            .text1(&i18n.tf("toast.connected_body", &[("percent", &pct)]))
            .show();
    } else {
        let _ = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(&i18n.t("toast.disconnected_title"))
            .text1(&i18n.t("toast.disconnected_body"))
            .show();
    }
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

struct Ui {
    // Status lines (disabled).
    mi_bat: MenuItem,
    mi_fw: MenuItem,
    mi_name: MenuItem,
    mi_dongle_fw: MenuItem,

    // Listening modes — "radio" checkboxes: the active one is checked.
    mi_off: CheckMenuItem,
    mi_anc: CheckMenuItem,
    mi_talk: CheckMenuItem,
    mi_amb: CheckMenuItem,

    // Level 1..=7 — "radio" checkboxes.
    submenu_level: Submenu,
    levels: Vec<CheckMenuItem>,

    // Voice prompts + Speak-to-Chat — state toggles.
    mi_vp: CheckMenuItem,
    mi_stc: CheckMenuItem,

    // Auto-off — "radio" checkboxes with active preset checked.
    submenu_autooff: Submenu,
    autooff: Vec<CheckMenuItem>,

    // Find my headphones: single checkbox.
    mi_find: CheckMenuItem,

    // Features — state toggles.
    submenu_recursos: Submenu,
    mi_lim: CheckMenuItem,
    mi_chat: CheckMenuItem,
    mi_hr: CheckMenuItem,
    mi_ss: CheckMenuItem,
    mi_lr: MenuItem,
    mi_low_eq: CheckMenuItem,
    mi_pause: CheckMenuItem,
    mi_va: CheckMenuItem,

    // VoiceAware level 1..=5 — "radio" checkboxes.
    submenu_va_level: Submenu,
    va_levels: Vec<CheckMenuItem>,

    // Grouping submenus (Voice, Headset); `submenu_recursos` is the Audio group.
    submenu_voice: Submenu,
    submenu_headset: Submenu,

    // Spatial sound — "radio" checkboxes: Off / Movie / Music / Game.
    submenu_spatial: Submenu,
    mi_sp_off: CheckMenuItem,
    mi_sp_movie: CheckMenuItem,
    mi_sp_music: CheckMenuItem,
    mi_sp_game: CheckMenuItem,

    // Diagnostics — static reading (NVKey). Disabled lines.
    submenu_diag: Submenu,
    mi_diag_serial: MenuItem,
    mi_diag_chip: MenuItem,
    mi_diag_fw: MenuItem,

    // App submenu
    submenu_app: Submenu,
    mi_autostart: CheckMenuItem,
    submenu_notif: Submenu,
    mi_notify_conn: CheckMenuItem,
    mi_pause_disconnect: CheckMenuItem,

    // Language selector — radio checkboxes (inside App submenu).
    submenu_lang: Submenu,
    lang_items: Vec<(String, CheckMenuItem)>,

    // Actions
    mi_shutdown: MenuItem,
    mi_quit: MenuItem,
}

impl Ui {
    fn new(i18n: &I18n, langs: &[LangInfo]) -> (Self, Menu) {
        let menu = Menu::new();

        let mi_bat = MenuItem::new(
            i18n.tf("status.battery", &[("value", &i18n.t("status.na"))]),
            false,
            None,
        );
        let mi_fw = MenuItem::new(
            i18n.tf("status.firmware", &[("value", &i18n.t("status.na"))]),
            false,
            None,
        );
        let mi_name = MenuItem::new(
            i18n.tf("status.name", &[("value", &i18n.t("status.na"))]),
            false,
            None,
        );
        let mi_dongle_fw = MenuItem::new("", false, None);

        let mi_off = CheckMenuItem::new(i18n.t("menu.anc_off"), true, false, None);
        let mi_anc = CheckMenuItem::new(i18n.t("menu.anc_on"), true, false, None);
        let mi_talk = CheckMenuItem::new(i18n.t("menu.talkthru"), true, false, None);
        let mi_amb = CheckMenuItem::new(i18n.t("menu.ambient"), true, false, None);

        let levels: Vec<CheckMenuItem> = (1..=7)
            .map(|n| {
                CheckMenuItem::new(
                    i18n.tf("menu.level_n", &[("n", &n.to_string())]),
                    true,
                    false,
                    None,
                )
            })
            .collect();
        let submenu_level = Submenu::new(i18n.t("menu.level"), true);
        submenu_level
            .append_items(
                &levels
                    .iter()
                    .map(|m| m as &dyn tray_icon::menu::IsMenuItem)
                    .collect::<Vec<_>>(),
            )
            .expect("submenu level");

        let mi_vp = CheckMenuItem::new(i18n.t("menu.voice_prompts"), true, false, None);
        let mi_stc = CheckMenuItem::new(i18n.t("menu.speak_to_chat"), true, false, None);

        let autooff: Vec<CheckMenuItem> = AUTOOFF_OPTS
            .iter()
            .map(|(en, min)| {
                let label = if !en {
                    i18n.t("autooff.never")
                } else {
                    i18n.tf("autooff.minutes", &[("n", &min.to_string())])
                };
                CheckMenuItem::new(label, true, false, None)
            })
            .collect();
        let submenu_autooff = Submenu::new(i18n.t("menu.autooff"), true);
        submenu_autooff
            .append_items(
                &autooff
                    .iter()
                    .map(|m| m as &dyn tray_icon::menu::IsMenuItem)
                    .collect::<Vec<_>>(),
            )
            .expect("submenu autooff");

        let mi_find = CheckMenuItem::new(i18n.t("menu.find_headphones"), true, false, None);

        let mi_lim = CheckMenuItem::new(i18n.t("menu.limiter"), true, false, None);
        let mi_chat = CheckMenuItem::new(i18n.t("menu.chat_optimizer"), true, false, None);
        let mi_hr = CheckMenuItem::new(i18n.t("menu.hi_res"), true, false, None);
        let mi_ss = CheckMenuItem::new(i18n.t("menu.safe_sound"), true, false, None);

        let mi_lr = MenuItem::new(i18n.t("menu.lr_balance"), true, None);

        let mi_low_eq = CheckMenuItem::new(i18n.t("menu.low_volume_eq"), true, false, None);
        let mi_pause = CheckMenuItem::new(i18n.t("menu.pause_on_remove"), true, false, None);
        let mi_va = CheckMenuItem::new(i18n.t("menu.voiceaware"), true, false, None);

        // Audio group: sound quality and protection.
        let submenu_recursos = Submenu::new(i18n.t("menu.audio"), true);
        submenu_recursos
            .append_items(&[&mi_hr, &mi_lim, &mi_ss, &mi_low_eq, &mi_lr])
            .expect("submenu audio");

        let va_levels: Vec<CheckMenuItem> = (1..=5)
            .map(|n| {
                CheckMenuItem::new(
                    i18n.tf("menu.level_n", &[("n", &n.to_string())]),
                    true,
                    false,
                    None,
                )
            })
            .collect();
        let submenu_va_level = Submenu::new(i18n.t("menu.voiceaware_level"), true);
        submenu_va_level
            .append_items(
                &va_levels
                    .iter()
                    .map(|m| m as &dyn tray_icon::menu::IsMenuItem)
                    .collect::<Vec<_>>(),
            )
            .expect("submenu voiceaware level");

        let mi_sp_off = CheckMenuItem::new(i18n.t("menu.spatial_off"), true, false, None);
        let mi_sp_movie = CheckMenuItem::new(i18n.t("menu.spatial_movie"), true, false, None);
        let mi_sp_music = CheckMenuItem::new(i18n.t("menu.spatial_music"), true, false, None);
        let mi_sp_game = CheckMenuItem::new(i18n.t("menu.spatial_game"), true, false, None);
        let submenu_spatial = Submenu::new(i18n.t("menu.spatial_sound"), true);
        submenu_spatial
            .append_items(&[&mi_sp_off, &mi_sp_movie, &mi_sp_music, &mi_sp_game])
            .expect("submenu spatial sound");

        let mi_diag_serial = MenuItem::new(
            i18n.tf("diag.serial", &[("value", &i18n.t("status.na"))]),
            false,
            None,
        );
        let mi_diag_chip = MenuItem::new(
            i18n.tf("diag.chip", &[("value", &i18n.t("status.na"))]),
            false,
            None,
        );
        let mi_diag_fw = MenuItem::new(
            i18n.tf("diag.firmware", &[("value", &i18n.t("status.na"))]),
            false,
            None,
        );
        let submenu_diag = Submenu::new(i18n.t("menu.diag"), true);
        submenu_diag
            .append_items(&[&mi_diag_serial, &mi_diag_chip, &mi_diag_fw])
            .expect("submenu diag");

        // Language submenu inside App submenu
        let submenu_lang = Submenu::new(i18n.t("menu.language"), true);
        let mut lang_items = Vec::new();
        for lang in langs {
            let is_active = lang.code.eq_ignore_ascii_case(i18n.code());
            let item = CheckMenuItem::new(&lang.name, true, is_active, None);
            submenu_lang.append(&item).expect("append lang item");
            lang_items.push((lang.code.clone(), item));
        }

        // App submenu items
        let mi_autostart = CheckMenuItem::new(
            i18n.t("menu.autostart"),
            true,
            autostart::is_enabled(),
            None,
        );
        let mi_notify_conn = CheckMenuItem::new(
            i18n.t("menu.notify_connection"),
            true,
            settings::get_bool("notify_connection", true),
            None,
        );
        let submenu_notif = Submenu::new(i18n.t("menu.notifications"), true);
        submenu_notif.append(&mi_notify_conn).expect("append notif");

        let mi_pause_disconnect = CheckMenuItem::new(
            i18n.t("menu.pause_on_disconnect"),
            true,
            settings::get_bool("pause_on_disconnect", false),
            None,
        );

        let submenu_app = Submenu::new(i18n.t("menu.app"), true);
        submenu_app
            .append_items(&[
                &mi_autostart,
                &submenu_notif,
                &mi_pause_disconnect,
                &PredefinedMenuItem::separator(),
                &submenu_lang,
            ])
            .expect("submenu app");

        let mi_shutdown = MenuItem::new(i18n.t("menu.power_off"), true, None);
        let mi_quit = MenuItem::new(i18n.t("menu.quit"), true, None);

        // Voice group: everything about talking and hearing yourself/others.
        let submenu_voice = Submenu::new(i18n.t("menu.voice"), true);
        submenu_voice
            .append_items(&[&mi_stc, &mi_chat, &mi_va, &submenu_va_level])
            .expect("submenu voice");

        // Headset group: behaviour and housekeeping (power off last, away from the top).
        let submenu_headset = Submenu::new(i18n.t("menu.headset"), true);
        submenu_headset
            .append_items(&[
                &mi_vp,
                &mi_pause,
                &submenu_autooff,
                &PredefinedMenuItem::separator(),
                &submenu_diag,
                &PredefinedMenuItem::separator(),
                &mi_shutdown,
            ])
            .expect("submenu headset");

        menu.append_items(&[
            &mi_bat,
            &mi_fw,
            &mi_name,
            &mi_dongle_fw,
            &PredefinedMenuItem::separator(),
            &mi_off,
            &mi_anc,
            &mi_talk,
            &mi_amb,
            &submenu_level,
            &PredefinedMenuItem::separator(),
            &submenu_spatial,
            &submenu_recursos,
            &submenu_voice,
            &submenu_headset,
            &PredefinedMenuItem::separator(),
            &mi_find,
            &PredefinedMenuItem::separator(),
            &submenu_app,
            &PredefinedMenuItem::separator(),
            &mi_quit,
        ])
        .expect("menu");

        (
            Self {
                mi_bat,
                mi_fw,
                mi_name,
                mi_dongle_fw,
                mi_off,
                mi_anc,
                mi_talk,
                mi_amb,
                submenu_level,
                levels,
                mi_vp,
                mi_stc,
                submenu_autooff,
                autooff,
                mi_find,
                submenu_recursos,
                mi_lim,
                mi_chat,
                mi_hr,
                mi_ss,
                mi_lr,
                mi_low_eq,
                mi_pause,
                mi_va,
                submenu_va_level,
                va_levels,
                submenu_voice,
                submenu_headset,
                submenu_spatial,
                mi_sp_off,
                mi_sp_movie,
                mi_sp_music,
                mi_sp_game,
                submenu_diag,
                mi_diag_serial,
                mi_diag_chip,
                mi_diag_fw,
                submenu_app,
                mi_autostart,
                submenu_notif,
                mi_notify_conn,
                mi_pause_disconnect,
                submenu_lang,
                lang_items,
                mi_shutdown,
                mi_quit,
            },
            menu,
        )
    }

    fn relabel(&self, i18n: &I18n) {
        self.mi_off.set_text(i18n.t("menu.anc_off"));
        self.mi_anc.set_text(i18n.t("menu.anc_on"));
        self.mi_talk.set_text(i18n.t("menu.talkthru"));
        self.mi_amb.set_text(i18n.t("menu.ambient"));

        self.submenu_level.set_text(i18n.t("menu.level"));
        for (i, it) in self.levels.iter().enumerate() {
            it.set_text(i18n.tf("menu.level_n", &[("n", &(i + 1).to_string())]));
        }

        self.mi_vp.set_text(i18n.t("menu.voice_prompts"));
        self.mi_stc.set_text(i18n.t("menu.speak_to_chat"));

        self.submenu_autooff.set_text(i18n.t("menu.autooff"));
        for (i, it) in self.autooff.iter().enumerate() {
            let (en, min) = AUTOOFF_OPTS[i];
            let label = if !en {
                i18n.t("autooff.never")
            } else {
                i18n.tf("autooff.minutes", &[("n", &min.to_string())])
            };
            it.set_text(label);
        }

        self.mi_find.set_text(i18n.t("menu.find_headphones"));

        self.submenu_recursos.set_text(i18n.t("menu.audio"));
        self.mi_lim.set_text(i18n.t("menu.limiter"));
        self.mi_chat.set_text(i18n.t("menu.chat_optimizer"));
        self.mi_hr.set_text(i18n.t("menu.hi_res"));
        self.mi_ss.set_text(i18n.t("menu.safe_sound"));

        self.mi_lr.set_text(i18n.t("menu.lr_balance"));

        self.mi_low_eq.set_text(i18n.t("menu.low_volume_eq"));
        self.mi_pause.set_text(i18n.t("menu.pause_on_remove"));
        self.mi_va.set_text(i18n.t("menu.voiceaware"));

        self.submenu_va_level.set_text(i18n.t("menu.voiceaware_level"));
        self.submenu_voice.set_text(i18n.t("menu.voice"));
        self.submenu_headset.set_text(i18n.t("menu.headset"));
        for (i, it) in self.va_levels.iter().enumerate() {
            it.set_text(i18n.tf("menu.level_n", &[("n", &(i + 1).to_string())]));
        }

        self.submenu_spatial.set_text(i18n.t("menu.spatial_sound"));
        self.mi_sp_off.set_text(i18n.t("menu.spatial_off"));
        self.mi_sp_movie.set_text(i18n.t("menu.spatial_movie"));
        self.mi_sp_music.set_text(i18n.t("menu.spatial_music"));
        self.mi_sp_game.set_text(i18n.t("menu.spatial_game"));

        self.submenu_diag.set_text(i18n.t("menu.diag"));

        self.submenu_app.set_text(i18n.t("menu.app"));
        self.mi_autostart.set_text(i18n.t("menu.autostart"));
        self.submenu_notif.set_text(i18n.t("menu.notifications"));
        self.mi_notify_conn.set_text(i18n.t("menu.notify_connection"));
        self.mi_pause_disconnect.set_text(i18n.t("menu.pause_on_disconnect"));

        self.submenu_lang.set_text(i18n.t("menu.language"));

        self.mi_shutdown.set_text(i18n.t("menu.power_off"));
        self.mi_quit.set_text(i18n.t("menu.quit"));
    }

    fn update_lang_checks(&self, active_code: &str) {
        for (code, item) in &self.lang_items {
            item.set_checked(code.eq_ignore_ascii_case(active_code));
        }
    }

    fn render(&self, s: &Status, i18n: &I18n, tray: &TrayIcon) {
        let _ = tray.set_tooltip(Some(tooltip(i18n, s)));
        let _ = tray.set_icon(Some(make_icon(icon_color(s))));

        let bat_val = match s.battery {
            Some(b) => format!("{b}%"),
            None => {
                if s.connected {
                    "?".into()
                } else {
                    i18n.t("status.disconnected")
                }
            }
        };
        self.mi_bat
            .set_text(i18n.tf("status.battery", &[("value", &bat_val)]));

        let na = i18n.t("status.na");
        let fw_val = s.fw.as_deref().unwrap_or(&na);
        self.mi_fw
            .set_text(i18n.tf("status.firmware", &[("value", fw_val)]));

        let name_val = s.name.as_deref().unwrap_or(&na);
        self.mi_name
            .set_text(i18n.tf("status.name", &[("value", name_val)]));

        if let Some(dfw) = &s.dongle_fw {
            self.mi_dongle_fw
                .set_text(i18n.tf("status.dongle_firmware", &[("value", dfw)]));
            self.mi_dongle_fw.set_enabled(false);
        } else {
            self.mi_dongle_fw.set_text("");
            self.mi_dongle_fw.set_enabled(false);
        }

        // Diagnostics (static).
        let serial_val = s.serial.as_deref().unwrap_or(&na);
        self.mi_diag_serial
            .set_text(i18n.tf("diag.serial", &[("value", serial_val)]));

        let chip_val = s.chip.as_deref().unwrap_or(&na);
        self.mi_diag_chip
            .set_text(i18n.tf("diag.chip", &[("value", chip_val)]));

        let diag_fw_val = s.fw.as_deref().unwrap_or(&na);
        self.mi_diag_fw
            .set_text(i18n.tf("diag.firmware", &[("value", diag_fw_val)]));

        // ANC modes: checks the active one.
        let mode = s.anc.map(|(m, _)| m);
        self.mi_off.set_checked(mode == Some(AncMode::Off));
        self.mi_anc.set_checked(mode == Some(AncMode::Anc));
        self.mi_talk.set_checked(mode == Some(AncMode::TalkThru));
        self.mi_amb.set_checked(mode == Some(AncMode::Ambient));

        // Level: checks the active one (1..=7).
        let lvl = s.anc.map(|(_, l)| l);
        for (i, it) in self.levels.iter().enumerate() {
            it.set_checked(lvl == Some((i + 1) as u16));
        }

        // Spatial sound: checks the active one (Off when !on; otherwise Movie/Music/Game according to type 0/1/2).
        if let Some((on, kind)) = s.spatial_full {
            self.mi_sp_off.set_checked(!on);
            self.mi_sp_movie.set_checked(on && kind == 0);
            self.mi_sp_music.set_checked(on && kind == 1);
            self.mi_sp_game.set_checked(on && kind == 2);
        }

        // State toggles: only updates when GET replied (otherwise keeps the last state).
        if let Some(v) = s.vp {
            self.mi_vp.set_checked(v);
        }
        if let Some(v) = s.stc {
            self.mi_stc.set_checked(v);
        }
        if let Some(v) = s.limiter {
            self.mi_lim.set_checked(v);
        }
        if let Some(v) = s.chat {
            self.mi_chat.set_checked(v);
        }
        if let Some(v) = s.hi_res {
            self.mi_hr.set_checked(v);
        }
        if let Some(v) = s.safe_sound {
            self.mi_ss.set_checked(v);
        }

        if let Some(v) = s.low_eq {
            self.mi_low_eq.set_checked(v);
        }
        if let Some(v) = s.voiceaware {
            self.mi_va.set_checked(v);
        }

        // VoiceAware level: checks the active one (1..=5).
        let va_lvl = s.voiceaware_level;
        for (i, it) in self.va_levels.iter().enumerate() {
            it.set_checked(va_lvl == Some((i + 1) as u16));
        }

        // Auto-off: checks the active preset.
        for (i, it) in self.autooff.iter().enumerate() {
            let (en, min) = AUTOOFF_OPTS[i];
            let checked = match s.auto_off {
                Some((false, _)) => !en,           // Never
                Some((true, t)) => en && t == min, // minutes preset
                None => false,
            };
            it.set_checked(checked);
        }

        // App settings checks
        self.mi_autostart.set_checked(autostart::is_enabled());
        self.mi_notify_conn
            .set_checked(settings::get_bool("notify_connection", true));
        self.mi_pause_disconnect
            .set_checked(settings::get_bool("pause_on_disconnect", false));

        // Language radio check
        self.update_lang_checks(i18n.code());
    }
}

/// The app has no console (windows subsystem), so a panic would vanish silently. Append it to
/// `%APPDATA%\akg-n9-control\crash.log` instead, to make bug reports possible.
fn install_panic_log() {
    std::panic::set_hook(Box::new(|info| {
        if let Some(dir) = std::env::var_os("APPDATA") {
            let dir = std::path::PathBuf::from(dir).join("akg-n9-control");
            let _ = std::fs::create_dir_all(&dir);
            let bt = std::backtrace::Backtrace::force_capture();
            let line = format!("{info}\n{bt}\n---\n");
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
                use std::io::Write;
                let _ = f.write_all(line.as_bytes());
            }
        }
    }));
}

fn main() {
    install_panic_log();
    let mut i18n = I18n::startup();
    let langs = available();

    let event_loop = EventLoopBuilder::new().build();
    let (ui, menu) = Ui::new(&i18n, &langs);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(i18n.t("tooltip.starting"))
        .with_icon(make_icon([0x80, 0x80, 0x80]))
        .build()
        .expect("tray");

    let menu_rx = MenuEvent::receiver();
    let _tray_rx = TrayIconEvent::receiver();

    // Battery notification hysteresis: alerts <=15%, rearms >=25%.
    let low_notified = Cell::new(false);
    let last_status = RefCell::new(Status::default());
    let last_linked = Cell::new(None);
    let last_full_poll = Cell::new(Instant::now());

    let refresh = |status_holder: &RefCell<Status>,
                   i18n: &I18n,
                   ui: &Ui,
                   tray: &TrayIcon,
                   low_notified: &Cell<bool>,
                   last_linked: &Cell<Option<bool>>| {
        let s = read_status();
        let linked = s.connected && s.battery.is_some();

        if let Some(b) = s.battery {
            if b <= 15 && !low_notified.get() {
                notify_low_battery(i18n, b);
                low_notified.set(true);
            } else if b >= 25 {
                low_notified.set(false);
            }
        }

        // Link transition notifications and pause on disconnect (never on first read).
        if let Some(was_linked) = last_linked.get() {
            if linked != was_linked {
                if settings::get_bool("notify_connection", true) {
                    notify_connection(i18n, linked, s.battery);
                }
                if !linked && settings::get_bool("pause_on_disconnect", false) {
                    media::pause_current();
                }
            }
        }
        last_linked.set(Some(linked));

        *status_holder.borrow_mut() = s;
        ui.render(&status_holder.borrow(), i18n, tray);
    };

    // "Find my headphones" ringing: (instant when started, Play/Pause key hook if installed).
    let mut ringing: Option<(Instant, Option<PlayStopHook>)> = None;

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
                last_full_poll.set(Instant::now());
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if let Some((since, hook)) = &ringing {
                    // Ringing: does not perform slow reads (the hook needs a free loop). Only checks
                    // if play was pressed or if safety limit was reached.
                    let pressed = hook.as_ref().map_or(false, |h| h.take_pressed());
                    if pressed || since.elapsed() >= RING_MAX {
                        apply_findme(false);
                        ui.mi_find.set_checked(false);
                        ringing = None; // drop the hook
                    }
                } else if last_full_poll.get().elapsed() >= POLL {
                    refresh(
                        &last_status,
                        &i18n,
                        &ui,
                        &tray,
                        &low_notified,
                        &last_linked,
                    );
                    last_full_poll.set(Instant::now());
                } else {
                    // Short poll: battery query only.
                    let linked = read_link();
                    if let Some(was_linked) = last_linked.get() {
                        if linked != was_linked {
                            refresh(
                                &last_status,
                                &i18n,
                                &ui,
                                &tray,
                                &low_notified,
                                &last_linked,
                            );
                            last_full_poll.set(Instant::now());
                        }
                    } else {
                        last_linked.set(Some(linked));
                    }
                }
            }
            _ => {}
        }

        if let Ok(ev) = menu_rx.try_recv() {
            if ev.id == ui.mi_quit.id() {
                *control_flow = ControlFlow::Exit;
            } else if ev.id == ui.mi_shutdown.id() {
                apply_sd();
            } else if ev.id == ui.mi_off.id() {
                apply(AncSet::Off);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if ev.id == ui.mi_anc.id() {
                apply(AncSet::Anc);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if ev.id == ui.mi_talk.id() {
                apply(AncSet::TalkThru);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if ev.id == ui.mi_amb.id() {
                apply(AncSet::Ambient);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if let Some(n) = ui.levels.iter().position(|it| it.id() == &ev.id) {
                apply_level((n + 1) as u16);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if let Some(i) = ui.autooff.iter().position(|it| it.id() == &ev.id) {
                let (en, min) = AUTOOFF_OPTS[i];
                apply_autooff(en, min);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if ev.id == ui.mi_vp.id() {
                apply_vp(ui.mi_vp.is_checked());
            } else if ev.id == ui.mi_stc.id() {
                apply_stc(ui.mi_stc.is_checked());
            } else if ev.id == ui.mi_sp_off.id() {
                apply_spatial(false, 0);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if ev.id == ui.mi_sp_movie.id() {
                apply_spatial(true, 0);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if ev.id == ui.mi_sp_music.id() {
                apply_spatial(true, 1);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if ev.id == ui.mi_sp_game.id() {
                apply_spatial(true, 2);
                refresh(
                    &last_status,
                    &i18n,
                    &ui,
                    &tray,
                    &low_notified,
                    &last_linked,
                );
            } else if ev.id == ui.mi_lim.id() {
                apply_limiter(ui.mi_lim.is_checked());
            } else if ev.id == ui.mi_chat.id() {
                apply_chat(ui.mi_chat.is_checked());
            } else if ev.id == ui.mi_hr.id() {
                apply_hr(ui.mi_hr.is_checked());
            } else if ev.id == ui.mi_ss.id() {
                apply_ss(ui.mi_ss.is_checked());
            } else if ev.id == ui.mi_lr.id() {
                let texts = balance::BalanceTexts {
                    title: i18n.t("balance.title"),
                    left: i18n.t("balance.left"),
                    right: i18n.t("balance.right"),
                    center: i18n.t("balance.center"),
                    value_left: i18n.t("balance.value_left"),
                    value_right: i18n.t("balance.value_right"),
                    center_button: i18n.t("balance.center_button"),
                };
                let initial_step = last_status.borrow().lr_level.unwrap_or(0);
                balance::open(texts, initial_step, apply_lr);
            } else if ev.id == ui.mi_low_eq.id() {
                apply_loweq(ui.mi_low_eq.is_checked());
            } else if ev.id == ui.mi_pause.id() {
                apply_autopause(ui.mi_pause.is_checked());
            } else if ev.id == ui.mi_va.id() {
                apply_va(ui.mi_va.is_checked());
            } else if let Some(n) = ui.va_levels.iter().position(|it| it.id() == &ev.id) {
                for (j, it) in ui.va_levels.iter().enumerate() {
                    it.set_checked(j == n);
                }
                apply_va_level((n + 1) as u16);
            } else if ev.id == ui.mi_find.id() {
                if ui.mi_find.is_checked() {
                    apply_findme(true);
                    ringing = Some((Instant::now(), PlayStopHook::install()));
                } else {
                    apply_findme(false);
                    ringing = None;
                }
            } else if ev.id == ui.mi_autostart.id() {
                autostart::set_enabled(ui.mi_autostart.is_checked());
                ui.mi_autostart.set_checked(autostart::is_enabled());
            } else if ev.id == ui.mi_notify_conn.id() {
                settings::set(
                    "notify_connection",
                    &ui.mi_notify_conn.is_checked().to_string(),
                );
            } else if ev.id == ui.mi_pause_disconnect.id() {
                settings::set(
                    "pause_on_disconnect",
                    &ui.mi_pause_disconnect.is_checked().to_string(),
                );
            } else if let Some((code, _)) = ui.lang_items.iter().find(|(_, it)| it.id() == &ev.id) {
                let code = code.clone();
                i18n = I18n::new(&code);
                save_lang(&code);
                ui.relabel(&i18n);
                ui.render(&last_status.borrow(), &i18n, &tray);
            }
        }

        // Next wakeup: short while ringing (to check play), link check interval otherwise.
        if !matches!(*control_flow, ControlFlow::Exit) {
            let wait = if ringing.is_some() {
                RING_POLL
            } else {
                LINK_POLL
            };
            *control_flow = ControlFlow::WaitUntil(Instant::now() + wait);
        }
    });
}
