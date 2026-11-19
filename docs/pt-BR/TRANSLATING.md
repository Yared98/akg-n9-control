# Tradução e Internacionalização

[English](../TRANSLATING.md) | **Português**

O app de bandeja do AKG N9 Control suporta múltiplos idiomas e troca dinâmica em tempo de execução. Ao iniciar, o app detecta automaticamente o idioma de exibição do Windows, caindo para o inglês caso uma correspondência exata ou por subtipo primário não esteja disponível.

---

## Duas formas de adicionar ou personalizar um idioma

### 1. Contribuir com uma tradução embutida (via Pull Request)
Crie um arquivo em `crates/akg-tray/locales/<codigo>.lang` (por exemplo, `es.lang` para espanhol ou `de.lang` para alemão).

O script de compilação (`build.rs`) descobre e embute automaticamente todos os arquivos `.lang` dessa pasta durante o build. **Nenhum código Rust precisa ser editado.**

### 2. Tradução local sem compilar (Locales do usuário)
Você pode adicionar ou customizar traduções no seu computador sem precisar compilar o código-fonte:
1. Abra ou crie a pasta `%APPDATA%\akg-n9-control\locales\` (por exemplo, `C:\Users\<Usuário>\AppData\Roaming\akg-n9-control\locales\`).
2. Coloque seu arquivo `<codigo>.lang` nessa pasta (ex.: `es.lang`).
3. Reinicie o `akg-tray.exe`. O novo idioma aparecerá no submenu **Idioma**.

> [!NOTE]
> Se um arquivo em `%APPDATA%\akg-n9-control\locales\` tiver o mesmo código de um idioma embutido (como `pt-BR.lang`), suas chaves serão mescladas sobre a tradução nativa, permitindo sobrescrever frases individuais.

---

## Formato do Arquivo

As traduções são arquivos de texto simples em UTF-8 com sintaxe `chave = valor`:

```lang
# Nome do idioma exibido no menu de Idioma (OBRIGATÓRIO)
lang.name = Español

# Entradas do menu e de status
menu.quit = Salir
status.battery = Batería: {value}
```

Regras:
- **`lang.name` é obrigatório**: Define o nome nativo exibido no submenu de Idioma (ex.: `Português (Brasil)`, `English`, `Español`).
- **Nome do arquivo**: Use códigos de idioma no padrão BCP 47 (ex.: `es.lang`, `de.lang`, `fr.lang`, `pt-BR.lang`, `zh-CN.lang`).
- **Comentários e espaçamento**: Linhas que começam com `#` e linhas em branco são ignoradas. Espaços antes e depois de chaves e valores são removidos.
- **Placeholders**: Textos entre chaves como `{value}`, `{n}`, `{percent}`, `{battery}`, `{anc}` ou `{mode}` são substituídos em tempo de execução. **Mantenha os nomes dos placeholders idênticos** aos de `en.lang`.
- **Quebras de linha**: Use `\n` caso seja necessário representar uma quebra de linha.

---

## Comportamento de Fallback

O arquivo `crates/akg-tray/locales/en.lang` é a referência oficial para todas as chaves.

Se uma chave estiver ausente em um arquivo de tradução:
1. O app recorre primeiro ao texto em inglês de `en.lang`.
2. Se a chave não existir nem em `en.lang`, exibe o próprio nome da chave.

Uma tradução incompleta continuará funcionando normalmente, sem erros ou falhas.

---

## Como testar traduções

Para validar a integridade dos arquivos de tradução, execute a suíte de testes automatizados:

```bash
cargo test -p akg-tray
```

Os testes verificam se:
- `locales/en.lang` existe e possui `lang.name = English`.
- Todo arquivo `.lang` define um `lang.name` válido.
- Nenhum arquivo possui chaves desconhecidas ausentes de `en.lang`.
- Os placeholders de cada chave são idênticos aos de `en.lang`.
- `pt-BR.lang` contém todas as chaves presentes em `en.lang`.
