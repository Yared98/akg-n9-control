//! AKG N9 Hybrid — app de bandeja.
//!
//! Mostra bateria, firmware e o estado dos recursos, e controla os reversíveis. Todo controle
//! on/off é um CheckMenuItem que reflete o estado lido via GET e alterna ao clicar; modos de ANC,
//! níveis e presets de auto-desligar são checkboxes "rádio" que marcam o item ativo. As escritas
//! passam pela whitelist dura da crate `akg-race`; flash/FOTA/reset/NVKey nunca são expostos.
//!
//! A leitura roda na thread do event loop (bloqueia ~0,5 s a cada 30 s), via `ControlFlow::WaitUntil`.
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
/// Enquanto o "achar fone" toca: intervalo curto para perceber o Play, e limite de segurança que
/// para o som e solta o hook de tecla caso ninguém pare (evita engolir o Play/Pause para sempre).
const RING_POLL: Duration = Duration::from_millis(100);
const RING_MAX: Duration = Duration::from_secs(120);

/// Estado lido do fone num ciclo.
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

/// Lê uma NVKey como string (descarta zeros/espaços). None se vazia.
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

/// Presets do auto-desligamento (rótulo, enabled, minutos).
const AUTOOFF_OPTS: [(&str, bool, u16); 6] = [
    ("Nunca", false, 0),
    ("5 min", true, 5),
    ("10 min", true, 10),
    ("15 min", true, 15),
    ("30 min", true, 30),
    ("60 min", true, 60),
];

/// Envia uma query e tenta ler a resposta por alguns ciclos: o dongle devolve o frame só depois
/// de 1–3 polls (a 1ª leitura costuma vir velha), então insistimos até o parser casar ou dar
/// timeout (~0,5 s). Retorna None se nada válido chegar.
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
        Err(_) => return s, // desconectado
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
    // Estáticos (série, chip) — NVKey 0xF508 / 0x1002.
    s.serial = query(&dev, &req_nvkey_read(0xF508, 32), nvkey_string);
    s.chip = query(&dev, &req_nvkey_read(0x1002, 32), nvkey_string);
    s
}

// ---- helpers de escrita (abrem a conexão na hora; falha de conexão é silenciosa) --------------

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
        let _ = apply_lr_balance(&dev, on, 0); // nível 0 (centralizado); ajuste fino a validar
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

/// Cor do ícone conforme a bateria: teal (ok), âmbar (média), vermelho (baixa), cinza (sem leitura).
fn icon_color(s: &Status) -> [u8; 3] {
    match s.battery {
        _ if !s.connected => [0x80, 0x80, 0x80],
        Some(b) if b < 20 => [0xD8, 0x4A, 0x3A],
        Some(b) if b < 50 => [0xE0, 0xA8, 0x2E],
        Some(_) => [0x2E, 0xA8, 0x9B],
        None => [0x80, 0x80, 0x80],
    }
}

/// Ícone 32×32 desenhado em memória: silhueta de headphone (arco da haste + duas conchas),
/// na cor dada, fundo transparente. Sem arquivo externo.
/// Toast nativo do Windows avisando bateria baixa (usa o app id do PowerShell, sem registro).
fn notify_low_battery(pct: u8) {
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title("AKG N9 — bateria baixa")
        .text1(&format!("Bateria em {pct}%. Hora de carregar o fone."))
        .show();
}

fn make_icon(rgb: [u8; 3]) -> tray_icon::Icon {
    const N: i32 = 32;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    // Elipse preenchida (concha), eixos (rx, ry) centrada em (cx, cy).
    let ear = |fx: f32, fy: f32, cx: f32, cy: f32, rx: f32, ry: f32| {
        let ex = (fx - cx) / rx;
        let ey = (fy - cy) / ry;
        ex * ex + ey * ey <= 1.0
    };
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32, y as f32);
            // Haste: anel centrado em (15.5, 15) só na metade de cima (arco).
            let (dx, dy) = (fx - 15.5, fy - 15.0);
            let r = (dx * dx + dy * dy).sqrt();
            let band = r > 9.3 && r < 12.3 && fy < 15.5;
            // Conchas nas pontas do arco.
            let cup_l = ear(fx, fy, 5.0, 20.0, 3.6, 6.2);
            let cup_r = ear(fx, fy, 26.0, 20.0, 3.6, 6.2);
            // "Almofada" interna mais clara dá leitura de headphone (opcional, mesma cor aqui).
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

    // Linhas de status (desabilitadas).
    let mi_bat = MenuItem::new("Bateria: —", false, None);
    let mi_fw = MenuItem::new("Firmware: —", false, None);

    // Modos de escuta — checkboxes "rádio": o ativo fica marcado.
    let mi_off = CheckMenuItem::new("ANC desligado", true, false, None);
    let mi_anc = CheckMenuItem::new("ANC ligado", true, false, None);
    let mi_talk = CheckMenuItem::new("TalkThru", true, false, None);
    let mi_amb = CheckMenuItem::new("Ambiente", true, false, None);

    // Nível 1..=7 — checkboxes "rádio".
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

    // Avisos de voz + Speak-to-Chat — toggles de estado.
    let mi_vp = CheckMenuItem::new("Avisos de voz", true, false, None);
    let mi_stc = CheckMenuItem::new("Speak-to-Chat", true, false, None);

    // Auto-desligar — checkboxes "rádio" com o preset ativo marcado.
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

    // Achar fone — ações one-shot (não são toggle de estado).
    // Achar fone: checkbox único. Marcado = tocando; desmarcar (ou apertar o play do fone) para.
    let mi_find = CheckMenuItem::new("Achar fone (tocar som)", true, false, None);
    let r_find = mi_find.clone();

    // Recursos — toggles de estado.
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

    // VoiceAware nível 1..=5 — checkboxes "rádio".
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


    // Som espacial — checkboxes "rádio": Desligado / Filme / Música / Jogo.
    let mi_sp_off = CheckMenuItem::new("Desligado", true, false, None);
    let mi_sp_movie = CheckMenuItem::new("Filme", true, false, None);
    let mi_sp_music = CheckMenuItem::new("Música", true, false, None);
    let mi_sp_game = CheckMenuItem::new("Jogo", true, false, None);
    let submenu_spatial = Submenu::new("Som espacial", true);
    submenu_spatial
        .append_items(&[&mi_sp_off, &mi_sp_movie, &mi_sp_music, &mi_sp_game])
        .expect("submenu som espacial");

    // Diagnóstico — leitura estática (NVKey). Linhas desabilitadas.
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

    // Clones para o refresh (set_checked); os originais ficam para o tratamento de clique.
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

    // Histerese da notificação de bateria: avisa ≤15%, rearma ≥25% (evita repetir a cada ciclo).
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
        // Diagnóstico (estático).
        if let Some(v) = s.serial.clone() {
            mi_diag_serial.set_text(format!("Série: {v}"));
        }
        if let Some(v) = s.chip.clone() {
            mi_diag_chip.set_text(format!("Chip: {v}"));
        }
        mi_diag_fw.set_text(format!("Firmware: {}", s.fw.clone().unwrap_or_else(|| "—".into())));

        // Modos de ANC: marca o ativo.
        let mode = s.anc.map(|(m, _)| m);
        r_off.set_checked(mode == Some(AncMode::Off));
        r_anc.set_checked(mode == Some(AncMode::Anc));
        r_talk.set_checked(mode == Some(AncMode::TalkThru));
        r_amb.set_checked(mode == Some(AncMode::Ambient));

        // Nível: marca o ativo (1..=7).
        let lvl = s.anc.map(|(_, l)| l);
        for (i, it) in r_levels.iter().enumerate() {
            it.set_checked(lvl == Some((i + 1) as u16));
        }

        // Som espacial: marca o ativo (Desligado quando !on; senão Filme/Música/Jogo conforme type 0/1/2).
        if let Some((on, kind)) = s.spatial_full {
            r_sp_off.set_checked(!on);
            r_sp_movie.set_checked(on && kind == 0);
            r_sp_music.set_checked(on && kind == 1);
            r_sp_game.set_checked(on && kind == 2);
        }

        // Toggles de estado: só atualiza quando o GET respondeu (senão mantém o último).
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

        // VoiceAware nível: marca o ativo (1..=5).
        let va_lvl = s.voiceaware_level;
        for (i, it) in r_va_levels.iter().enumerate() {
            it.set_checked(va_lvl == Some((i + 1) as u16));
        }

        // Auto-desligar: marca o preset ativo.
        for (i, it) in r_autooff.iter().enumerate() {
            let (_, en, min) = AUTOOFF_OPTS[i];
            let checked = match s.auto_off {
                Some((false, _)) => !en,              // "Nunca"
                Some((true, t)) => en && t == min,    // preset de minutos
                None => false,
            };
            it.set_checked(checked);
        }
    };

    // "Achar fone" tocando: (momento em que começou, hook da tecla Play/Pause se instalou).
    let mut ringing: Option<(Instant, Option<PlayStopHook>)> = None;

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::NewEvents(StartCause::Init) => refresh(&tray),
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if let Some((since, hook)) = &ringing {
                    // Tocando: não faz leituras demoradas (o hook precisa do loop livre). Só checa
                    // se o play foi apertado ou se estourou o limite de segurança.
                    let pressed = hook.as_ref().map_or(false, |h| h.take_pressed());
                    if pressed || since.elapsed() >= RING_MAX {
                        apply_findme(false);
                        r_find.set_checked(false);
                        ringing = None; // solta o hook
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
                    // Começou a tocar: instala o hook do Play/Pause (None = sem o atalho do botão).
                    apply_findme(true);
                    ringing = Some((Instant::now(), PlayStopHook::install()));
                } else {
                    apply_findme(false);
                    ringing = None; // solta o hook
                }
            }
        }

        // Próximo despertar: curto enquanto toca (para checar o play), normal caso contrário.
        if !matches!(*control_flow, ControlFlow::Exit) {
            let wait = if ringing.is_some() { RING_POLL } else { POLL };
            *control_flow = ControlFlow::WaitUntil(Instant::now() + wait);
        }
    });
}
