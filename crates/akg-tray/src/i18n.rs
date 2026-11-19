//! Minimal internationalisation for the tray app, with no extra dependency.
//!
//! * Translations are plain text files, one `key = value` per line (`#` starts a comment).
//!   Values may use `{name}` placeholders and `\n` for a line break. See `docs/TRANSLATING.md`.
//! * Built-in languages live in `locales/*.lang` and are embedded at build time (see `build.rs`).
//! * Users can add or override a language **without recompiling** by dropping `<code>.lang` into
//!   `%APPDATA%\akg-n9-control\locales\`. The file name is the language code (`es`, `pt-BR`, ...).
//!   A user file is merged on top of a built-in one with the same code.
//! * A missing key falls back to English (`en`), and then to the key itself, so an incomplete
//!   translation still works.
//! * The chosen language is stored in `%APPDATA%\akg-n9-control\settings.ini`; with no saved choice
//!   the Windows display language is used when we have it, otherwise English.

use std::collections::HashMap;
use std::path::PathBuf;

include!(concat!(env!("OUT_DIR"), "/locales_gen.rs"));

/// Language used for any key a translation does not define.
pub const FALLBACK: &str = "en";

/// Parses the text of a `.lang` file into a key -> value map.
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if !key.is_empty() {
                map.insert(key.to_string(), value.trim().replace("\\n", "\n"));
            }
        }
    }
    map
}

/// Replaces every `{name}` in `template` with the matching value from `args`.
pub fn format(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// The `{placeholder}` names used in a string, sorted and de-duplicated (used by the tests).
#[cfg(test)]
pub fn placeholders(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        match rest[start + 1..].find('}') {
            Some(end) => {
                found.push(rest[start + 1..start + 1 + end].to_string());
                rest = &rest[start + 1 + end + 1..];
            }
            None => break,
        }
    }
    found.sort();
    found.dedup();
    found
}

fn user_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("akg-n9-control"))
}

/// Folder where users can drop extra `.lang` files.
pub fn user_locales_dir() -> Option<PathBuf> {
    user_dir().map(|d| d.join("locales"))
}

fn settings_path() -> Option<PathBuf> {
    user_dir().map(|d| d.join("settings.ini"))
}

/// Every language we can load: the embedded ones, with user files merged on top / added.
fn load_all() -> Vec<(String, HashMap<String, String>)> {
    let mut all: Vec<(String, HashMap<String, String>)> =
        BUILTIN.iter().map(|(code, text)| (code.to_string(), parse(text))).collect();
    if let Some(dir) = user_locales_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |x| x == "lang") {
                    let code = path.file_stem().and_then(|s| s.to_str()).map(str::to_string);
                    if let (Some(code), Ok(text)) = (code, std::fs::read_to_string(&path)) {
                        let map = parse(&text);
                        match all.iter_mut().find(|(c, _)| c.eq_ignore_ascii_case(&code)) {
                            Some(slot) => slot.1.extend(map),
                            None => all.push((code, map)),
                        }
                    }
                }
            }
        }
    }
    all
}

/// A language offered in the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LangInfo {
    pub code: String,
    /// Native name, from the file's `lang.name` key (the code when it is missing).
    pub name: String,
}

/// All available languages (embedded + user files), in a stable order.
pub fn available() -> Vec<LangInfo> {
    load_all()
        .into_iter()
        .map(|(code, map)| {
            let name = map.get("lang.name").cloned().unwrap_or_else(|| code.clone());
            LangInfo { code, name }
        })
        .collect()
}

/// Best match for `wanted` among `codes`: exact (case-insensitive), then the same primary subtag
/// (`pt-PT` -> `pt-BR`), then English.
pub fn pick(codes: &[String], wanted: &str) -> String {
    if let Some(c) = codes.iter().find(|c| c.eq_ignore_ascii_case(wanted)) {
        return c.clone();
    }
    let primary = |s: &str| s.split(|ch| ch == '-' || ch == '_').next().unwrap_or("").to_ascii_lowercase();
    let wanted_primary = primary(wanted);
    if !wanted_primary.is_empty() {
        if let Some(c) = codes.iter().find(|c| primary(c) == wanted_primary) {
            return c.clone();
        }
    }
    FALLBACK.to_string()
}

/// The Windows display language (e.g. `pt-BR`), or English when it cannot be read.
#[cfg(windows)]
pub fn system_lang() -> String {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(buf: *mut u16, cch: i32) -> i32;
    }
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if n > 1 {
        String::from_utf16_lossy(&buf[..(n as usize - 1)])
    } else {
        FALLBACK.to_string()
    }
}

#[cfg(not(windows))]
pub fn system_lang() -> String {
    FALLBACK.to_string()
}

/// The language saved by the user, if any.
pub fn saved_lang() -> Option<String> {
    let text = std::fs::read_to_string(settings_path()?).ok()?;
    parse(&text).get("language").cloned().filter(|s| !s.is_empty())
}

/// Saves the chosen language. Failures are ignored: the choice just will not survive a restart.
pub fn save_lang(code: &str) {
    if let Some(path) = settings_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, format!("# akg-n9-control settings\nlanguage = {code}\n"));
    }
}

/// The active language: look strings up with [`I18n::t`] / [`I18n::tf`].
pub struct I18n {
    code: String,
    fallback: HashMap<String, String>,
    current: HashMap<String, String>,
}

impl I18n {
    /// Loads the best available match for `wanted`.
    pub fn new(wanted: &str) -> Self {
        let all = load_all();
        let codes: Vec<String> = all.iter().map(|(c, _)| c.clone()).collect();
        let code = pick(&codes, wanted);
        let find = |c: &str| {
            all.iter().find(|(k, _)| k.eq_ignore_ascii_case(c)).map(|(_, m)| m.clone()).unwrap_or_default()
        };
        Self { current: find(&code), fallback: find(FALLBACK), code }
    }

    /// At startup: the saved language, otherwise the Windows display language.
    pub fn startup() -> Self {
        Self::new(&saved_lang().unwrap_or_else(system_lang))
    }

    /// Builds a language from maps directly (used by the tests).
    #[cfg(test)]
    pub fn from_maps(code: &str, current: HashMap<String, String>, fallback: HashMap<String, String>) -> Self {
        Self { code: code.to_string(), current, fallback }
    }

    /// Code of the active language (`en`, `pt-BR`, ...).
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Text for `key`: the active language, then English, then the key itself.
    pub fn t(&self, key: &str) -> String {
        self.current
            .get(key)
            .or_else(|| self.fallback.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// Like [`I18n::t`], replacing `{name}` placeholders from `args`.
    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        format(&self.t(key), args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn parse_handles_comments_bom_equals_and_newline() {
        let m = parse("\u{feff}# comment\n\n a.b = Hello = world \nline = one\\ntwo\n =ignored\nnovalue\n");
        assert_eq!(m.get("a.b").map(String::as_str), Some("Hello = world"));
        assert_eq!(m.get("line").map(String::as_str), Some("one\ntwo"));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn format_replaces_placeholders() {
        assert_eq!(format("Level {n} of {max}", &[("n", "3"), ("max", "7")]), "Level 3 of 7");
        assert_eq!(format("No placeholder", &[("n", "3")]), "No placeholder");
    }

    #[test]
    fn placeholders_are_listed() {
        assert_eq!(placeholders("{b} and {a} and {b}"), vec!["a".to_string(), "b".to_string()]);
        assert!(placeholders("none").is_empty());
    }

    #[test]
    fn pick_prefers_exact_then_primary_then_english() {
        let codes = vec!["en".to_string(), "pt-BR".to_string(), "es".to_string()];
        assert_eq!(pick(&codes, "pt-br"), "pt-BR");
        assert_eq!(pick(&codes, "pt-PT"), "pt-BR");
        assert_eq!(pick(&codes, "es_MX"), "es");
        assert_eq!(pick(&codes, "ja-JP"), "en");
        assert_eq!(pick(&codes, ""), "en");
    }

    #[test]
    fn lookup_falls_back_to_english_then_key() {
        let i = I18n::from_maps("xx", map(&[("a", "AA-xx")]), map(&[("a", "AA-en"), ("b", "BB-en")]));
        assert_eq!(i.t("a"), "AA-xx");
        assert_eq!(i.t("b"), "BB-en");
        assert_eq!(i.t("missing"), "missing");
        assert_eq!(i.tf("a", &[]), "AA-xx");
    }

    // ---- consistency of the embedded translations ----

    fn builtin_maps() -> Vec<(String, HashMap<String, String>)> {
        BUILTIN.iter().map(|(c, t)| (c.to_string(), parse(t))).collect()
    }

    #[test]
    fn english_exists_and_every_language_has_a_name() {
        let all = builtin_maps();
        assert!(all.iter().any(|(c, _)| c == FALLBACK), "locales/en.lang is required");
        for (code, m) in &all {
            assert!(m.contains_key("lang.name"), "{code}.lang is missing `lang.name`");
        }
    }

    #[test]
    fn translations_have_no_unknown_keys_and_same_placeholders() {
        let all = builtin_maps();
        let en = &all.iter().find(|(c, _)| c == FALLBACK).expect("en").1;
        for (code, m) in all.iter().filter(|(c, _)| c != FALLBACK) {
            for (key, value) in m {
                let reference = en.get(key).unwrap_or_else(|| panic!("{code}.lang: unknown key `{key}` (not in en.lang)"));
                assert_eq!(
                    placeholders(value),
                    placeholders(reference),
                    "{code}.lang: placeholders of `{key}` differ from en.lang"
                );
            }
        }
    }

    #[test]
    fn pt_br_is_complete() {
        let all = builtin_maps();
        let en = &all.iter().find(|(c, _)| c == FALLBACK).expect("en").1;
        let pt = &all.iter().find(|(c, _)| c == "pt-BR").expect("pt-BR").1;
        let missing: Vec<_> = en.keys().filter(|k| !pt.contains_key(*k)).collect();
        assert!(missing.is_empty(), "pt-BR.lang is missing keys: {missing:?}");
    }
}
