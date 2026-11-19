# EQ do AKG N9 — notas de engenharia reversa (para implementar)

English version: [../EQ-NOTES.md](../EQ-NOTES.md)

Usuário aprovou escrita de NVKey para EQ (18/11/2026). Objetivo: selecionar presets e criar perfis
(editar curva), como o app oficial.

## Caminho no app (Airoha PEQ, chip AB1585)
- `setEQSettings2(hmDevice, EQSettings2, isNotify)` → `AirohaEQControl.setEQSettings2`:
  - `category == 201` → PersoniFi EQ (recurso à parte).
  - `category == 0` (STUDIO/flat) → `AirohaPeqMgr.setPeqGroupIdx((byte)0)` — **reversível, sem NVKey**.
  - demais (com payload) → `UpdateAndSavePeqData(...)` → estágios `PeqStageSaveUiData` /
    `PeqStageSaveCoef` → **escrita de NVKey** (`genWriteNvKeyPacket`).
- NVKey do UI data: `0xEF00 + index` (`setPeqUiDataNvKey`: i + 61184).
- NVKey dos coeficientes: `AirohaPeqMgr.NVKEY_AUDIO_COEF_BASE + AUDIO_CUSTOM_COEF_OFFSET + i`.
- Payload do UI data (`getSavePeqUiDataPayload`) é montado a partir de `PeqUiDataStru` /
  `PeqBandInfo[10]` (freq, Q = freq/Qvalue, ganho, enable) + masterGain. Os **coeficientes** vêm da
  conversão IIR (biquad) de cada banda — matemática a reproduzir (libpeq).

## Presets (registros guardados no fone)
`EnumEqPresetIdx`: JAZZ, VOCAL, BASS, …, CLUB, STUDIO. São registros de EQ; o app lê/aplica via
`getEQSettings2(category)` / `getCustomIndex` / `getEqRecordSize`. Bandas: 32,64,125,250,500,1k,2k,4k,8k,16k.

## Plano de implementação (fases)
1. **Selecionar preset** — confirmar o comando exato de "aplicar registro N" (grupo/índice ou
   setEQSettings2 do preset). Expor no tray um submenu com os presets. [tratável]
2. **Perfil custom (curva)** — maior esforço:
   a. Reproduzir a conversão ganho(dB)→coeficiente IIR por banda (biquad) da libpeq.
   b. Montar `getSavePeqUiDataPayload` e os pacotes de save (UI data + coef) exatamente.
   c. Escrever nas NVKeys (`0xEF00+i`, coef base+offset+i) — com a whitelist liberando esses ids
      SÓ sob este fluxo de EQ (não abrir 0x0A01 geral).
   d. UX: como não cabe slider em menu de bandeja, definir as bandas por um **arquivo de config**
      (ex.: ganhos por banda) que o app lê e aplica — ou uma janela separada (esforço maior).

## Segurança
A whitelist atual barra 0x0A01 (NVKey). Para o EQ, liberar APENAS as NVKeys de EQ (0xEF00+i e as de
coef), idealmente por uma função dedicada `apply_eq_*`, mantendo gestos/standby/factory fora.

## ATUALIZAÇÃO — Fase 1 é REVERSÍVEL (setPeqGroupIdx)
- **Aplicar preset = `PeqStageSetPeqGrpIdx`** → id `0x0900`, payload `00 00 <index>` (módulo 0x0000).
  Reversível, NÃO grava NVKey. (id 0x0900 já está na whitelist — é o mesmo do voice prompt, módulo distingue.)
- Lista de presets = NVKey "audio path" (`NVKEY_AUDIO_PATH` = 0xF233), lido por `PeqStageReadAudiPath`
  + `PeqStageReadPeqSubset`. Formato: `[count U16][registros...]`; cada registro tem o coef NVID em +4/+5;
  isCustom se o NVID cai na faixa custom (`NVKEY_AUDIO_COEF_BASE + AUDIO_CUSTOM_COEF_OFFSET + 0..3`),
  catID = offset+101; built-in catID = posição+1. (Parse do audio path = refinamento p/ scan dinâmico com nomes.)
- **Fase 1 MVP (reversível):** submenu "Preset de EQ" aplicando `setPeqGroupIdx(index)` por índice.
- **Fase 2 (NVKey, aprovado):** criar/editar curva = coeficientes IIR + save via NVKey (0xEF00+i, coef base+offset).

## CONCLUSÃO DEFINITIVA (revisão da extração) — EQ de música é bloqueado por lib nativa
Rastreado de cima pra baixo (DashboardActivity → CmdCtrl → AirohaPEQControl1568 → AirohaPeqMgr):
- Um preset de música = conjunto de **ganhos por banda** (10 bandas). Aplicar = `setEQSettings2(category, gains)`.
  `category==0` = EQ off (flat). NÃO existe comando "selecionar preset" — todo preset envia os ganhos.
- O fone precisa dos **coeficientes IIR**, que o app calcula via **biblioteca NATIVA** `NativePeq`:
  `setPeqPoint(freq,Q,gain)` → `generateCofe()` → `changeRescaleCofe(masterGain)` → `getCofeParam()` (short[]).
  Esses coeficientes vão em `CoefParamStruct` e são enviados (RealTimeUpdate) + salvos via NVKey.
- **Impedimento:** reproduzir fielmente os coeficientes exige a `.so` nativa (formato ponto-fixo + rescale
  da Airoha). Não é permissão (NVKey já aprovado) nem protocolo — é o cálculo nativo, impraticável de
  reimplementar sem a lib. `setPeqGroupIdx` sozinho só troca o grupo ativo (no N9 cai no EQ de voz/chamada),
  sem carregar coeficientes.
- **Opções (todas ruins):** (a) reimplementar biquad peaking-EQ e torcer pra bater o formato (arriscado,
  pode soar errado); (b) extrair/rodar a .so (inviável no Windows/Rust); (c) deixar o EQ de música pro app
  oficial. Recomendação: (c) — é o único recurso que não dá pra reimplementar limpo; o resto do app funciona.
