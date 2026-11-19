# akg-n9-control

[English](README.md) | **Português**

Controle do **AKG N9 Hybrid** no PC (Windows), em Rust, via protocolo **Airoha RACE** sobre USB-HID,
pelo dongle 2.4 GHz (`0ECB:20D9`) ou pela porta USB-C do fone (`0ECB:20D8`). Um app de bandeja lê o
estado e controla os recursos reversíveis do fone, sem depender do app oficial.

*Não oficial: sem relação com a AKG, Harman, Samsung ou Airoha.*

## Crates
- `crates/akg-race`: biblioteca do protocolo RACE sobre HID. Leituras, parsers e uma **camada de
  escrita com whitelist de ids** (só comandos reversíveis verificados). Testes sem hardware (`cargo test`).
- `crates/akg-tray`: app de bandeja. Painel com o estado do fone e os controles como toggles.

## O que o app faz
- **Lê** (a cada 30 s): bateria, firmware, modo/nível de ANC e o estado dos recursos. O ícone muda de
  cor com a bateria e há um aviso (toast) abaixo de 15%.
- **Controla** (toggles que refletem o estado real): ANC desligado / ANC / TalkThru / Ambiente e nível,
  Som espacial (Filme/Música/Jogo), avisos de voz, Speak-to-Chat, VoiceAware (e nível), pausar ao tirar
  o fone, auto-desligar, limitador, Chat Optimizer, Hi-Res, Safe Sound, EQ de volume baixo, balanço L/R,
  achar fone (toca um som; **para ao apertar o play do fone**) e desligar o fone.
- **Diagnóstico**: série, chip e firmware (leitura).

## Build, teste e execução
    cargo test                 # protocolo + parsers (sem precisar do fone)
    cargo build --release      # binário em target/release/akg-tray.exe
    cargo run -p akg-tray      # abre o ícone na bandeja

Requer o fone conectado (dongle ou USB-C). Botão direito no ícone abre o menu.

## Segurança
Os comandos de escrita foram extraídos e conferidos no código do app oficial (não inferidos) e testados
em um AKG N9 Hybrid (firmware 4.5.0) pelo dongle. `send_write` recusa qualquer id fora de
`ALLOWED_WRITE_IDS`. Ficam **fora do app**: escrita/apagamento de flash, FOTA, factory reset e escrita de
NVKey (`0x0A01`), que é como o fone guarda gestos, auto-standby, Multi-AI e os coeficientes do EQ. O EQ de
música também não é suportado (os coeficientes vêm de uma biblioteca nativa do app oficial); veja
[docs/EQ-NOTES.md](docs/EQ-NOTES.md). **Use por sua conta e risco**: escrever configurações em um
dispositivo pode mudar o comportamento dele.

## Documentação
- [docs/FUNCTIONS.md](docs/FUNCTIONS.md): comandos mapeados e verificados (id, payload, classificação).
- [docs/FUNCTIONS_full.md](docs/FUNCTIONS_full.md): índice dos 94 métodos do app (**não verificado**).
- [docs/EQ-NOTES.md](docs/EQ-NOTES.md): por que o EQ de música não é suportado.
- [docs/STATUS.md](docs/STATUS.md): estado do projeto.

Licença: MIT.
