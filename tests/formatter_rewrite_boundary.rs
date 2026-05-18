//! Formatter byte edits must go through the transactional rewrite engine.

#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn formatter_byte_replacements_are_engine_owned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let format_dir = root.join("src").join("format");
    let mut files = Vec::new();
    rust_files(&format_dir, &mut files);

    let mut offenders = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        if !text.contains("replace_range") {
            continue;
        }
        let rel = file.strip_prefix(&root).expect("under repo root");
        if rel != Path::new("src/format/rewrite/engine.rs") {
            offenders.push(rel.display().to_string());
        }
    }

    assert!(
        offenders.is_empty(),
        "formatter replace_range calls must stay in src/format/rewrite/engine.rs; offenders: {offenders:?}",
    );
}
