//! Airoha RACE protocol over USB-HID for the AKG N9 Hybrid.
//!
//! Wire format (little-endian lengths/ids), reverse-engineered and verified on hardware:
//!   RACE request packet: [0x05][0x5A][len:u16][id:u16][payload...]   where len = 2 + payload.len()
//!   HID output report:   [0x06][packet_len:u8][0x80][packet...]      padded to the report size (62)
//!   RACE reply packet:   [0x05][0x5B|0x5D][len:u16][id:u16][body...]
//!   The reply is read with HID get_input_report(0x07); byte 1 of that report is the number of
//!   valid bytes that follow (index 3..3+valid); the rest is stale buffer and must be ignored.
//!
//! This crate is READ-ONLY by design. Write/erase/FOTA/reset command ids are intentionally absent.
//! See docs/FUNCTIONS.md for the verified command catalogue and the safety rules.

pub const REPORT_SIZE: usize = 62;
pub const RACE_CMD: u8 = 0x5A; // request type (needs response)
pub const RACE_RESP: u8 = 0x5B; // synchronous response
pub const RACE_IND: u8 = 0x5D; // asynchronous indication

/// Verified RACE command ids (read-only).
pub mod id {
    pub const BATTERY: u16 = 0x0CD6;
    pub const ANC_STATUS: u16 = 0x0901;
    pub const FW_VERSION: u16 = 0x1C07;
    pub const MODEL_ID: u16 = 0x000E;
    pub const NVKEY_READ: u16 = 0x0A00;
    pub const PARTITION_INFO: u16 = 0x1C00;
    /// ANC control (MmiStage, 0x0E06): on/off, modo de escuta e nível.
    pub const ANC_CONTROL: u16 = 0x0E06;
    // --- outros recursos reversíveis (payloads verificados no APK) ---
    pub const VOICE_PROMPT: u16 = 0x0900; // MmiStage_SetVpOnoff
    pub const SPATIAL_3D: u16 = 0x0012; // MmiStage_Set3DSpatialStatus
    pub const LR_BALANCE: u16 = 0x0036; // MmiStage_LRBalance_Set
    pub const AUDIO_LIMITER: u16 = 0x0050; // MmiStage_AudioLimiterStatus_Set
    pub const CHAT_OPTIMIZER: u16 = 0x0052; // MmiStage_SetChatOptimizerStatus
    // --- toggles de fone (lote 3) ---
    pub const SPEAK_TO_CHAT: u16 = 0x0061; // MmiStage_SmartTalkStatus_Set (= Speak-to-Chat)
    pub const PSAP: u16 = 0x0078; // MmiStage_SetPsapStatus
    pub const SAFE_SOUND: u16 = 0x0015; // MmiStage_SetSafeSoundStatus
    pub const FIND_ME: u16 = 0x2C01; // MmiStageFindMe (RACE_FIND_ME) — toca som p/ achar o fone
    pub const AUTO_POWER_OFF: u16 = 0x2C82; // MmiStage_SetAutoPowerOffStatus (RACE_SET_MMI_COMMON_CONFIG)
    pub const HI_RES: u16 = 0x0076; // MmiStage_SetHiResAudioType
    pub const MIC_GAIN: u16 = 0x0065; // MmiStage_SetMicSettingGainLevel
    pub const LOW_VOL_EQ: u16 = 0x0059; // MmiStage_SetLowVolumeEqStatus
    pub const AUTO_PLAY_PAUSE: u16 = 0x000A; // MmiStage_IR_SetOnOff
    // --- ids de LEITURA (GET) de estado (não vão na whitelist de escrita) ---
    pub const SPATIAL_3D_GET: u16 = 0x0013; // MmiStage_Get3DSpatialStatus
    pub const AUDIO_LIMITER_GET: u16 = 0x0051; // MmiStage_AudioLimiterStatus_Get
    pub const CHAT_OPTIMIZER_GET: u16 = 0x0053; // MmiStage_GetChatOptimizerStatus
    pub const SPEAK_TO_CHAT_GET: u16 = 0x0060; // MmiStage_SmartTalkStatus_Get
    pub const AUTO_POWER_OFF_GET: u16 = 0x2C83; // MmiStage_GetAutoPowerOffStatus (RACE_GET_MMI_COMMON_CONFIG)
    pub const HI_RES_GET: u16 = 0x0075; // MmiStage_GetHiResAudioType
    pub const SAFE_SOUND_GET: u16 = 0x0014; // MmiStage_GetSafeSoundStatus
    pub const LR_BALANCE_GET: u16 = 0x0037; // MmiStage_LRBalance_Get
    pub const LOW_VOL_EQ_GET: u16 = 0x0058; // MmiStage_GetLowVolumeEqStatus
    /// Enviar evento de tecla (MmiStageSendKeyEvent). Só o keycode de desligar é exposto (apply_shutdown).
    pub const SEND_KEY_EVENT: u16 = 0x1101;
}

/// Which end of the link a query targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Headset = 0,
    Dongle = 1,
}

/// Encode a RACE request packet: [0x05][0x5A][len LE][id LE][payload].
pub fn encode_packet(id: u16, payload: &[u8]) -> Vec<u8> {
    let len = (2 + payload.len()) as u16;
    let mut p = Vec::with_capacity(6 + payload.len());
    p.push(0x05);
    p.push(RACE_CMD);
    p.push((len & 0xFF) as u8);
    p.push((len >> 8) as u8);
    p.push((id & 0xFF) as u8);
    p.push((id >> 8) as u8);
    p.extend_from_slice(payload);
    p
}

/// Wrap a RACE packet in the HID output report: [0x06][packet_len][0x80][packet...] padded.
pub fn hid_report(packet: &[u8]) -> [u8; REPORT_SIZE] {
    let mut r = [0u8; REPORT_SIZE];
    r[0] = 0x06;
    r[1] = packet.len() as u8;
    r[2] = 0x80;
    let n = packet.len().min(REPORT_SIZE - 3);
    r[3..3 + n].copy_from_slice(&packet[..n]);
    r
}

/// Convenience: full HID report for a command id + payload.
pub fn request(id: u16, payload: &[u8]) -> [u8; REPORT_SIZE] {
    hid_report(&encode_packet(id, payload))
}

// ---- request builders (read-only) -------------------------------------------------------------

pub fn req_battery() -> [u8; REPORT_SIZE] {
    request(id::BATTERY, &[0x00])
}
pub fn req_anc_status() -> [u8; REPORT_SIZE] {
    // payload: module 0x0905 (ANC), reserved 0x00
    request(id::ANC_STATUS, &[0x05, 0x00])
}
pub fn req_fw_version(role: Role) -> [u8; REPORT_SIZE] {
    request(id::FW_VERSION, &[role as u8])
}
pub fn req_model_id() -> [u8; REPORT_SIZE] {
    request(id::MODEL_ID, &[])
}
pub fn req_nvkey_read(nvid: u16, max_len: u16) -> [u8; REPORT_SIZE] {
    request(
        id::NVKEY_READ,
        &[
            (nvid & 0xFF) as u8,
            (nvid >> 8) as u8,
            (max_len & 0xFF) as u8,
            (max_len >> 8) as u8,
        ],
    )
}
pub fn req_partition_info() -> [u8; REPORT_SIZE] {
    request(id::PARTITION_INFO, &[0x00])
}

// ---- reversible write layer (Fase 3, validada no hardware) ------------------------------------
//
// SEGURANÇA: a única operação de escrita exposta é o controle de ANC (id 0x0E06), que é
// reversível e foi validado no fone relendo 0x0901. Flash (0x0704/0707/0702), FOTA, factory
// reset, shutdown e escrita de NVKey (0x0A01) NÃO são expressáveis aqui e são recusados em
// tempo de execução por `is_allowed_write`. Todo write passa por `send_write` (ponto único).

/// Mudança reversível de ANC sob o comando 0x0E06. Só o conjunto seguro: desligar, ou ligar
/// com um filtro de escuta. Off usa a sub-op 0x0B; os demais, sub-op 0x0A + byte de filtro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AncSet {
    Off,      // sub-op 0x0B
    Anc,      // sub-op 0x0A, filtro 0x01
    TalkThru, // sub-op 0x0A, filtro 0x09
    Ambient,  // sub-op 0x0A, filtro 0x0A
}

/// Ids que `send_write` aceita. Whitelist dura: só comandos reversíveis verificados. Flash/FOTA/
/// reset/shutdown e escrita de NVKey (0x0A01, usada por gesto/auto-standby) NÃO entram aqui.
pub const ALLOWED_WRITE_IDS: &[u16] = &[
    id::ANC_CONTROL,
    id::VOICE_PROMPT,
    id::SPATIAL_3D,
    id::LR_BALANCE,
    id::AUDIO_LIMITER,
    id::CHAT_OPTIMIZER,
    id::SPEAK_TO_CHAT,
    id::PSAP,
    id::SAFE_SOUND,
    id::FIND_ME,
    id::AUTO_POWER_OFF,
    id::HI_RES,
    id::MIC_GAIN,
    id::LOW_VOL_EQ,
    id::AUTO_PLAY_PAUSE,
    id::SEND_KEY_EVENT, // só p/ apply_shutdown (keycode fixo de desligar); nenhum outro key event é exposto
];

/// Verdadeiro só para ids da whitelist reversível. Flash/FOTA/reset/NVKey-write nunca passam.
pub fn is_allowed_write(id: u16) -> bool {
    ALLOWED_WRITE_IDS.contains(&id)
}

/// Payload RACE para uma mudança de ANC (sem o enquadramento). Bytes extraídos do APK:
/// `AncStageAncOff.genRacePackets` = `00 0B 01`; `AncStageAncOn.genRacePackets` = `00 0A <filtro> 00 01`.
/// Filtro cru (igual ao lido em 0x0901): 01=ANC, 09=TalkThru, 0A=Ambient. O `01` final é o enable
/// que faltava na 1ª tentativa (por isso TalkThru/Ambient só davam ack sem trocar o modo).
pub fn anc_set_payload(set: AncSet) -> Vec<u8> {
    match set {
        AncSet::Off => vec![0x00, 0x0B, 0x01],
        AncSet::Anc => vec![0x00, 0x0A, 0x01, 0x00, 0x01],
        AncSet::TalkThru => vec![0x00, 0x0A, 0x09, 0x00, 0x01],
        AncSet::Ambient => vec![0x00, 0x0A, 0x0A, 0x00, 0x01],
    }
}

/// Relatório HID para uma mudança reversível de ANC (comando 0x0E06). Para inspeção/testes;
/// o envio ao fone deve ir por `apply_anc`/`send_write` para passar pela whitelist.
pub fn req_anc_set(set: AncSet) -> [u8; REPORT_SIZE] {
    request(id::ANC_CONTROL, &anc_set_payload(set))
}

/// Ponto ÚNICO de escrita: só envia se o id estiver na whitelist reversível; caso contrário
/// recusa SEM tocar no dispositivo.
pub fn send_write<T: RaceTransport>(t: &T, id: u16, payload: &[u8]) -> std::io::Result<()> {
    if !is_allowed_write(id) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("write id 0x{id:04X} fora da whitelist reversivel"),
        ));
    }
    t.send(&request(id, payload))
}

/// Aplica uma mudança reversível de ANC no fone, passando pela whitelist de escrita.
pub fn apply_anc<T: RaceTransport>(t: &T, set: AncSet) -> std::io::Result<()> {
    send_write(t, id::ANC_CONTROL, &anc_set_payload(set))
}

/// Payload do nível de ANC/Ambiente (`MmiStage_SetAncAaLevel`, sub-op 0x1E): `00 1E <level LE16>`.
/// Ajusta a intensidade do modo ativo (ANC ou Ambient). Faixa típica 1..=7.
pub fn anc_level_payload(level: u16) -> Vec<u8> {
    vec![0x00, 0x1E, (level & 0xFF) as u8, (level >> 8) as u8]
}

/// Relatório HID do comando de nível (inspeção/teste). Enviar via `apply_anc_level`.
pub fn req_anc_level(level: u16) -> [u8; REPORT_SIZE] {
    request(id::ANC_CONTROL, &anc_level_payload(level))
}

/// Ajusta o nível do modo ativo (ANC/Ambient), passando pela whitelist.
pub fn apply_anc_level<T: RaceTransport>(t: &T, level: u16) -> std::io::Result<()> {
    send_write(t, id::ANC_CONTROL, &anc_level_payload(level))
}

// ---- outros recursos reversíveis (payloads verificados no genRacePackets do APK) --------------

/// Avisos de voz on/off (`MmiStage_SetVpOnoff`, 0x0900): payload `01 00 <onOff>`.
pub fn apply_voice_prompt<T: RaceTransport>(t: &T, on: bool) -> std::io::Result<()> {
    send_write(t, id::VOICE_PROMPT, &[0x01, 0x00, on as u8])
}

/// Áudio espacial 3D on/off (`MmiStage_Set3DSpatialStatus`, 0x0012): payload `<onOff> <tipo>`.
pub fn apply_spatial_3d<T: RaceTransport>(t: &T, on: bool, kind: u8) -> std::io::Result<()> {
    send_write(t, id::SPATIAL_3D, &[on as u8, kind])
}

/// Balanço L/R (`MmiStage_LRBalance_Set`, 0x0036): payload `<onOff> <nível>`.
pub fn apply_lr_balance<T: RaceTransport>(t: &T, on: bool, level: u8) -> std::io::Result<()> {
    send_write(t, id::LR_BALANCE, &[on as u8, level])
}

/// Limitador de áudio on/off (`MmiStage_AudioLimiterStatus_Set`, 0x0050): payload `<onOff>`.
pub fn apply_audio_limiter<T: RaceTransport>(t: &T, on: bool) -> std::io::Result<()> {
    send_write(t, id::AUDIO_LIMITER, &[on as u8])
}

/// Otimizador de chat on/off (`MmiStage_SetChatOptimizerStatus`, 0x0052): payload `<onOff>`.
pub fn apply_chat_optimizer<T: RaceTransport>(t: &T, on: bool) -> std::io::Result<()> {
    send_write(t, id::CHAT_OPTIMIZER, &[on as u8])
}

/// Auto-desligamento (`MmiStage_SetAutoPowerOffStatus`, 0x2C82, MODULE_ID=1): payload
/// `01 00 <en> 00 <timeout_min LE16>`. `enabled=false` = nunca desliga sozinho.
pub fn apply_auto_power_off<T: RaceTransport>(
    t: &T,
    enabled: bool,
    timeout_min: u16,
) -> std::io::Result<()> {
    send_write(
        t,
        id::AUTO_POWER_OFF,
        // O fone espera SEGUNDOS (o app envia minutos*60). Converte aqui.
        &[0x01, 0x00, enabled as u8, 0x00,
          ((timeout_min as u32 * 60).min(0xFFFF) as u16 & 0xFF) as u8,
          ((timeout_min as u32 * 60).min(0xFFFF) as u16 >> 8) as u8],
    )
}

/// Toca (ou para) um som para localizar o fone (`MmiStageFindMe`, 0x2C01): payload
/// `<blink> <ring> <target>`. Aqui: blink=0, ring=on, target=0 (ambos os lados).
pub fn apply_find_me<T: RaceTransport>(t: &T, ring: bool) -> std::io::Result<()> {
    send_write(t, id::FIND_ME, &[0x00, ring as u8, 0x00])
}

/// Speak-to-Chat / SmartTalk on/off (`MmiStage_SmartTalkStatus_Set`, 0x0061): payload
/// `<onOff> <latency>`. Latência 0 = padrão.
pub fn apply_speak_to_chat<T: RaceTransport>(t: &T, on: bool, latency: u8) -> std::io::Result<()> {
    send_write(t, id::SPEAK_TO_CHAT, &[on as u8, latency])
}

/// PSAP (realce auditivo) (`MmiStage_SetPsapStatus`, 0x0078): payload `<onOff> <level> <index>`.
pub fn apply_psap<T: RaceTransport>(t: &T, on: bool, level: u8, index: u8) -> std::io::Result<()> {
    send_write(t, id::PSAP, &[on as u8, level, index])
}

/// Safe Sound / proteção auditiva (`MmiStage_SetSafeSoundStatus`, 0x0015): payload `<modo>`.
pub fn apply_safe_sound<T: RaceTransport>(t: &T, mode: u8) -> std::io::Result<()> {
    send_write(t, id::SAFE_SOUND, &[mode])
}

/// EQ volume baixo on/off (`MmiStage_SetLowVolumeEqStatus`, 0x0059): payload `<onOff>`.
pub fn apply_low_vol_eq<T: RaceTransport>(t: &T, on: bool) -> std::io::Result<()> {
    send_write(t, id::LOW_VOL_EQ, &[on as u8])
}

/// Desliga o fone (`MmiStageSendKeyEvent`, 0x1101, keycode power-off `{0x00,0x18}` → payload `18 00`).
/// Ação benigna e reversível (liga de volta no botão); só é enviada quando o usuário clica.
/// `doShutdownDevice` no APK: `bArr = {0, Ascii.CAN(0x18)}`; genRacePackets escreve `[1]` depois `[0]`.
pub fn apply_shutdown<T: RaceTransport>(t: &T) -> std::io::Result<()> {
    send_write(t, id::SEND_KEY_EVENT, &[0x18, 0x00])
}

/// Tipo de áudio Hi-Res (`MmiStage_SetHiResAudioType`, 0x0076): payload `<tipo>`.
pub fn apply_hi_res<T: RaceTransport>(t: &T, kind: u8) -> std::io::Result<()> {
    send_write(t, id::HI_RES, &[kind])
}

/// Ganho do microfone (`MmiStage_SetMicSettingGainLevel`, 0x0065): payload `<ganho>`.
pub fn apply_mic_gain<T: RaceTransport>(t: &T, gain: u8) -> std::io::Result<()> {
    send_write(t, id::MIC_GAIN, &[gain])
}

/// Pausa a música ao tirar o fone (`MmiStage_IR_SetOnOff`, 0x000A): payload `<onOff>`.
pub fn apply_auto_play_pause<T: RaceTransport>(t: &T, on: bool) -> std::io::Result<()> {
    send_write(t, id::AUTO_PLAY_PAUSE, &[on as u8])
}

/// VoiceAware / Sidetone on/off (0x2C82, módulo 0x0007): payload `07 00 <onOff>`.
pub fn apply_voiceaware<T: RaceTransport>(t: &T, on: bool) -> std::io::Result<()> {
    send_write(t, id::AUTO_POWER_OFF, &[0x07, 0x00, on as u8])
}

/// Nível de VoiceAware (0x2C82, módulo 0x0006): payload `06 00 <level LE16>`.
pub fn apply_voiceaware_level<T: RaceTransport>(t: &T, level: u16) -> std::io::Result<()> {
    send_write(
        t,
        id::AUTO_POWER_OFF,
        &[0x06, 0x00, (level & 0xFF) as u8, (level >> 8) as u8],
    )
}

/// Ativa um grupo PEQ (PeqStageSetPeqGrpIdx, 0x0900, módulo 0x0000): payload `00 00 <index>`.
/// Reversível, mas NÃO seleciona um preset de música sozinho: é só o passo final de "ativar grupo"
/// do fluxo de EQ (sem carregar coeficientes, troca o grupo ativo — no N9 isso cai nas definições de
/// modo chamada). O EQ de música de verdade exige o fluxo completo (coeficientes + save NVKey). Mantido
/// para uso futuro nesse fluxo; não exposto no tray.
pub fn apply_eq_group<T: RaceTransport>(t: &T, index: u8) -> std::io::Result<()> {
    send_write(t, id::VOICE_PROMPT, &[0x00, 0x00, index])
}

// ---- reply parsing ----------------------------------------------------------------------------

/// Find the RACE reply frame for `id` inside a 0x07 input report and return the body *after* the
/// 2-byte id. The whole report (index 3..) is scanned, because over the dongle the relayed frame
/// can sit past `report[1]`'s valid count, and the same id may appear both as a short synchronous
/// response (0x5B) and a fuller indication (0x5D) — we keep the **longest** matching body so the
/// caller gets the frame that actually carries the data. Matching the exact `want_id` keeps stale
/// bytes from being mistaken for a reply.
pub fn find_reply<'a>(report: &'a [u8], want_id: u16) -> Option<&'a [u8]> {
    let end_scan = report.len();
    let mut best: Option<&'a [u8]> = None;
    let mut best_resp = false;
    let mut i = 3;
    while i + 6 <= end_scan {
        if report[i] == 0x05 && (report[i + 1] == RACE_RESP || report[i + 1] == RACE_IND) {
            let is_resp = report[i + 1] == RACE_RESP;
            let len = report[i + 2] as usize | ((report[i + 3] as usize) << 8);
            let id = report[i + 4] as u16 | ((report[i + 5] as u16) << 8);
            let end = (i + 4 + len).min(end_scan);
            if id == want_id && end > i + 6 {
                let body = &report[i + 6..end];
                // Prioridade: corpo mais longo (a bateria vem na indicação, que é maior); em caso
                // de empate, prefere a RESPOSTA síncrona (0x5B) à indicação (0x5D) — assim a
                // resposta atual ganha de uma indicação antiga presa no buffer do dongle (o que
                // deixava o nível de ANC "preso" num valor velho).
                let better = match best {
                    None => true,
                    Some(b) => body.len() > b.len() || (body.len() == b.len() && is_resp && !best_resp),
                };
                if better {
                    best = Some(body);
                    best_resp = is_resp;
                }
            }
        }
        i += 1;
    }
    best
}

/// Lê o estado dos avisos de voz (GET 0x0901, módulo 0x0001): requisição `01 00`.
pub fn req_vp_status() -> [u8; REPORT_SIZE] {
    request(id::ANC_STATUS, &[0x01, 0x00])
}

/// Decodifica a resposta de `req_vp_status`. Como 0x0901 é multiplexado (ANC/VP/nível), filtra
/// pelo módulo no corpo (`01 00`): corpo = `[01 00 <status> <onOff>]`. Retorna o on/off.
pub fn parse_vp_status(report: &[u8]) -> Option<bool> {
    let end = report.len();
    let mut i = 3;
    while i + 10 <= end {
        if report[i] == 0x05 && (report[i + 1] == RACE_RESP || report[i + 1] == RACE_IND) {
            let id = report[i + 4] as u16 | ((report[i + 5] as u16) << 8);
            if id == id::ANC_STATUS && report[i + 6] == 0x01 && report[i + 7] == 0x00 {
                return Some(report[i + 9] != 0);
            }
        }
        i += 1;
    }
    None
}

/// Decodifica um GET simples de on/off: corpo `[<x> <onOff> …]`, on/off no índice 1 do corpo.
fn parse_onoff(report: &[u8], want_id: u16) -> Option<bool> {
    find_reply(report, want_id).and_then(|b| b.get(1)).map(|&v| v != 0)
}

/// GET do áudio 3D (0x0013): requisição vazia; `parse_spatial_3d_status` → on/off.
pub fn req_spatial_3d_status() -> [u8; REPORT_SIZE] {
    request(id::SPATIAL_3D_GET, &[])
}
pub fn parse_spatial_3d_status(report: &[u8]) -> Option<bool> {
    parse_onoff(report, id::SPATIAL_3D_GET)
}
pub fn parse_spatial_3d_full(report: &[u8]) -> Option<(bool, u8)> {
    let b = find_reply(report, id::SPATIAL_3D_GET)?;
    Some((*b.get(1)? != 0, *b.get(2)?))
}

/// GET do EQ volume baixo (0x0058): requisição vazia; `parse_low_vol_eq_status` → on/off.
pub fn req_low_vol_eq_status() -> [u8; REPORT_SIZE] {
    request(id::LOW_VOL_EQ_GET, &[])
}
pub fn parse_low_vol_eq_status(report: &[u8]) -> Option<bool> {
    parse_onoff(report, id::LOW_VOL_EQ_GET)
}

/// GET do limitador de áudio (0x0051): requisição vazia; `parse_audio_limiter_status` → on/off.
pub fn req_audio_limiter_status() -> [u8; REPORT_SIZE] {
    request(id::AUDIO_LIMITER_GET, &[])
}
pub fn parse_audio_limiter_status(report: &[u8]) -> Option<bool> {
    parse_onoff(report, id::AUDIO_LIMITER_GET)
}

/// GET do chat optimizer (0x0053): requisição vazia; `parse_chat_optimizer_status` → on/off.
pub fn req_chat_optimizer_status() -> [u8; REPORT_SIZE] {
    request(id::CHAT_OPTIMIZER_GET, &[])
}
pub fn parse_chat_optimizer_status(report: &[u8]) -> Option<bool> {
    parse_onoff(report, id::CHAT_OPTIMIZER_GET)
}

/// GET do Speak-to-Chat / SmartTalk (0x0060): requisição vazia; on/off no índice 1 do corpo.
pub fn req_speak_to_chat_status() -> [u8; REPORT_SIZE] {
    request(id::SPEAK_TO_CHAT_GET, &[])
}
pub fn parse_speak_to_chat_status(report: &[u8]) -> Option<bool> {
    parse_onoff(report, id::SPEAK_TO_CHAT_GET)
}

/// GET do Hi-Res (0x0075): on/off no índice 1 do corpo.
pub fn req_hi_res_status() -> [u8; REPORT_SIZE] {
    request(id::HI_RES_GET, &[])
}
pub fn parse_hi_res_status(report: &[u8]) -> Option<bool> {
    parse_onoff(report, id::HI_RES_GET)
}

/// GET do Safe Sound (0x0014): on/off no índice 1 do corpo.
pub fn req_safe_sound_status() -> [u8; REPORT_SIZE] {
    request(id::SAFE_SOUND_GET, &[])
}
pub fn parse_safe_sound_status(report: &[u8]) -> Option<bool> {
    parse_onoff(report, id::SAFE_SOUND_GET)
}

/// GET do balanço L/R (0x0037): isOn no índice 1 do corpo (nível no índice 2, ignorado aqui).
pub fn req_lr_balance_status() -> [u8; REPORT_SIZE] {
    request(id::LR_BALANCE_GET, &[])
}
pub fn parse_lr_balance_status(report: &[u8]) -> Option<bool> {
    parse_onoff(report, id::LR_BALANCE_GET)
}

/// GET do auto-desligamento (0x2C83, módulo 0x0001): requisição `01 00`. Corpo:
/// `[<status> 01 00 <enabled> <?> <timeout_lo> <timeout_hi>]` → (enabled, timeout_min).
pub fn req_auto_power_off_status() -> [u8; REPORT_SIZE] {
    request(id::AUTO_POWER_OFF_GET, &[0x01, 0x00])
}
pub fn parse_auto_power_off_status(report: &[u8]) -> Option<(bool, u16)> {
    let body = find_reply(report, id::AUTO_POWER_OFF_GET)?;
    // body: [status, módulo_lo(01), módulo_hi(00), enabled, _, timeout_lo, timeout_hi]
    if *body.get(1)? != 0x01 {
        return None;
    }
    let enabled = *body.get(3)? != 0;
    // O fone responde em segundos; convertemos para minutos.
    let timeout = (*body.get(5)? as u16 | ((*body.get(6)? as u16) << 8)) / 60;
    Some((enabled, timeout))
}

/// GET do VoiceAware estado (0x2C83, módulo 0x0007): requisição `07 00`.
pub fn req_voiceaware_status() -> [u8; REPORT_SIZE] {
    request(id::AUTO_POWER_OFF_GET, &[0x07, 0x00])
}

/// Decodifica o estado do VoiceAware (0x2C83, módulo 0x0007). Corpo: `[status, 07, 00, <on>]`.
pub fn parse_voiceaware_status(report: &[u8]) -> Option<bool> {
    let body = find_reply(report, id::AUTO_POWER_OFF_GET)?;
    if *body.get(1)? != 0x07 || *body.get(2)? != 0x00 {
        return None;
    }
    Some(*body.get(3)? != 0)
}

/// GET do VoiceAware nível (0x2C83, módulo 0x0006): requisição `06 00`.
pub fn req_voiceaware_level() -> [u8; REPORT_SIZE] {
    request(id::AUTO_POWER_OFF_GET, &[0x06, 0x00])
}

/// Decodifica o nível do VoiceAware (0x2C83, módulo 0x0006). Corpo: `[status, 06, 00, lvl_lo, lvl_hi]`.
pub fn parse_voiceaware_level(report: &[u8]) -> Option<u16> {
    let body = find_reply(report, id::AUTO_POWER_OFF_GET)?;
    if *body.get(1)? != 0x06 || *body.get(2)? != 0x00 {
        return None;
    }
    let lo = *body.get(3)? as u16;
    let hi = *body.get(4)? as u16;
    Some(lo | (hi << 8))
}

/// Battery percent (0..100) from a reply report, if present.
pub fn parse_battery(report: &[u8]) -> Option<u8> {
    let body = find_reply(report, id::BATTERY)?; // [status, agent, percent]
    let pct = *body.get(2)?;
    (pct <= 100).then_some(pct)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AncMode {
    Off,
    Anc,
    TalkThru,
    Ambient,
    Other(u8),
}

/// ANC filter + level (1..=7). Mapping verified no hardware: 0x00 off, 0x01 ANC, 0x09 TalkThru,
/// 0x0A Ambient Aware.
pub fn parse_anc(report: &[u8]) -> Option<(AncMode, u16)> {
    let body = find_reply(report, id::ANC_STATUS)?; // [modLo, modHi, status, filter, lvlLo, lvlHi]
    let filter = *body.get(3)?;
    let level = *body.get(4)? as u16 | ((*body.get(5)? as u16) << 8);
    let mode = match filter {
        0x00 => AncMode::Off,
        0x01 => AncMode::Anc,
        0x09 => AncMode::TalkThru,
        0x0A => AncMode::Ambient,
        other => AncMode::Other(other),
    };
    Some((mode, level))
}

/// Firmware version string, e.g. "v4.5.0".
pub fn parse_fw_version(report: &[u8]) -> Option<String> {
    let body = find_reply(report, id::FW_VERSION)?; // [status, agent, len, ascii...]
    let len = *body.get(2)? as usize;
    let data = body.get(3..3 + len)?;
    Some(String::from_utf8_lossy(data).into_owned())
}

/// Raw NVKey value (empty vec means the key exists but is empty).
pub fn parse_nvkey(report: &[u8]) -> Option<Vec<u8>> {
    let body = find_reply(report, id::NVKEY_READ)?; // [lenLo, lenHi, data...]
    let len = *body.get(0)? as usize | ((*body.get(1)? as usize) << 8);
    let data = body.get(2..2 + len)?;
    Some(data.to_vec())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partition {
    pub id: u8,
    pub storage_type: u8,
    pub start: u32,
    pub length: u32,
}

/// FOTA/background partition info (address + length) from a reply.
pub fn parse_partition(report: &[u8]) -> Option<Partition> {
    let b = find_reply(report, id::PARTITION_INFO)?; // [status, partId, storage, addr(4), len(4)]
    let rd32 = |s: &[u8]| s[0] as u32 | (s[1] as u32) << 8 | (s[2] as u32) << 16 | (s[3] as u32) << 24;
    Some(Partition {
        id: *b.get(1)?,
        storage_type: *b.get(2)?,
        start: rd32(b.get(3..7)?),
        length: rd32(b.get(7..11)?),
    })
}

// ---- transport --------------------------------------------------------------------------------

/// A RACE transport: send one HID output report, read one input report. Implemented over hidapi
/// for real hardware; mockable for tests.
pub trait RaceTransport {
    fn send(&self, report: &[u8; REPORT_SIZE]) -> std::io::Result<()>;
    fn recv(&self) -> std::io::Result<[u8; REPORT_SIZE]>;
}

#[cfg(feature = "hid")]
pub mod hid {
    use super::*;
    use hidapi::{HidApi, HidDevice};

    pub const VID_HARMAN: u16 = 0x0ECB;
    pub const PID_HEADSET_USB: u16 = 0x20D8; // preferred: headset over USB-C
    pub const PID_DONGLE: u16 = 0x20D9; // fallback: 2.4 GHz dongle

    /// Open the RACE collection (usage page 0xFF13, 62-byte reports). Tries the headset first,
    /// then the dongle. The caller keeps the handle for the session.
    pub struct HidRace {
        dev: HidDevice,
    }

    impl HidRace {
        pub fn open() -> std::io::Result<Self> {
            let api = HidApi::new().map_err(to_io)?;
            for pid in [PID_HEADSET_USB, PID_DONGLE] {
                for info in api.device_list() {
                    if info.vendor_id() == VID_HARMAN
                        && info.product_id() == pid
                        && info.usage_page() == 0xFF13
                        && info.usage() == 0x0001
                    {
                        if let Ok(dev) = info.open_device(&api) {
                            return Ok(Self { dev });
                        }
                    }
                }
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "AKG N9 RACE interface not found (headset 0ECB:20D8 or dongle 0ECB:20D9)",
            ))
        }
    }

    impl RaceTransport for HidRace {
        fn send(&self, report: &[u8; REPORT_SIZE]) -> std::io::Result<()> {
            self.dev.send_feature_report(report).or_else(|_| {
                // Fall back to output report if feature is rejected.
                self.dev.write(report).map(|_| ()).map_err(to_io)
            })
        }
        fn recv(&self) -> std::io::Result<[u8; REPORT_SIZE]> {
            let mut buf = [0u8; REPORT_SIZE];
            buf[0] = 0x07;
            self.dev.get_input_report(&mut buf).map_err(to_io)?;
            Ok(buf)
        }
    }

    fn to_io(e: hidapi::HidError) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_request_bytes() {
        let pkt = encode_packet(id::BATTERY, &[0x00]);
        assert_eq!(pkt, [0x05, 0x5A, 0x03, 0x00, 0xD6, 0x0C, 0x00]);
        let r = req_battery();
        assert_eq!(&r[0..10], &[0x06, 0x07, 0x80, 0x05, 0x5A, 0x03, 0x00, 0xD6, 0x0C, 0x00]);
    }

    /// Transporte de teste: registra o que foi enviado; `recv` devolve um frame fixo.
    struct MockTx {
        sent: std::cell::RefCell<Vec<Vec<u8>>>,
    }
    impl MockTx {
        fn new() -> Self {
            Self { sent: std::cell::RefCell::new(Vec::new()) }
        }
    }
    impl RaceTransport for MockTx {
        fn send(&self, report: &[u8; REPORT_SIZE]) -> std::io::Result<()> {
            self.sent.borrow_mut().push(report.to_vec());
            Ok(())
        }
        fn recv(&self) -> std::io::Result<[u8; REPORT_SIZE]> {
            Ok([0u8; REPORT_SIZE])
        }
    }

    #[test]
    fn anc_write_bytes_match_stage_code() {
        // Pacotes exatamente como o app monta (AncStageAncOff/AncOn.genRacePackets no APK).
        // Off: payload 00 0B 01 -> 05 5A 05 00 06 0E 00 0B 01
        assert_eq!(
            &req_anc_set(AncSet::Off)[0..12],
            &[0x06, 0x09, 0x80, 0x05, 0x5A, 0x05, 0x00, 0x06, 0x0E, 0x00, 0x0B, 0x01]
        );
        // On ANC: payload 00 0A 01 00 01 -> 05 5A 07 00 06 0E 00 0A 01 00 01
        assert_eq!(
            &req_anc_set(AncSet::Anc)[0..14],
            &[0x06, 0x0B, 0x80, 0x05, 0x5A, 0x07, 0x00, 0x06, 0x0E, 0x00, 0x0A, 0x01, 0x00, 0x01]
        );
        assert_eq!(anc_set_payload(AncSet::TalkThru), vec![0x00, 0x0A, 0x09, 0x00, 0x01]);
        assert_eq!(anc_set_payload(AncSet::Ambient), vec![0x00, 0x0A, 0x0A, 0x00, 0x01]);
    }

    #[test]
    fn anc_level_bytes() {
        // MmiStage_SetAncAaLevel: 00 1E <level LE16> -> nivel 3 = 05 5A 06 00 06 0E 00 1E 03 00
        assert_eq!(anc_level_payload(3), vec![0x00, 0x1E, 0x03, 0x00]);
        assert_eq!(
            &req_anc_level(3)[0..13],
            &[0x06, 0x0A, 0x80, 0x05, 0x5A, 0x06, 0x00, 0x06, 0x0E, 0x00, 0x1E, 0x03, 0x00]
        );
    }

    #[test]
    fn feature_writes_go_through_whitelist_with_right_bytes() {
        let tx = MockTx::new();
        apply_voice_prompt(&tx, true).unwrap();
        apply_lr_balance(&tx, true, 5).unwrap();
        apply_audio_limiter(&tx, false).unwrap();
        let sent = tx.sent.borrow();
        // 0x0900 payload 01 00 01 -> 05 5A 05 00 00 09 01 00 01
        assert_eq!(&sent[0][3..12], &[0x05, 0x5A, 0x05, 0x00, 0x00, 0x09, 0x01, 0x00, 0x01]);
        // 0x0036 payload 01 05 -> 05 5A 04 00 36 00 01 05
        assert_eq!(&sent[1][3..11], &[0x05, 0x5A, 0x04, 0x00, 0x36, 0x00, 0x01, 0x05]);
        // 0x0050 payload 00 -> 05 5A 03 00 50 00 00
        assert_eq!(&sent[2][3..10], &[0x05, 0x5A, 0x03, 0x00, 0x50, 0x00, 0x00]);
    }

    #[test]
    fn low_vol_eq_write_bytes() {
        let tx = MockTx::new();
        apply_low_vol_eq(&tx, true).unwrap();
        // on=true -> payload 05 5A 03 00 59 00 01
        assert_eq!(
            &tx.sent.borrow()[0][3..10],
            &[0x05, 0x5A, 0x03, 0x00, 0x59, 0x00, 0x01]
        );
    }

    #[test]
    fn parse_spatial_3d_full_corpo() {
        // corpo 00 01 02 -> on=true, type=2
        let mut rep = [0u8; REPORT_SIZE];
        let b = [0x07u8, 0x09, 0x80, 0x05, 0x5B, 0x05, 0x00, 0x13, 0x00, 0x00, 0x01, 0x02];
        rep[..b.len()].copy_from_slice(&b);
        assert_eq!(parse_spatial_3d_full(&rep), Some((true, 2)));
    }

    #[test]
    fn toggle_batch3_bytes() {
        let tx = MockTx::new();
        apply_auto_power_off(&tx, true, 10).unwrap();
        apply_find_me(&tx, true).unwrap();
        apply_speak_to_chat(&tx, true, 0).unwrap();
        let sent = tx.sent.borrow();
        // auto-off: 10 min -> 600 s (58 02) -> 05 5A 08 00 82 2C 01 00 01 00 58 02
        assert_eq!(
            &sent[0][3..15],
            &[0x05, 0x5A, 0x08, 0x00, 0x82, 0x2C, 0x01, 0x00, 0x01, 0x00, 0x58, 0x02]
        );
        // find me: 00 01 00 -> 05 5A 05 00 01 2C 00 01 00
        assert_eq!(&sent[1][3..12], &[0x05, 0x5A, 0x05, 0x00, 0x01, 0x2C, 0x00, 0x01, 0x00]);
        // speak-to-chat: 01 00 -> 05 5A 04 00 61 00 01 00
        assert_eq!(&sent[2][3..11], &[0x05, 0x5A, 0x04, 0x00, 0x61, 0x00, 0x01, 0x00]);
    }

    #[test]
    fn eq_group_bytes() {
        let tx = MockTx::new();
        apply_eq_group(&tx, 2).unwrap();
        // 00 00 02 -> 05 5A 05 00 00 09 00 00 02
        assert_eq!(
            &tx.sent.borrow()[0][3..12],
            &[0x05, 0x5A, 0x05, 0x00, 0x00, 0x09, 0x00, 0x00, 0x02]
        );
    }

    #[test]
    fn shutdown_bytes() {
        let tx = MockTx::new();
        apply_shutdown(&tx).unwrap();
        // 0x1101 payload 18 00 -> 05 5A 04 00 01 11 18 00
        assert_eq!(&tx.sent.borrow()[0][3..11], &[0x05, 0x5A, 0x04, 0x00, 0x01, 0x11, 0x18, 0x00]);
    }

    #[test]
    fn auto_play_pause_bytes() {
        let tx = MockTx::new();
        apply_auto_play_pause(&tx, true).unwrap();
        // 05 5A 03 00 0A 00 01
        assert_eq!(
            &tx.sent.borrow()[0][3..10],
            &[0x05, 0x5A, 0x03, 0x00, 0x0A, 0x00, 0x01]
        );
    }

    #[test]
    fn voiceaware_bytes_and_parse() {
        let tx = MockTx::new();
        // SET estado on -> 05 5A 05 00 82 2C 07 00 01
        apply_voiceaware(&tx, true).unwrap();
        assert_eq!(
            &tx.sent.borrow()[0][3..12],
            &[0x05, 0x5A, 0x05, 0x00, 0x82, 0x2C, 0x07, 0x00, 0x01]
        );
        // SET nível 3 -> 05 5A 06 00 82 2C 06 00 03 00
        apply_voiceaware_level(&tx, 3).unwrap();
        assert_eq!(
            &tx.sent.borrow()[1][3..13],
            &[0x05, 0x5A, 0x06, 0x00, 0x82, 0x2C, 0x06, 0x00, 0x03, 0x00]
        );

        // GET estado: corpo [00 07 00 01] -> Some(true)
        let mut rep_st = [0u8; REPORT_SIZE];
        let b_st = [
            0x07u8, 0x0A, 0x80, 0x05, 0x5B, 0x06, 0x00, 0x83, 0x2C, 0x00, 0x07, 0x00, 0x01,
        ];
        rep_st[..b_st.len()].copy_from_slice(&b_st);
        assert_eq!(parse_voiceaware_status(&rep_st), Some(true));

        // GET nível: corpo [00 06 00 03 00] -> Some(3)
        let mut rep_lvl = [0u8; REPORT_SIZE];
        let b_lvl = [
            0x07u8, 0x0B, 0x80, 0x05, 0x5B, 0x07, 0x00, 0x83, 0x2C, 0x00, 0x06, 0x00, 0x03, 0x00,
        ];
        rep_lvl[..b_lvl.len()].copy_from_slice(&b_lvl);
        assert_eq!(parse_voiceaware_level(&rep_lvl), Some(3));
    }

    #[test]
    fn whitelist_rejects_dangerous_ids() {
        assert!(is_allowed_write(id::ANC_CONTROL));
        // Flash sector erase, page program, set protect, NVKey write, FW/partition.
        for bad in [0x0704u16, 0x0707, 0x0702, 0x0A01, 0x1C07, 0x1C00, 0x0A00] {
            assert!(!is_allowed_write(bad), "id 0x{bad:04X} nao deveria passar");
        }
    }

    #[test]
    fn send_write_refuses_without_touching_device() {
        let tx = MockTx::new();
        // Id perigoso: deve retornar erro e NAO enviar nada.
        let err = send_write(&tx, 0x0A01, &[0xDE, 0xAD]).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(tx.sent.borrow().is_empty(), "nenhum write deveria ter sido enviado");
        // Id reversivel: envia e bate com o pacote validado.
        apply_anc(&tx, AncSet::Off).unwrap();
        assert_eq!(tx.sent.borrow().len(), 1);
        assert_eq!(
            &tx.sent.borrow()[0][0..12],
            &[0x06, 0x09, 0x80, 0x05, 0x5A, 0x05, 0x00, 0x06, 0x0E, 0x00, 0x0B, 0x01]
        );
    }

    #[test]
    fn fw_request_role() {
        assert_eq!(
            &req_fw_version(Role::Dongle)[0..10],
            &[0x06, 0x07, 0x80, 0x05, 0x5A, 0x03, 0x00, 0x07, 0x1C, 0x01]
        );
    }

    #[test]
    fn parse_battery_50() {
        // report: 07 <valid> 80 05 5D 05 00 D6 0C 00 00 32 ...
        let mut rep = [0u8; REPORT_SIZE];
        rep[0] = 0x07;
        rep[1] = 0x0C; // 12 valid bytes follow index 3
        rep[2] = 0x80;
        rep[3..12].copy_from_slice(&[0x05, 0x5D, 0x05, 0x00, 0xD6, 0x0C, 0x00, 0x00, 0x32]);
        assert_eq!(parse_battery(&rep), Some(0x32));
    }

    #[test]
    fn parse_battery_dongle_zero_validcount_prefers_indication() {
        // Relatorio real do dongle: report[1]=0x00 (contagem nao cobre o frame), com uma resposta
        // curta 05 5B 03 (sem porcentagem) ANTES da indicacao 05 5D 05 (com a porcentagem 0x64).
        let mut rep = [0u8; REPORT_SIZE];
        let bytes = [
            0x07u8, 0x00, 0x80, 0x54, 0x3F, 0x20, 0x04, 0x54, 0x00, 0x00, 0x01, 0xFF, 0xFF, //
            0x05, 0x5B, 0x03, 0x00, 0xD6, 0x0C, 0x00, // resposta curta, body [00]
            0x05, 0x5D, 0x05, 0x00, 0xD6, 0x0C, 0x00, 0x00, 0x64, // indicacao, body [00 00 64]
        ];
        rep[..bytes.len()].copy_from_slice(&bytes);
        assert_eq!(parse_battery(&rep), Some(0x64));
    }

    #[test]
    fn parse_autooff_status() {
        // 0x2C83 corpo: status 01 00 <en=01> 00 <to=600s LE = 58 02> -> (true, 10 min)
        let mut rep = [0u8; REPORT_SIZE];
        let b = [
            0x07u8, 0x0D, 0x80, 0x05, 0x5B, 0x09, 0x00, 0x83, 0x2C, 0x00, 0x01, 0x00, 0x01, 0x00,
            0x58, 0x02,
        ];
        rep[..b.len()].copy_from_slice(&b);
        assert_eq!(parse_auto_power_off_status(&rep), Some((true, 10)));
        // apply: 10 min -> 600 s (58 02) no payload
        let tx = MockTx::new();
        apply_auto_power_off(&tx, true, 10).unwrap();
        assert_eq!(
            &tx.sent.borrow()[0][3..15],
            &[0x05, 0x5A, 0x08, 0x00, 0x82, 0x2C, 0x01, 0x00, 0x01, 0x00, 0x58, 0x02]
        );
    }

    #[test]
    fn parse_onoff_gets() {
        // 3D (0x0013) ligado: 05 5B 04 00 13 00 <x=00> <onOff=01>
        let mut rep = [0u8; REPORT_SIZE];
        let b = [0x07u8, 0x08, 0x80, 0x05, 0x5B, 0x04, 0x00, 0x13, 0x00, 0x00, 0x01];
        rep[..b.len()].copy_from_slice(&b);
        assert_eq!(parse_spatial_3d_status(&rep), Some(true));
        // limiter (0x0051) desligado
        let mut rep2 = [0u8; REPORT_SIZE];
        let b2 = [0x07u8, 0x08, 0x80, 0x05, 0x5B, 0x04, 0x00, 0x51, 0x00, 0x00, 0x00];
        rep2[..b2.len()].copy_from_slice(&b2);
        assert_eq!(parse_audio_limiter_status(&rep2), Some(false));
    }

    #[test]
    fn parse_vp_on() {
        // frame 0x0901 modulo 01: 05 5B 06 00 01 09 01 00 00 01 -> onOff = 1 (ligado)
        let mut rep = [0u8; REPORT_SIZE];
        let bytes = [
            0x07u8, 0x0A, 0x80, 0x05, 0x5B, 0x06, 0x00, 0x01, 0x09, 0x01, 0x00, 0x00, 0x01,
        ];
        rep[..bytes.len()].copy_from_slice(&bytes);
        assert_eq!(parse_vp_status(&rep), Some(true));
        // garante que nao confunde com o frame ANC (modulo 05)
        let mut anc = [0u8; REPORT_SIZE];
        let ab = [
            0x07u8, 0x0C, 0x80, 0x05, 0x5B, 0x08, 0x00, 0x01, 0x09, 0x05, 0x00, 0x00, 0x01, 0x01,
            0x00,
        ];
        anc[..ab.len()].copy_from_slice(&ab);
        assert_eq!(parse_vp_status(&anc), None);
    }

    #[test]
    fn parse_anc_prefers_resp_over_stale_ind() {
        // Buffer do dongle com indicação velha (5D, nível 1) ANTES da resposta atual (5B, nível 6).
        // parse_anc deve devolver o nível 6 (a resposta síncrona), não o 1 da indicação presa.
        let mut rep = [0u8; REPORT_SIZE];
        let bytes = [
            0x07u8, 0x00, 0x80, //
            0x05, 0x5D, 0x08, 0x00, 0x01, 0x09, 0x05, 0x00, 0x00, 0x01, 0x01, 0x00, // IND nível 1
            0x05, 0x5B, 0x08, 0x00, 0x01, 0x09, 0x05, 0x00, 0x00, 0x01, 0x06, 0x00, // RESP nível 6
        ];
        rep[..bytes.len()].copy_from_slice(&bytes);
        assert_eq!(parse_anc(&rep), Some((AncMode::Anc, 6)));
    }

    #[test]
    fn parse_talkthru_filter_09() {
        // body ANC: [05 00 00 09 00 00] -> filtro 0x09 = TalkThru
        let mut rep = [0u8; REPORT_SIZE];
        let bytes = [
            0x07u8, 0x0C, 0x80, 0x05, 0x5B, 0x08, 0x00, 0x01, 0x09, 0x05, 0x00, 0x00, 0x09, 0x00,
            0x00,
        ];
        rep[..bytes.len()].copy_from_slice(&bytes);
        assert_eq!(parse_anc(&rep), Some((AncMode::TalkThru, 0)));
    }

    #[test]
    fn parse_nvkey_name() {
        // 05 5B 11 00 00 0A 0D 00 "AKG N9 Hybrid"
        let name = b"AKG N9 Hybrid";
        let mut body = vec![0x05, 0x5B, 0x11, 0x00, 0x00, 0x0A, 0x0D, 0x00];
        body.extend_from_slice(name);
        let mut rep = [0u8; REPORT_SIZE];
        rep[0] = 0x07;
        rep[1] = body.len() as u8;
        rep[3..3 + body.len()].copy_from_slice(&body);
        assert_eq!(parse_nvkey(&rep).as_deref(), Some(&name[..]));
    }

    #[test]
    fn parse_partition_fota() {
        // start 0x004ED000, length 0x00302000
        let body = [
            0x05, 0x5B, 0x0D, 0x00, 0x00, 0x1C, 0x00, 0x00, 0x00, 0x00, 0xD0, 0x4E, 0x00, 0x00,
            0x20, 0x30, 0x00,
        ];
        let mut rep = [0u8; REPORT_SIZE];
        rep[0] = 0x07;
        rep[1] = body.len() as u8;
        rep[3..3 + body.len()].copy_from_slice(&body);
        let p = parse_partition(&rep).unwrap();
        assert_eq!(p.start, 0x004E_D000);
        assert_eq!(p.length, 0x0030_2000);
    }
}
