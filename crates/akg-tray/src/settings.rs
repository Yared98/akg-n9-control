//! Settings storage in `%APPDATA%\akg-n9-control\settings.ini`.
//!
//! Stores key-value configuration such as chosen language and application toggles.
//! Unknown keys and comment headers (`#`) are preserved across writes.

use std::path::{Path, PathBuf};

/// Returns the user directory `%APPDATA%\akg-n9-control`.
pub fn user_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("akg-n9-control"))
}

/// Returns the path to `settings.ini`.
pub fn settings_path() -> Option<PathBuf> {
    user_dir().map(|d| d.join("settings.ini"))
}

/// Updates the INI file content in memory, replacing or adding `key = value`
/// while preserving comments and existing unknown keys.
pub fn update_ini_content(content: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut out = String::new();
    let has_content = !content.trim().is_empty();
    if !has_content {
        return format!("# akg-n9-control settings\n{key} = {value}\n");
    }
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if let Some((k, _)) = trimmed.split_once('=') {
            if k.trim() == key {
                out.push_str(&format!("{key} = {value}\n"));
                found = true;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if !found {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("{key} = {value}\n"));
    }
    out
}

/// Reads a key from an explicit path.
pub fn get_from_path(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    crate::i18n::parse(&text).get(key).cloned().filter(|s| !s.is_empty())
}

/// Reads a boolean setting from an explicit path, or returns `default`.
pub fn get_bool_from_path(path: &Path, key: &str, default: bool) -> bool {
    match get_from_path(path, key) {
        Some(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => default,
        },
        None => default,
    }
}

/// Writes a key-value pair at an explicit path, preserving other content.
pub fn set_at_path(path: &Path, key: &str, value: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let updated = update_ini_content(&content, key, value);
    let _ = std::fs::write(path, updated);
}

/// Gets a setting from `%APPDATA%\akg-n9-control\settings.ini`.
pub fn get(key: &str) -> Option<String> {
    settings_path().and_then(|p| get_from_path(&p, key))
}

/// Gets a boolean setting from `%APPDATA%\akg-n9-control\settings.ini`.
pub fn get_bool(key: &str, default: bool) -> bool {
    settings_path().map_or(default, |p| get_bool_from_path(&p, key, default))
}

/// Sets a key-value setting in `%APPDATA%\akg-n9-control\settings.ini`.
pub fn set(key: &str, value: &str) {
    if let Some(path) = settings_path() {
        set_at_path(&path, key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_ini_preserves_comments_and_other_keys() {
        let original = "# header comment\n# another line\nkey1 = value1\ncustom_unknown = keep_me\n";
        let updated = update_ini_content(original, "key1", "new_val");
        assert!(updated.contains("# header comment\n"));
        assert!(updated.contains("custom_unknown = keep_me\n"));
        assert!(updated.contains("key1 = new_val\n"));
        assert!(!updated.contains("key1 = value1\n"));
    }

    #[test]
    fn test_update_ini_adds_new_key_if_missing() {
        let original = "# header\nold = val\n";
        let updated = update_ini_content(original, "new_key", "123");
        assert!(updated.contains("# header\nold = val\n"));
        assert!(updated.contains("new_key = 123\n"));
    }

    #[test]
    fn test_path_roundtrip_and_bool_parsing() {
        let tmp_dir = std::env::temp_dir().join("akg_test_settings");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let test_file = tmp_dir.join("test_settings.ini");
        let _ = std::fs::remove_file(&test_file);

        // Initially missing: default should be returned
        assert_eq!(get_bool_from_path(&test_file, "notify_connection", true), true);
        assert_eq!(get_bool_from_path(&test_file, "hotkeys", false), false);

        // Set values
        set_at_path(&test_file, "notify_connection", "false");
        set_at_path(&test_file, "language", "pt-BR");
        set_at_path(&test_file, "hotkeys", "true");

        // Verify values
        assert_eq!(get_from_path(&test_file, "language"), Some("pt-BR".to_string()));
        assert_eq!(get_bool_from_path(&test_file, "notify_connection", true), false);
        assert_eq!(get_bool_from_path(&test_file, "hotkeys", false), true);

        // Overwrite one key, verify others preserved
        set_at_path(&test_file, "language", "en");
        assert_eq!(get_from_path(&test_file, "language"), Some("en".to_string()));
        assert_eq!(get_bool_from_path(&test_file, "notify_connection", true), false);
        assert_eq!(get_bool_from_path(&test_file, "hotkeys", false), true);

        let _ = std::fs::remove_file(&test_file);
        let _ = std::fs::remove_dir(&tmp_dir);
    }
}
