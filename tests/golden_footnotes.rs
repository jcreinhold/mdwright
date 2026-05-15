#![allow(
    clippy::panic,
    reason = "test fixture loader; missing-file panics are the desired failure mode"
)]

//! Footnote golden tests.
//!
//! Each fixture is an `*.in` / `*.out` pair under
//! `tests/golden_footnotes/`. The `preserve` fixture runs with
//! `[fmt.footnotes] placement = "preserve"`; every other fixture
//! uses default `FmtOptions` (placement = end).

use std::fs;
use std::path::Path;

use mdwright::{Config, Document, FmtOptions};

fn opts_for(stem: &str) -> FmtOptions {
    if stem == "preserve" {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let path = dir.path().join("mdwright.toml");
        std::fs::write(&path, "[fmt.footnotes]\nplacement = \"preserve\"\n")
            .unwrap_or_else(|e| panic!("write: {e}"));
        let cfg = Config::load(Some(&path)).unwrap_or_else(|e| panic!("load config: {e}"));
        cfg.fmt_options().clone()
    } else {
        FmtOptions::default()
    }
}

#[test]
fn golden_footnotes() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden_footnotes");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::path);

    let mut failures: Vec<String> = Vec::new();
    let mut count = 0usize;
    for entry in &entries {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".in") else {
            continue;
        };
        let expected_path = dir.join(format!("{stem}.out"));
        let input =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display()));
        let doc = Document::parse(&input);
        let opts = opts_for(stem);
        let got = doc.format(&opts);
        count = count.saturating_add(1);
        if got != expected {
            failures.push(format!(
                "--- {stem} ---\n--- input ---\n{input}--- expected ---\n{expected}--- got ---\n{got}--- end ---\n"
            ));
        }
    }
    assert!(
        count > 0,
        "no golden fixtures found under {}",
        dir.display()
    );
    assert!(
        failures.is_empty(),
        "{}/{} golden fixture(s) failed:\n{}",
        failures.len(),
        count,
        failures.join("\n")
    );
}
