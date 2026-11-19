# Translating and Localisation

**English** | [Português](pt-BR/TRANSLATING.md)

The AKG N9 Control tray app supports runtime language switching and multiple languages. By default, it detects your Windows display language on startup, falling back to English if an exact or primary subtag match is unavailable.

---

## Two ways to add or customize a language

### 1. Contributing a built-in translation (via Pull Request)
Add a new translation file at `crates/akg-tray/locales/<code>.lang` (for example, `es.lang` for Spanish or `de.lang` for German).

The build script (`build.rs`) automatically discovers and embeds all `.lang` files in that folder during compilation. **No Rust code needs to be modified.**

### 2. Local translation without compiling (User locales)
You can add or customize translations on your machine without compiling from source:
1. Open or create the folder `%APPDATA%\akg-n9-control\locales\` (for example, `C:\Users\<User>\AppData\Roaming\akg-n9-control\locales\`).
2. Drop your `<code>.lang` file into that folder (e.g. `es.lang`).
3. Restart `akg-tray.exe`. The language will appear in the **Language** menu.

> [!NOTE]
> If a file in `%APPDATA%\akg-n9-control\locales\` shares the same language code as a built-in language (e.g. `pt-BR.lang`), its keys will be merged on top of the built-in translation, allowing you to override individual phrases.

---

## File Format

Translations are UTF-8 plain text files following a simple `key = value` syntax:

```lang
# Language name displayed in the Language menu (MANDATORY)
lang.name = Español

# Menu and status entries
menu.quit = Salir
status.battery = Batería: {value}
```

Rules:
- **`lang.name` is required**: This defines the native name displayed in the Language submenu (e.g. `Español`, `Deutsch`, `Français`).
- **File naming**: Name the file using standard BCP 47 language codes (e.g. `es.lang`, `de.lang`, `fr.lang`, `pt-BR.lang`, `zh-CN.lang`).
- **Comments and spacing**: Lines starting with `#` and blank lines are ignored. Leading and trailing whitespace around keys and values is trimmed.
- **Placeholders**: Text enclosed in braces like `{value}`, `{n}`, `{percent}`, `{battery}`, `{anc}`, or `{mode}` are placeholders replaced at runtime. **Preserve placeholder names exactly** as they appear in `en.lang`.
- **Line breaks**: Use `\n` to represent a line break if needed.

---

## Fallback Behavior

`crates/akg-tray/locales/en.lang` serves as the primary reference for all translation keys.

If a key is missing from a translation:
1. The app first falls back to the English value from `en.lang`.
2. If the key is not in `en.lang`, it displays the raw key name.

An incomplete translation will continue to function safely without errors or crashes.

---

## Testing Translations

To verify that your translation is well-formed, run the automated test suite:

```bash
cargo test -p akg-tray
```

The tests verify that:
- `locales/en.lang` exists and has `lang.name = English`.
- Every `.lang` file defines a non-empty `lang.name`.
- No translation file contains unrecognized keys that do not exist in `en.lang`.
- All placeholders in each key match the placeholders in `en.lang` exactly.
- `pt-BR.lang` has all keys present in `en.lang`.
