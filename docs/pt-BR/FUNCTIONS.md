# Funções do AKG N9 Hybrid (protocolo RACE)

English version: [../FUNCTIONS.md](../FUNCTIONS.md)

Dispositivo: AKG N9 Hybrid · SoC Airoha AB1585 (família AB158x; via NVKey 0x1002) · firmware **v4.5.0**.
Canais (ambos coleção HID `FF13:0001`, relatórios de 62 bytes):
- **Fone por USB-C:** `0ECB:20D8` coleção COL01 — canal preferido, resposta limpa.
- **Dongle 2.4 GHz:** `0ECB:20D9` interface MI_03 COL02 — fallback (relay para o fone).

Enquadramento: requisição `05 5A <tam LE> <id LE> <payload>`, embrulho HID `06 <tam> 80 <pacote>`;
resposta por `GetInputReport(0x07)`, byte 1 = bytes válidos; frame `05 5B|5D <tam LE> <id LE> <corpo>`.

Classificação: **LEITURA** · **ESCRITA REVERSÍVEL** · **PERIGOSO** (fora do app).

## Verificados em hardware (LEITURA)

| Função | id | Pacote enviado | Resposta / decodificação | Canal |
|---|---|---|---|---|
| Bateria | `0x0CD6` | `05 5A 03 00 D6 0C 00` | `… D6 0C 00 <agente> <pct>` → pct 0–100 | USB ok |
| ANC (status) | `0x0901` | `05 5A 04 00 01 09 05 00` | filtro: `00`=off, `01`=ANC, `09`=TalkThru, `0A`=Ambient Aware; nível 1–7 | USB+dongle ok |
| Versão de firmware | `0x1C07` | `05 5A 03 00 07 1C <00 fone\|01 dongle>` | `… <status> <agente> <tam> <ascii>` → "v4.5.0" | USB ok |
| Model ID | `0x000E` | `05 5A 02 00 0E 00` | byte do MID (deu `00`) | USB ok |
| Ler NVKey | `0x0A00` | `05 5A 06 00 00 0A <nvid LE> <tam máx LE>` | `<tam LE> <dados>` (tam 0 = chave vazia) | USB ok |
| Info de partição | `0x1C00` | `05 5A 03 00 00 1C 00` | partId, tipo, endereço (int32 LE), tamanho (int32 LE). Obtido: início `0x4ED000`, tam `0x302000` (FOTA) | USB ok |

### NVKeys conhecidos (via `0x0A00`)
| NVKey | Conteúdo | Observação |
|---|---|---|
| `0xF202` | Nome padrão: "AKG N9 Hybrid" | leitura |
| `0xF203` | Nome personalizado | vazio (não definido) |
| `0xF204` | `01` | a interpretar |
| `0x1002` | Nome do chip: "AB1585_Headset" | SoC = Airoha AB1585 (AB158x) |

## PERIGOSO (nunca enviar sem aprovação humana, comando a comando)
Flash: sector erase `1796` (`0x0704`), page program `1799` (`0x0707`), set protect `1794` (`0x0702`);
programação de FOTA; factory reset; shutdown; escrita de NVKey `0x0A01` / `2561`.
Estes ficam fora do app. Dump de firmware (leitura) é objetivo separado (Fase 4, RACE Toolkit da ERNW).

## Pendente
- Mapear o filtro do **TalkThru** no ANC (precisa alternar o modo no app oficial).
- Varredura completa de NVKeys do fone (rodando) → preencher recursos escondidos.
- Cruzar os 94 métodos de `FUNCTIONS_full.md` (conferindo cada id no código) para completar esta tabela.

## Revisão de segurança do Claude (sobre a tabela completa em FUNCTIONS_full.md)
Tabela dos 94 comandos gerada por worker e conferida aqui nos pontos de segurança:
- **10 PERIGOSO** (factory reset, shutdown, pareamentos, apagar dispositivos/telemetria/wallpaper):
  ficam FORA do app. Alguns ids desses foram inferidos pelo worker (ex.: vários como 0x1101
  "send key event"); como não serão enviados, o id exato é irrelevante.
- **Atenção — escrita de NVKey (0x0A01 / 2561):** `setMultiAIStatus` e `setAutoStandbyModeList`
  foram marcados "reversível", mas persistem via escrita de NVKey. São reversíveis (ajuste de
  config), porém, pela regra do goal, a camada de escrita do app deve ter uma LISTA BRANCA de
  NVKeys específicos permitidos e pedir aprovação; nunca escrever NVKey por id de comando genérico.
- **EQ:** `setEQPreset` é stub vazio em N9NCControl. O EQ de verdade é `setEQSettings2` /
  `getEQSettings2`. A Fase 3 deve usar esses, não o preset.
- Os controles reversíveis seguros para a Fase 3 (ANC, ambiente, EQ via EQSettings2) continuam
  valendo, sempre testados um a um no hardware com o app oficial aberto.

## NVKeys achados na varredura do fone (383 chaves com conteúdo)
O dump completo de NVKeys (383 chaves) não é publicado: contém identificadores do aparelho (MAC, série). Destaques (tudo leitura, via `0x0A00`):
| NVKey | Conteúdo | Significado |
|---|---|---|
| `0x1002` | `AB1585_Headset` | SoC Airoha AB1585 |
| `0x1003` | `AKG…20D8…` | modelo / id USB |
| `0x1854` | `<omitido: MAC Bluetooth do fone>` | endereço Bluetooth do fone |
| `0x3901` | `AKG N9 Hybrid-LE` | nome BLE |
| `0xF202` | `AKG N9 Hybrid` | nome Bluetooth clássico |
| `0xF508` | `<omitido: número de série>` | número de série |
| `0xE080`–`0xE094` | rampas de bytes | tabelas de EQ / coeficientes de filtro ANC |
| `0x20xx` | valores curtos | flags/ajustes (EQ, ANC, auto-off etc.) a decodificar |

Observação: `0x1854` (endereço BT) e `0xF508` (série) identificam o aparelho; ficam só local.
A maioria das 383 chaves são blocos zerados (não inicializados) — ruído, ignorar.

## Fase 3 — comandos de ESCRITA (VALIDADOS no hardware, reversíveis)
Testados no fone **via dongle** (PID_20D9, COL02), com app oficial aberto, em 18/11/2026.
**O dongle relaya escrita** (não só leitura): cada write recebeu ack e foi confirmado relendo `0x0901`.
Comando ANC: id `0x0E06` (3590). Payloads extraídos do APK (`AncStageAncOff/AncOn.genRacePackets`):
Off = `00 0B 01`; On = `00 0A <filtro> 00 01` (o **`01` final é o enable** — era o que faltava).
| Ação | Pacote RACE (app-exato) | Readback 0x0901 | Status |
|---|---|---|---|
| Desligar ANC | `05 5A 05 00 06 0E 00 0B 01` | filter→`00` | ✅ modo troca (validado; forma curta `…0B` também funcionou) |
| ANC | `05 5A 07 00 06 0E 00 0A 01 00 01` | filter→`01` | ✅ modo troca |
| TalkThru | `05 5A 07 00 06 0E 00 0A 09 00 01` | filter→`09` | ✅ modo troca (usuário confirmou; faltava o enable) |
| Ambiente | `05 5A 07 00 06 0E 00 0A 0A 00 01` | filter→`0A` | ✅ modo troca (usuário confirmou) |
| Nível ambiente (ganho) | `05 5A 06 00 06 0E 00 0F <ganho LE>` | — | ⏳ nível/intensidade, a ligar |
- **Descoberta**: a 1ª tentativa mandava `00 0A <filtro> 00` (sem o `01` final). ANC (01) trocava
  assim mesmo, mas TalkThru/Ambient não — davam ack e o modo não mudava. Com o enable `01` todos
  trocam. O **nível/intensidade** de cada modo é um comando separado (ganho `0x0F` p/ ambiente),
  ainda não ligado no tray.
- `<filtro>` (igual na leitura 0x0901 e na escrita AncOn): 01=ANC, 09=TalkThru, 0A=Ambient; off = sub-op `0B`.
- `setANCSubMode` = "Not Support" neste fone; o modo de escuta vai por `setAncSetting`
  (flow SET_ANC_SETTINGS) → AncStageAncOn (0x0E06).
- EQ: `setEQSettings2` ramifica em `AirohaEQControl` (linha 251): `category==201` → **PersoniFi EQ**
  (id `0x0071` set / `0x0070` get; ganho int32 LE, dB×100, −12..+12 dB — ver `EQ-NOTES.md`).
  O EQ de **bandas** normal usa o flow `SET_EQ_SETTINGS` com `AirohaEQPayload`; o estágio/mRaceId
  dele é mapeado indiretamente e ainda **não foi resolvido**. EQ não implementado no app (polish).
- Protocolo de teste (com você): 1 escrita → ler 0x0901 → conferir no app oficial → desfazer.

## Recursos reversíveis adicionais (payloads verificados no `genRacePackets` do APK)
Confirmados como recursos do N9 via `getAllDashboardCommands`. Ids na whitelist da crate.
| Recurso | id | Payload | Builder (akg-race) | No tray |
|---|---|---|---|---|
| Nível ANC/Ambiente | `0x0E06` | `00 1E <nível LE16>` (sub-op 0x1E) | `apply_anc_level` | ✅ submenu Nível 1–7 |
| Avisos de voz | `0x0900` | `01 00 <onOff>` | `apply_voice_prompt` | ✅ ligar/desligar |
| Áudio espacial 3D | `0x0012` | `<onOff> <tipo>` | `apply_spatial_3d` | — (lib) |
| Balanço L/R | `0x0036` | `<onOff> <nível>` | `apply_lr_balance` | — (lib) |
| Limitador de áudio | `0x0050` | `<onOff>` | `apply_audio_limiter` | — (lib) |
| Chat Optimizer | `0x0052` | `<onOff>` | `apply_chat_optimizer` | — (lib) |

Outros SETs mapeados (ainda não implementados): SilentNow `0x0069` (payload de 10B: status+durações
LE16), MicGainLevel `0x0065`, HiRes `0x0076`, PSAP `0x0078`, SafeSound `0x0015`, 3DSpatial type.
**Fora da whitelist por segurança:** gesto e auto-standby usam `0x0A01` (escrita de NVKey) — exigem
aprovação comando a comando; não entram no app como toggle simples.

## Lote 3 — toggles de fone (payloads verificados)
| Recurso | id | Payload | Builder | No tray |
|---|---|---|---|---|
| Auto-desligamento | `0x2C82` | `01 00 <en> 00 <min LE16>` (MODULE_ID=1) | `apply_auto_power_off` | ✅ submenu (Nunca/5/10/15/30/60) |
| Achar fone (tocar som) | `0x2C01` | `<blink> <ring> <target>` | `apply_find_me` | ✅ botão |
| Speak-to-Chat | `0x0061` | `<onOff> <latency>` | `apply_speak_to_chat` | ✅ ligar/desligar |
| PSAP (realce auditivo) | `0x0078` | `<onOff> <level> <index>` | `apply_psap` | — (lib) |
| Safe Sound | `0x0015` | `<modo>` | `apply_safe_sound` | — (lib) |
- `0x2C82`=RACE_SET_MMI_COMMON_CONFIG, `0x2C01`=RACE_FIND_ME (resolvidos de `RaceId.java`).
- Sound Effect roteia por SoundEffectInfo→categoria de EQ (fica com o EQ de bandas, pendente).

## EQ de bandas — CONCLUSÃO: fora de escopo por segurança
Investigado a fundo (flow `SET_EQ_SETTINGS` → `AirohaPEQControl1568`/`AirohaPeqMgr`):
- O EQ de bandas é **paramétrico** (`AirohaEQPayload`: bandCount, IIR por banda, L/R gain, sampleRate).
- Os coeficientes são **persistidos via escrita de NVKey `0x0A01`** (`PeqStageSaveCoef`,
  `PeqStageSaveUiData`, `PeqStageSaveUiExtData`...). Isso é **exatamente** a operação que o goal
  proíbe sem aprovação humana comando a comando. **Não implementado** — barrado pela regra de segurança.
- `PeqStageSetPeqGrpIdx` (0x0900, payload `00 00 <idx>`) só troca o grupo PEQ ativo (reversível),
  mas depende de grupos já gravados e está acoplado ao fluxo que salva via NVKey; sem valor isolado.
- PersoniFi EQ (0x0071, do worker) é um perfil personalizado à parte (não é o EQ de bandas do app).
Resumo: o único "EQ" seguro seria trocar um preset já existente; edição de bandas exige NVKey-write.

## Lote 4 — mais toggles + submenu Recursos
| Recurso | id | Payload | Builder | No tray |
|---|---|---|---|---|
| Hi-Res audio | `0x0076` | `<tipo>` | `apply_hi_res` | — (lib; valores do tipo a confirmar) |
| Ganho do mic | `0x0065` | `<ganho>` | `apply_mic_gain` | — (lib; faixa a confirmar) |
| Áudio 3D | `0x0012` | `<onOff> <tipo>` | `apply_spatial_3d` | ✅ submenu Recursos |
| Limitador | `0x0050` | `<onOff>` | `apply_audio_limiter` | ✅ submenu Recursos |
| Chat Optimizer | `0x0052` | `<onOff>` | `apply_chat_optimizer` | ✅ submenu Recursos |
- **Barrados (NVKey-write 0x0A01):** gesto, auto-standby, Multi-AI, coeficientes de EQ.
- **Complexos/niche (fora por ora):** SmartSwitch (0x4353, key codes+bitrate), HotWord (frame de 9B).

## Lote 5 — GETs de estado (padrão de leitura, painel)
0x0901 é multiplexado (ANC, VP, nível...) — o módulo no corpo distingue. parse_* filtra por módulo.
| Estado | GET id | Requisição | Corpo da resposta | Parser |
|---|---|---|---|---|
| Avisos de voz | `0x0901` (mód 0x0001) | `05 5A 04 00 01 09 01 00` | `01 00 <status> <onOff>` | `parse_vp_status` → bool |
No tray: linha "Avisos de voz: ligado/desligado" (atualizada a cada 30s). Mesmo padrão serve pros
demais toggles (cada um com seu módulo/GET); implementado 1 como referência.

## Lote 5b — GETs de estado + checkboxes (Recursos)
GETs simples (requisição vazia, on/off no corpo índice 1; ids não multiplexados):
| Estado | GET id | Parser |
|---|---|---|
| Áudio 3D | `0x0013` | `parse_spatial_3d_status` |
| Limitador | `0x0051` | `parse_audio_limiter_status` |
| Chat Optimizer | `0x0053` | `parse_chat_optimizer_status` |
No tray: submenu "Recursos" virou **CheckMenuItem** — o check reflete o estado lido (refresh 30s)
e clicar alterna (envia o novo estado). Avisos de voz têm linha de estado própria (0x0901 mód 01).

## Lote 6 — mais recursos no tray (Hi-Res, Safe Sound, Balanço L/R)
Checkboxes de estado (GET + SET), on/off. Valores finos (nível L/R, tipo Hi-Res) a confirmar no fone.
| Recurso | SET | GET | on/off |
|---|---|---|---|
| Hi-Res | `0x0076` `<tipo>` | `0x0075` | `apply_hi_res(on as u8)` / `parse_hi_res_status` |
| Safe Sound | `0x0015` `<modo>` | `0x0014` | `apply_safe_sound(on as u8)` / `parse_safe_sound_status` |
| Balanço L/R | `0x0036` `<on> <nível>` | `0x0037` | `apply_lr_balance(on, 0)` / `parse_lr_balance_status` |
No tray: submenu Recursos agora tem 6 toggles. PSAP e Ganho do mic seguem só na lib (precisam faixa/índice).

## Perfis de EQ (efeito de som) — REVERSÍVEL (não é NVKey!)
Diferente do EQ de bandas (que grava NVKey 0x0A01 e fica barrado), os **perfis** usam
`MmiStage_SetVoiceEqMode` — comando MMI reversível.
| Ação | id | Payload | Builder |
|---|---|---|---|
| Selecionar perfil | `0x2C82` (mód 0x13) | `13 00 <modo>` | `apply_eq_profile` |
| Ler perfil atual | `0x2C83` (mód 0x13) | `13 00` → modo no índice 4 | `parse_eq_profile` |
Modos: 5=Standard, 6=Treble Boost, 7=Bass Boost. No tray: submenu "Perfil de EQ" (rádio).
