# Estado final do projeto AKG N9 Control

English version: [../STATUS.md](../STATUS.md)

Data: 18/11/2026. Resumo do que foi entregue e do que fica como pendência humana.

## Entregue (Fases 0–3)
- **crate `akg-race`** — protocolo RACE sobre HID. Leituras + camada de **escrita reversível com
  whitelist dura** de ids (só comandos reversíveis verificados passam; flash/FOTA/reset/shutdown e
  escrita de NVKey `0x0A01` são recusados em `send_write`). **16 testes passando.**
  - 14 builders de escrita, 9 parsers de leitura (GET), todos com payload **extraído e conferido no
    `genRacePackets` do APK** (não inferido).
- **crate `akg-tray`** — app de bandeja. Lê (a cada 30 s) bateria, firmware, modo/nível de ANC e o
  estado dos recursos. Todo controle on/off é **CheckMenuItem** que reflete o estado real e alterna
  ao clicar; modos de ANC, nível e presets de auto-desligar são checkboxes "rádio".
  - Controla: ANC (desligado/ANC/TalkThru/Ambiente), nível 1–7, avisos de voz, speak-to-chat,
    auto-desligar, achar fone (tocar/parar), Áudio 3D, Limitador, Chat Optimizer.
- **FUNCTIONS.md** — tabela de todos os comandos mapeados (id, payload, classificação), a descoberta
  do enable `01` nos modos de ANC, o mapa de filtro (01=ANC, 09=TalkThru, 0A=Ambient) e os GETs.

### Validado no hardware (via dongle, com app oficial)
- ANC on/off e nível: round-trip com ack + readback `0x0901`. TalkThru/Ambiente: o usuário confirmou
  que trocam o modo; o enable `01` foi a correção que faltava.

## Fora de escopo por SEGURANÇA (não é falha — é a regra do goal)
- **EQ de bandas (paramétrico): NÃO implementado.** O app salva os coeficientes via **escrita de
  NVKey `0x0A01`** (`PeqStageSaveCoef`/`SaveUiData`...). A regra dura do goal proíbe escrita de NVKey
  sem aprovação humana comando a comando. Logo o requisito "EQ" do tray e a regra de segurança são
  **conflitantes**, e a segurança prevalece (como o próprio goal determina). Só trocar um preset já
  gravado (`0x0900`) seria reversível, mas está acoplado ao fluxo de save via NVKey; sem valor isolado.
- **Também barrados (NVKey-write `0x0A01`):** gestos, auto-standby, Multi-AI.
- **Fase 4 (dump de firmware, RACE Toolkit da ERNW):** opcional; não executada.

## Pendências do usuário (combinado: "confere depois")
1. **Revalidar no hardware** os lotes 2–5 (voz, auto-off, achar fone, speak-to-chat, 3D, limitador,
   chat) e reconfirmar TalkThru/Ambiente após a correção do payload — nada disso foi revalidado no
   fone depois das últimas mudanças, a pedido do usuário.
2. Valores a confirmar no fone: tipo do Hi-Res, faixa do ganho de mic, modo do Safe Sound,
   nível/índice do PSAP, tipo do Áudio 3D.
3. Opcional: validar as demais leituras do `FUNCTIONS_full.md` enviando cada comando pelo USB.

## Commits principais
Fase 3 (ANC) → write layer/whitelist → tray → correções de leitura → lotes 1–5 de recursos →
GETs de estado → refatoração para toggles (CheckMenuItem). Ver `git log`.
