//! End-to-end idempotence over the full external documentation corpus.
//!
//! Gated on `MDWRIGHT_CORPUS_TEST=1` so the default `cargo test` run
//! stays self-contained — the corpus lives outside the crate and is
//! large enough to slow CI noticeably. The bench manifest at
//! `benches/corpus.list` is the single source of truth; this test
//! mirrors `benches/lint_bench.rs::load_corpus` (lines 50–71).

#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use mdwright_document::Document;
use mdwright_format::FmtOptions;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    manifest_dir().join("../..")
}

/// Resolve the root of the documentation corpus referenced by `corpus.list`.
///
/// Order: `MDWRIGHT_CORPUS_ROOT` env var, then a sibling `mdwright-corpus`
/// directory next to this checkout. Returns `None` when no candidate is
/// usable so the opt-in test can skip with a clear message rather than
/// panic.
fn corpus_root() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("MDWRIGHT_CORPUS_ROOT") {
        let p = PathBuf::from(v);
        return p.join("docs").join("books").is_dir().then_some(p);
    }
    let sibling = workspace_root().parent()?.join("mdwright-corpus");
    sibling.join("docs").join("books").is_dir().then_some(sibling)
}

fn corpus_files(root: &Path) -> Vec<PathBuf> {
    let list_path = manifest_dir().join("benches").join("corpus.list");
    let list =
        fs::read_to_string(&list_path).unwrap_or_else(|e| panic!("corpus list {} missing: {e}", list_path.display()));
    list.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|rel| root.join(rel))
        .collect()
}

#[test]
fn idempotent_over_corpus() {
    if std::env::var_os("MDWRIGHT_CORPUS_TEST").is_none() {
        eprintln!("skipping; set MDWRIGHT_CORPUS_TEST=1 to enable");
        return;
    }
    let Some(root) = corpus_root() else {
        eprintln!(
            "skipping; set MDWRIGHT_CORPUS_ROOT to a directory containing the \
             corpus paths listed in benches/corpus.list (or place a \
             `mdwright-corpus` checkout next to this crate)",
        );
        return;
    };
    let opts = FmtOptions::default();
    let mut failures: Vec<PathBuf> = Vec::new();
    for path in corpus_files(&root) {
        let src =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("corpus file {} unreadable: {e}", path.display()));
        let once = mdwright_format::format_document(&Document::parse(&src).expect("fixture parses"), &opts);
        let twice = mdwright_format::format_document(&Document::parse(&once).expect("fixture parses"), &opts);
        if once != twice {
            failures.push(path);
        }
    }
    assert!(failures.is_empty(), "non-idempotent corpus files: {failures:#?}");
}
