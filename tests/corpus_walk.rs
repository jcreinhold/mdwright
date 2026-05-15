//! End-to-end idempotence over the full Kan documentation corpus.
//!
//! Gated on `MDWRIGHT_CORPUS_TEST=1` so the default `cargo test` run
//! stays self-contained — the corpus lives outside the crate and is
//! large enough to slow CI noticeably. The bench manifest at
//! `benches/corpus.list` is the single source of truth; this test
//! mirrors `benches/lint_bench.rs::load_corpus` (lines 50–71).

#![allow(clippy::panic)]

use std::fs;
use std::path::PathBuf;

use mdwright::{Document, FmtOptions};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    // tools/mdwright/ -> tools/ -> repo root
    let mut p = manifest_dir();
    p.pop();
    p.pop();
    p
}

fn corpus_files() -> Vec<PathBuf> {
    let list_path = manifest_dir().join("benches").join("corpus.list");
    let list =
        fs::read_to_string(&list_path).unwrap_or_else(|e| panic!("corpus list {} missing: {e}", list_path.display()));
    let root = repo_root();
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
    let opts = FmtOptions::default();
    let mut failures: Vec<PathBuf> = Vec::new();
    for path in corpus_files() {
        let src =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("corpus file {} unreadable: {e}", path.display()));
        let once = Document::parse(&src).format(&opts);
        let twice = Document::parse(&once).format(&opts);
        if once != twice {
            failures.push(path);
        }
    }
    assert!(failures.is_empty(), "non-idempotent corpus files: {failures:#?}");
}
