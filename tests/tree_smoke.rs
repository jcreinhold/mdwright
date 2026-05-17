#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Smoke test: every Markdown file in the first available real-doc
//! corpus parses to a non-empty tree without panicking. In Kan this
//! uses `docs/books/gentle-sga/i/`; in the standalone crate it falls
//! back to checked-in test documentation.

use std::fs;
use std::path::PathBuf;

use mdwright::Document;

fn corpus_dirs() -> Vec<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    vec![
        manifest_dir
            .join("..")
            .join("..")
            .join("docs")
            .join("books")
            .join("gentle-sga")
            .join("i"),
        manifest_dir.join("tests").join("gfm-spec"),
    ]
}

#[test]
fn corpus_files_parse_to_non_empty_trees() {
    let dir = corpus_dirs()
        .into_iter()
        .find(|dir| dir.is_dir())
        .expect("at least one Markdown corpus directory should exist");
    let entries = fs::read_dir(&dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    let mut files = 0usize;
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let doc = Document::parse(&src);
        let tree = doc.tree();
        let count = tree.descendants(tree.root()).count();
        assert!(count > 0, "tree for {} had no descendants", path.display());
        // Also exercise raw_text on a sampling of nodes.
        for id in tree.descendants(tree.root()).take(16) {
            let _ = tree.raw_text(doc.source(), id);
        }
        files = files.saturating_add(1);
    }
    assert!(files > 0, "no .md files discovered in {}", dir.display());
}
