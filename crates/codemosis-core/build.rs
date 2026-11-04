//! Embeds the packs in `/packs` into the binary so `codemosis` works with no
//! files beside it.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn toml_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let packs = manifest_dir.join("../../packs");
    println!("cargo:rerun-if-changed={}", packs.display());

    let mut dirs: Vec<PathBuf> = fs::read_dir(&packs)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.join("pack.toml").is_file())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();

    let mut out = String::from("pub static BUILTIN_PACKS: &[BuiltinPack] = &[\n");
    for dir in dirs {
        let dir = dir.canonicalize().unwrap();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let _ = writeln!(
            out,
            "    BuiltinPack {{ dir: {name:?}, manifest: include_str!({:?}),",
            dir.join("pack.toml")
        );
        for (field, sub) in [("rules", "rules"), ("tables", "tables")] {
            let _ = write!(out, "        {field}: &[");
            for f in toml_files(&dir.join(sub)) {
                let rel = format!("{sub}/{}", f.file_name().unwrap().to_string_lossy());
                let _ = write!(out, "({rel:?}, include_str!({f:?})), ");
            }
            out.push_str("],\n");
        }
        out.push_str("    },\n");
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("builtin_packs.rs");
    fs::write(dest, out).unwrap();
}
