//! Compile the power table in: every `assets/powers/*.ron` (a plain RON list each), in FILENAME
//! order (byte order of the name — identical on every peer, so power ids are deterministic).
//!
//! Generates `$OUT_DIR/powers_files.rs`:
//! * `SHIPPED_FILES: &[(&str, &str)]` — `(file name, contents)` per file (parse errors name the file);
//! * `SHIPPED_RON: &str` — every file's entries merged into ONE list (tests / tools that want a
//!   single text).
//!
//! Never fails the build over content: a file whose outer `[ ... ]` can't be found is left out of
//! the merged text with a cargo warning (it still appears in `SHIPPED_FILES`, where
//! `PowerTable::parse_files` reports it by name). Content edits re-run this script
//! (`rerun-if-changed` on the directory, which cargo scans recursively).

use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let dir = manifest.join("../../assets/powers");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "ron")).collect())
        .unwrap_or_default();
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    let mut list = String::from("pub const SHIPPED_FILES: &[(&str, &str)] = &[\n");
    let mut merged = String::from("[\n");
    for p in &files {
        println!("cargo:rerun-if-changed={}", p.display());
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        list.push_str(&format!("    ({name:?}, include_str!({:?})),\n", path_str(&abs)));
        let src = std::fs::read_to_string(p).unwrap_or_default();
        match list_body(&src) {
            // Entries end with `,` already, or get one on its own line (a trailing comment can't
            // swallow it); an empty list adds nothing.
            Some(body) => match last_significant(body) {
                Some(b',') => merged.push_str(&format!("// ---- {name}\n{body}\n")),
                Some(_) => merged.push_str(&format!("// ---- {name}\n{body}\n,\n")),
                None => {}
            },
            None => println!("cargo:warning=assets/powers/{name}: no top-level [ ... ] list; left out of SHIPPED_RON"),
        }
    }
    merged.push_str("]\n");
    list.push_str("];\n");
    std::fs::write(out_dir.join("powers_merged.ron"), merged).expect("write powers_merged.ron");
    list.push_str("pub const SHIPPED_RON: &str = include_str!(concat!(env!(\"OUT_DIR\"), \"/powers_merged.ron\"));\n");
    std::fs::write(out_dir.join("powers_files.rs"), list).expect("write powers_files.rs");
}

fn path_str(p: &Path) -> String {
    // `\\?\` verbatim prefixes (canonicalize on Windows) are fine for include_str!, but keep it plain.
    let s = p.to_string_lossy().into_owned();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

/// The last byte of `s` that isn't whitespace or inside a comment (strings count as significant).
fn last_significant(s: &str) -> Option<u8> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut last = None;
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                last = Some(b'"');
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            c if c.is_ascii_whitespace() => {}
            c => last = Some(c),
        }
        i += 1;
    }
    last
}

/// The text between a file's outermost `[` and its matching `]` (strings and comments skipped).
fn list_body(src: &str) -> Option<&str> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut depth = 0i32;
    let mut open = None;
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            b'[' | b'(' | b'{' => {
                if depth == 0 {
                    if b[i] != b'[' {
                        return None;
                    }
                    open = Some(i);
                }
                depth += 1;
            }
            b']' | b')' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return open.map(|o| &src[o + 1..i]);
                }
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
