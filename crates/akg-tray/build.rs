//! Discovers the translation files in `locales/*.lang` and embeds them in the binary.
//!
//! Adding a built-in translation is just dropping a new `locales/<code>.lang` file here
//! (e.g. `es.lang`, `de.lang`, `pt-BR.lang`); no Rust code has to be edited.
use std::{env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let dir = Path::new(&manifest).join("locales");
    println!("cargo:rerun-if-changed=locales");

    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("locales/ directory")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |x| x == "lang"))
        .collect();
    files.sort();

    let mut out = String::from("/// (language code, file text) of every embedded `locales/*.lang`.\n");
    out.push_str("pub const BUILTIN: &[(&str, &str)] = &[\n");
    for p in files {
        let code = p.file_stem().expect("file stem").to_string_lossy().to_string();
        out.push_str(&format!("    ({:?}, include_str!({:?})),\n", code, p.to_string_lossy()));
    }
    out.push_str("];\n");

    let target = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("locales_gen.rs");
    fs::write(target, out).expect("write locales_gen.rs");
}
