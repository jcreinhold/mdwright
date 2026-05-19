#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Smoke test: every Markdown file in the first available real-doc
//! corpus parses to non-empty document facts without panicking. When
//! the external corpus is available it is preferred; otherwise the
//! test falls back to checked-in fixture documentation.

use std::fs;
use std::path::PathBuf;

use mdwright_document::Document;

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
fn corpus_files_parse_to_non_empty_document_facts() {
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
        let doc = Document::parse(&src).expect("fixture parses");
        assert!(
            !doc.block_checkpoints().is_empty(),
            "document for {} had no checkpoints",
            path.display()
        );
        assert!(
            !doc.prose_chunks().is_empty()
                || !doc.code_blocks().is_empty()
                || !doc.headings().is_empty()
                || !doc.html_blocks().is_empty()
                || doc.frontmatter().is_some(),
            "document for {} had no public facts",
            path.display()
        );
        files = files.saturating_add(1);
    }
    assert!(files > 0, "no .md files discovered in {}", dir.display());
}
